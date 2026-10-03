//! The page-image protocol:
//! `folio://page/{docId}/{pageIndex}?scale={s}&rev={r}[&tile={x},{y},{w},{h}][&fmt=png]`.
//!
//! On Windows (WebView2) the same URL is served as `http://folio.localhost/page/...`. The
//! revision is part of the URL, so a URL always means the same pixels.
//!
//! The default response is raw RGBA (`application/octet-stream`, 4 bytes per pixel, size
//! in the `X-Folio-Width` and `X-Folio-Height` headers) that the frontend draws into a
//! `<canvas>` (AGENTS.md section 3). `fmt=png` returns the same cached pixels as PNG;
//! Phase 1 measures both (docs/progress.md).

use std::borrow::Cow;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

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
        // The app page (tauri.localhost or the dev server) fetches from folio.localhost.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "X-Folio-Width, X-Folio-Height, X-Folio-Revision, X-Folio-Timing",
        )
        .header("X-Folio-Width", width)
        .header("X-Folio-Height", height)
        .header("X-Folio-Revision", revision)
        .header("X-Folio-Timing", timing)
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
