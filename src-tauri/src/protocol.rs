//! The page-image protocol:
//! `lectrix://page/{docId}/{pageIndex}?scale={s}&rev={r}[&tile={x},{y},{w},{h}][&fmt=png]`,
//! and previews of recent files: `lectrix://recent/{index}?w={px}&opened={ms}`.
//!
//! On Windows (WebView2) the same URL is served as `http://lectrix.localhost/page/...`. The
//! revision is part of the URL, so a URL always means the same pixels.
//!
//! Without `fmt`, the response is raw RGBA (`application/octet-stream`, 4 bytes per pixel,
//! size in the `X-Lectrix-Width` and `X-Lectrix-Height` headers). `fmt=png` returns the same
//! cached pixels as PNG, which the app uses by default because it reaches the screen about
//! three times faster through WebView2 (ADR 0004).

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Instant, SystemTime};

use pdf_core::render::{ImageCache, ImageKey, PixelRect, RgbaImage, TILE_SIZE};
use tauri::http::{Request, Response, StatusCode, header};

use crate::AppState;

type Body = Cow<'static, [u8]>;
type Failure = (StatusCode, &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Rgba,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PageRequest {
    doc: u32,
    page: usize,
    scale: f32,
    revision: u64,
    tile: Option<PixelRect>,
    format: Format,
}

fn parse(path: &str, query: Option<&str>) -> Result<PageRequest, Failure> {
    let mut segments = path.trim_matches('/').split('/');
    let (Some("page"), Some(doc), Some(page), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return Err((StatusCode::NOT_FOUND, "unknown resource"));
    };
    let mut request = PageRequest {
        doc: doc
            .parse()
            .map_err(|_| (StatusCode::BAD_REQUEST, "bad document id"))?,
        page: page
            .parse()
            .map_err(|_| (StatusCode::BAD_REQUEST, "bad page index"))?,
        scale: 1.0,
        revision: 0,
        tile: None,
        format: Format::Rgba,
    };
    for pair in query.unwrap_or("").split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "scale" => {
                request.scale = value
                    .parse()
                    .ok()
                    .filter(|s: &f32| {
                        s.is_finite() && *s > 0.0 && *s <= pdf_core::render::MAX_SCALE
                    })
                    .ok_or((StatusCode::BAD_REQUEST, "bad scale"))?;
            }
            "rev" => {
                request.revision = value
                    .parse()
                    .map_err(|_| (StatusCode::BAD_REQUEST, "bad revision"))?;
            }
            "tile" => {
                let parts: Vec<u32> = value
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| (StatusCode::BAD_REQUEST, "bad tile"))?;
                let [x, y, width, height] = parts[..] else {
                    return Err((StatusCode::BAD_REQUEST, "bad tile"));
                };
                if width == 0 || height == 0 || width > TILE_SIZE || height > TILE_SIZE {
                    return Err((StatusCode::BAD_REQUEST, "bad tile size"));
                }
                request.tile = Some(PixelRect {
                    x,
                    y,
                    width,
                    height,
                });
            }
            "fmt" => {
                request.format = match value {
                    "png" => Format::Png,
                    "rgba" => Format::Rgba,
                    _ => return Err((StatusCode::BAD_REQUEST, "bad format")),
                };
            }
            _ => {}
        }
    }
    Ok(request)
}

/// Limits how many pages render at once. Rendering is CPU-bound; more parallel renders
/// than cores only add latency to the page the user is looking at.
pub struct RenderGate {
    free: Mutex<usize>,
    available: Condvar,
}

impl RenderGate {
    pub fn new() -> Self {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        RenderGate {
            free: Mutex::new(cores.saturating_sub(1).clamp(2, 4)),
            available: Condvar::new(),
        }
    }

    fn run<T>(&self, f: impl FnOnce() -> T) -> T {
        {
            let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
            while *free == 0 {
                free = self.available.wait(free).unwrap_or_else(|e| e.into_inner());
            }
            *free -= 1;
        }
        let result = f();
        *self.free.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.available.notify_one();
        result
    }
}

pub fn handle(state: &AppState, request: &Request<Vec<u8>>) -> Response<Body> {
    match serve(state, request) {
        Ok(response) => response,
        Err((status, message)) => error_response(status, message),
    }
}

