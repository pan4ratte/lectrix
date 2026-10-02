//! The page-image protocol: `folio://page/{docId}/{pageIndex}?scale={s}&rev={r}`.
//!
//! On Windows (WebView2) the same URL is served as
//! `http://folio.localhost/page/{docId}/{pageIndex}?...`. The revision is part of the URL
//! so that cached images are invalidated automatically when the document changes.

use std::borrow::Cow;

use tauri::http::{Request, Response, StatusCode, header};

use crate::AppState;

pub fn handle(state: &AppState, request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    match render(state, request) {
        Ok(response) => response,
        Err((status, message)) => error_response(status, message),
    }
}

type Failure = (StatusCode, &'static str);

fn render(
    state: &AppState,
    request: &Request<Vec<u8>>,
) -> Result<Response<Cow<'static, [u8]>>, Failure> {
    let uri = request.uri();
    let mut segments = uri.path().trim_matches('/').split('/');
    let (Some("page"), Some(doc), Some(page), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return Err((StatusCode::NOT_FOUND, "unknown resource"));
    };
    let doc: u32 = doc
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "bad document id"))?;
    let page: usize = page
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "bad page index"))?;
    let mut scale = 1.0f32;
    for pair in uri.query().unwrap_or("").split('&') {
        if let Some(value) = pair.strip_prefix("scale=") {
            scale = value
                .parse()
                .map_err(|_| (StatusCode::BAD_REQUEST, "bad scale"))?;
        }
    }

    let session = state
        .session(doc)
        .ok_or((StatusCode::NOT_FOUND, "document not open"))?;
    let rendered = session.render_png(page, scale).map_err(|e| {
        eprintln!("[folio] render doc {doc} page {page}: {e:?}");
        (StatusCode::INTERNAL_SERVER_ERROR, "render failed")
    })?;
    let t = rendered.timings;
    let timing = format!(
        "dl={:.2};raster={:.2};encode={:.2}",
        t.display_list.as_secs_f64() * 1000.0,
        t.raster.as_secs_f64() * 1000.0,
        t.encode.as_secs_f64() * 1000.0
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        // The URL includes the revision, so a given URL never changes.
        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
        // The app page (tauri.localhost or the dev server) fetches from folio.localhost.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_EXPOSE_HEADERS, "X-Folio-Timing")
        .header("X-Folio-Timing", timing)
        .body(Cow::Owned(rendered.png))
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "response failed"))
}

fn error_response(status: StatusCode, message: &'static str) -> Response<Cow<'static, [u8]>> {
    let mut response = Response::new(Cow::Borrowed(message.as_bytes()));
    *response.status_mut() = status;
    response
}
