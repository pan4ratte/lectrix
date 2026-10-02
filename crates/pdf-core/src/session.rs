//! Document sessions: one actor thread per open document.
//!
//! MuPDF documents are not `Send` (ADR 0001), so each open document lives on its own thread
//! that exclusively owns it. Callers talk to it through a channel. Display lists are
//! `Send + Sync`, so rendering happens on the caller's (worker) thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use mupdf::DisplayList;
use mupdf::pdf::PdfDocument;

use crate::error::{Error, Result};
use crate::geometry::{PageGeometry, read_page_boxes};
use crate::render;

/// Size of a page in view space at zoom 1 (points), after rotation and cropping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub path: PathBuf,
    pub page_count: usize,
    pub pages: Vec<PageSize>,
    /// Time to open the file and read every page's geometry.
    pub open_time: Duration,
}

type Reply<T> = SyncSender<Result<T>>;

enum Command {
    DisplayList {
        page: usize,
        reply: Reply<DisplayList>,
    },
    Close,
}

/// Handle to a document actor. Cheap to clone; the actor stops on [`Session::close`] or
/// when every handle is dropped.
#[derive(Clone)]
pub struct Session {
    tx: Sender<Command>,
}

impl Session {
    /// Opens `path` on a new actor thread and returns once the document is loaded.
    pub fn open(path: &Path) -> Result<(Session, DocumentInfo)> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let path_buf = path.to_path_buf();
        thread::Builder::new()
            .name(format!("doc:{}", path.display()))
            .spawn(move || actor(path_buf, rx, ready_tx))?;
        let info = ready_rx.recv().map_err(|_| Error::ActorGone)??;
        Ok((Session { tx }, info))
    }

    /// Builds the display list of `page` on the actor thread.
    pub fn display_list(&self, page: usize) -> Result<DisplayList> {
        let (reply, rx) = mpsc::sync_channel(1);
        self.tx
            .send(Command::DisplayList { page, reply })
            .map_err(|_| Error::ActorGone)?;
        rx.recv().map_err(|_| Error::ActorGone)?
    }

    /// Renders `page` at `scale` to PNG: display list on the actor, raster and encode on
    /// the calling thread.
    pub fn render_png(&self, page: usize, scale: f32) -> Result<render::RenderedPng> {
        let t0 = Instant::now();
        let list = self.display_list(page)?;
        let t1 = Instant::now();
        let pixmap = render::rasterize(&list, scale)?;
        let t2 = Instant::now();
        let png = render::encode_png(&pixmap)?;
        let t3 = Instant::now();
        Ok(render::RenderedPng {
            width: pixmap.width(),
            height: pixmap.height(),
            png,
            timings: render::RenderTimings {
                display_list: t1 - t0,
                raster: t2 - t1,
                encode: t3 - t2,
            },
        })
    }

    pub fn close(&self) {
        // The actor may already be gone; nothing to do then.
        let _ = self.tx.send(Command::Close);
    }
}

fn actor(path: PathBuf, rx: Receiver<Command>, ready: SyncSender<Result<DocumentInfo>>) {
    let start = Instant::now();
    let doc = match load(&path) {
        Ok(doc) => doc,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let info = page_sizes(&doc).map(|pages| DocumentInfo {
        path: path.clone(),
        page_count: pages.len(),
        pages,
        open_time: start.elapsed(),
    });
    let ok = info.is_ok();
    if ready.send(info).is_err() || !ok {
        return;
    }
    while let Ok(command) = rx.recv() {
        match command {
            Command::DisplayList { page, reply } => {
                let _ = reply.send(render::display_list(&doc, page));
            }
            Command::Close => break,
        }
    }
}

fn load(path: &Path) -> Result<PdfDocument> {
    let s = path.to_str().ok_or_else(|| {
        Error::InvalidArgument(format!("path is not valid Unicode: {}", path.display()))
    })?;
    let doc = PdfDocument::open(s).map_err(|e| match e {
        mupdf::Error::InvalidArgument(_) => Error::NotPdf,
        other => Error::MuPdf(other),
    })?;
    Ok(doc)
}

/// Page sizes from the page dictionaries (no content parsing).
fn page_sizes(doc: &PdfDocument) -> Result<Vec<PageSize>> {
    let count = doc.page_count()?;
    let mut pages = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for i in 0..count {
        let obj = doc.find_page(i)?;
        let g = PageGeometry::new(&read_page_boxes(&obj)?);
        pages.push(PageSize {
            width: g.width as f32,
            height: g.height as f32,
        });
    }
    Ok(pages)
}