fn serve(state: &AppState, request: &Request<Vec<u8>>) -> Result<Response<Body>, Failure> {
    let uri = request.uri();
    if uri.path().starts_with("/recent/") {
        return serve_preview(state, parse_preview(uri.path(), uri.query())?);
    }
    let req = parse(uri.path(), uri.query())?;
    let session = state
        .documents
        .session(req.doc)
        .map_err(|_| (StatusCode::NOT_FOUND, "document not open"))?;
    let t0 = Instant::now();

    let key = ImageKey {
        doc: req.doc,
        page: req.page,
        revision: req.revision,
        scale_milli: ImageKey::scale_milli(req.scale),
        tile: req.tile,
    };
    let (image, cached) = match state.image_cache.get(&key) {
        Some(image) => (image, true),
        None => {
            let (image, revision) = state
                .render_gate
                .run(|| session.render_rgba(req.page, req.scale, req.tile))
                .map_err(|e| render_failed(&req, e))?;
            let image = Arc::new(image);
            // Key by the revision actually rendered: if the document changed after the
            // request was made, these pixels belong to the newer revision.
            insert(&state.image_cache, ImageKey { revision, ..key }, &image);
            (image, false)
        }
    };
    let render_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let (content_type, body, encode_ms) = match req.format {
        Format::Rgba => ("application/octet-stream", image.data.clone(), 0.0),
        Format::Png => {
            let t1 = Instant::now();
            let png =
                pdf_core::render::encode_rgba_png(&image).map_err(|e| render_failed(&req, e))?;
            ("image/png", png, t1.elapsed().as_secs_f64() * 1000.0)
        }
    };
    let timing = format!(
        "render={render_ms:.2};encode={encode_ms:.2};cached={}",
        u8::from(cached)
    );
    respond(
        content_type,
        image.width,
        image.height,
        req.revision,
        &timing,
        body,
    )
}

/// A preview of a recent file: the entry at `index` in the recent list, which must still
/// be the one opened at `opened` (ms since the Unix epoch), so a URL never shows another
/// file after the list changes. The webview names no path (section 2, security).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewRequest {
    index: usize,
    width: u32,
    opened: u64,
}

fn parse_preview(path: &str, query: Option<&str>) -> Result<PreviewRequest, Failure> {
    let mut segments = path.trim_matches('/').split('/');
    let (Some("recent"), Some(index), None) = (segments.next(), segments.next(), segments.next())
    else {
        return Err((StatusCode::NOT_FOUND, "unknown resource"));
    };
    let index = index
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "bad recent index"))?;
    let (mut width, mut opened) = (None, None);
    for pair in query.unwrap_or("").split('&') {
        match pair.split_once('=') {
            Some(("w", v)) => width = v.parse().ok().filter(|w| (1..=2 * TILE_SIZE).contains(w)),
            Some(("opened", v)) => opened = v.parse().ok(),
            _ => {}
        }
    }
    Ok(PreviewRequest {
        index,
        width: width.ok_or((StatusCode::BAD_REQUEST, "bad preview width"))?,
        opened: opened.ok_or((StatusCode::BAD_REQUEST, "bad opened time"))?,
    })
}

/// How many previews are kept; the recent list holds 20 files, and a screen's pixel ratio
/// can change the width asked for.
const MAX_PREVIEWS: usize = 64;

/// First-page previews of recent files as PNG, keyed by path and width. An entry is used
/// only while the file's size and modification time are the ones it was made from.
#[derive(Default)]
pub struct PreviewCache {
    entries: Mutex<HashMap<(PathBuf, u32), Preview>>,
}

struct Preview {
    stamp: (u64, Option<SystemTime>),
    width: u32,
    height: u32,
    png: Arc<Vec<u8>>,
}

fn serve_preview(state: &AppState, req: PreviewRequest) -> Result<Response<Body>, Failure> {
    let not_found = (StatusCode::NOT_FOUND, "no such recent file");
    let path = {
        let store = state
            .store
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "state unavailable"))?;
        let entry = store.recent().get(req.index).ok_or(not_found)?;
        if entry.opened_at.saturating_mul(1000) != req.opened {
            return Err(not_found);
        }
        entry.path.clone()
    };
    let meta = std::fs::metadata(&path)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .ok_or(not_found)?;
    let stamp = (meta.len(), meta.modified().ok());
    let key = (path, req.width);
    let cached = state.previews.entries.lock().ok().and_then(|entries| {
        entries
            .get(&key)
            .filter(|p| p.stamp == stamp)
            .map(|p| (p.width, p.height, p.png.clone()))
    });
    let t0 = Instant::now();
    let (width, height, png, was_cached) = match cached {
        Some((w, h, png)) => (w, h, png, true),
        None => {
            let image = state
                .render_gate
                .run(|| pdf_core::render::first_page_preview(&key.0, req.width))
                .map_err(preview_failed)?;
            let png = Arc::new(pdf_core::render::encode_rgba_png(&image).map_err(preview_failed)?);
            if let Ok(mut entries) = state.previews.entries.lock() {
                if entries.len() >= MAX_PREVIEWS {
                    entries.clear();
                }
                entries.insert(
                    key,
                    Preview {
                        stamp,
                        width: image.width,
                        height: image.height,
                        png: png.clone(),
                    },
                );
            }
            (image.width, image.height, png, false)
        }
    };
    let timing = format!(
        "render={:.2};cached={}",
        t0.elapsed().as_secs_f64() * 1000.0,
        u8::from(was_cached)
    );
    respond("image/png", width, height, 0, &timing, png.as_ref().clone())
}

/// A file that needs a password or cannot be read has no preview; the start screen shows
/// its icon instead. Only unexpected failures are logged.
fn preview_failed(e: pdf_core::Error) -> Failure {
    match e {
        pdf_core::Error::PasswordRequired => (StatusCode::FORBIDDEN, "needs a password"),
        e => {
            crate::applog::warn(format!("preview of a recent file: {e:?}"));
            (StatusCode::INTERNAL_SERVER_ERROR, "preview failed")
        }
    }
}

fn insert(cache: &ImageCache, key: ImageKey, image: &Arc<RgbaImage>) {
    cache.insert(key, image.clone());
}

fn render_failed(req: &PageRequest, e: pdf_core::Error) -> Failure {
    crate::applog::warn(format!(
        "render doc {} page {} scale {}: {e:?}",
        req.doc, req.page, req.scale
    ));
    match e {
        pdf_core::Error::PageOutOfRange(_) => (StatusCode::NOT_FOUND, "no such page"),
        pdf_core::Error::InvalidArgument(_) => (StatusCode::BAD_REQUEST, "bad request"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "render failed"),
    }
}

fn respond(
    content_type: &str,
    width: u32,
    height: u32,
    revision: u64,
    timing: &str,
    body: Vec<u8>,
) -> Result<Response<Body>, Failure> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        // Rendered pixels are cached in Rust (`ImageCache`) and drawn into canvases; a
        // browser cache would only hold a second copy of every image.
        .header(header::CACHE_CONTROL, "no-store")
        // The app page (tauri.localhost or the dev server) fetches from lectrix.localhost.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "X-Lectrix-Width, X-Lectrix-Height, X-Lectrix-Revision, X-Lectrix-Timing",
        )
        .header("X-Lectrix-Width", width)
        .header("X-Lectrix-Height", height)
        .header("X-Lectrix-Revision", revision)
        .header("X-Lectrix-Timing", timing)
        .body(Cow::Owned(body))
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "response failed"))
}

fn error_response(status: StatusCode, message: &'static str) -> Response<Body> {
    let mut response = Response::new(Cow::Borrowed(message.as_bytes()));
    *response.status_mut() = status;
    if let Ok(origin) = "*".parse() {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_whole_pages_tiles_and_formats() {
        let r = parse("/page/3/41", Some("scale=1.500&rev=7")).unwrap();
        assert_eq!((r.doc, r.page, r.scale, r.revision), (3, 41, 1.5, 7));
        assert_eq!(r.tile, None);
        assert_eq!(r.format, Format::Rgba);

        let r = parse(
            "/page/1/0",
            Some("scale=4&rev=0&tile=512,1024,512,300&fmt=png"),
        )
        .unwrap();
        assert_eq!(
            r.tile,
            Some(PixelRect {
                x: 512,
                y: 1024,
                width: 512,
                height: 300
            })
        );
        assert_eq!(r.format, Format::Png);
    }

    #[test]
    fn parses_previews_of_recent_files() {
        let r = parse_preview("/recent/3", Some("w=240&opened=1791315108000")).unwrap();
        assert_eq!(
            r,
            PreviewRequest {
                index: 3,
                width: 240,
                opened: 1_791_315_108_000
            }
        );
        assert!(parse_preview("/recent/x", Some("w=240&opened=1")).is_err());
        assert!(parse_preview("/recent/1/2", Some("w=240&opened=1")).is_err());
        assert!(parse_preview("/recent/1", Some("opened=1")).is_err());
        assert!(parse_preview("/recent/1", Some("w=0&opened=1")).is_err());
        assert!(parse_preview("/recent/1", Some("w=5000&opened=1")).is_err());
        assert!(parse_preview("/recent/1", Some("w=240")).is_err());
    }

    #[test]
    fn rejects_malformed_requests() {
        assert!(parse("/thumb/1/0", None).is_err());
        assert!(parse("/page/x/0", None).is_err());
        assert!(parse("/page/1/0/extra", None).is_err());
        assert!(parse("/page/1/0", Some("scale=0")).is_err());
        assert!(parse("/page/1/0", Some("scale=NaN")).is_err());
        assert!(parse("/page/1/0", Some("scale=1000")).is_err());
        assert!(parse("/page/1/0", Some("tile=0,0,0,10")).is_err());
        assert!(parse("/page/1/0", Some("tile=0,0,4096,10")).is_err());
        assert!(parse("/page/1/0", Some("tile=1,2,3")).is_err());
        assert!(parse("/page/1/0", Some("fmt=jpeg")).is_err());
    }
}
