//! The images in a document exported (section 6.14, ADR 0035): finding them on the pages
//! chosen, a few pages at a time; where their files go, once the Save dialog has said; and
//! the export itself, with progress and Stop. The work is `pdf_core::export::embedded`'s.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_core::export::embedded::{self, ImageFile, PageImage, Plan};
use pdf_core::export::names::{self, ImageName, Naming};
use pdf_core::session::Session;

use crate::export::{name_of, target_of};
use crate::ipc::{
    AppError, ErrorCode, ExportProgress, ExportTarget, FileNaming, FoundImage, ImagesDone,
    ImagesFound, ImagesSettings,
};

/// Pages read at a time while looking for images, so that the document keeps rendering.
const CHUNK: usize = 8;

/// Progress is reported at most this often (and at the end).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// The images found on some pages of a document.
struct Found {
    pages: Vec<usize>,
    images: Vec<PageImage>,
}

/// Where a document's images go, once the Save dialog has said.
struct Planned {
    settings: ImagesSettings,
    files: Vec<ImageFile>,
}

#[derive(Default)]
pub struct Images {
    /// The images found last, by document.
    found: Mutex<HashMap<u32, Arc<Found>>>,
    /// The search for images running, by document, with the flag that stops it once
    /// another is asked for.
    finding: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    planned: Mutex<HashMap<u32, Planned>>,
    /// Exports running, by document, with the flag that stops them.
    running: Mutex<HashMap<u32, Arc<AtomicBool>>>,
}

fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
    m.lock().map_err(|_| AppError::bad_state())
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn naming(n: FileNaming) -> Naming {
    match n {
        FileNaming::Numbers => Naming::Numbers,
        FileNaming::Labels => Naming::Labels,
    }
}

fn names_of(plan: &Plan) -> Vec<ImageName<'static>> {
    plan.images
        .iter()
        .map(|i| ImageName {
            page: i.page,
            index: i.index,
            extension: i.kind.extension(),
        })
        .collect()
}

impl Images {
    /// The images on `settings`' pages of document `doc` (its `session`), each once, as
    /// they are exported; `progress` is told how many pages were read. Found again only
    /// for other pages. A search asked for later stops this one (the `cancelled` code).
    pub fn find(
        &self,
        doc: u32,
        session: &Session,
        settings: &ImagesSettings,
        progress: impl Fn(ExportProgress),
    ) -> Result<ImagesFound, AppError> {
        let found = self.found_on(doc, session, settings, progress)?;
        let plan = embedded::plan(&found.images, settings.min_side());
        Ok(ImagesFound {
            images: plan
                .images
                .iter()
                .map(|i| FoundImage {
                    page: count(i.page),
                    index: count(i.index),
                    num: i.num,
                    key: format!("{:032x}", i.key),
                    width: i.width,
                    height: i.height,
                    kind: i.kind.into(),
                    bytes: i.bytes,
                })
                .collect(),
            repeats: count(plan.repeats),
            small: count(plan.small),
        })
    }

    fn found_on(
        &self,
        doc: u32,
        session: &Session,
        settings: &ImagesSettings,
        progress: impl Fn(ExportProgress),
    ) -> Result<Arc<Found>, AppError> {
        let pages: Vec<usize> = settings.pages.iter().map(|&p| p as usize).collect();
        if let Some(found) = lock(&self.found)?.get(&doc)
            && found.pages == pages
        {
            return Ok(found.clone());
        }
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(older) = lock(&self.finding)?.insert(doc, stop.clone()) {
            older.store(true, Ordering::Relaxed);
        }
        let result = (|| -> Result<Arc<Found>, AppError> {
            let mut images = Vec::new();
            let mut last: Option<Instant> = None;
            for (n, chunk) in pages.chunks(CHUNK).enumerate() {
                if stop.load(Ordering::Relaxed) {
                    return Err(
                        AppError::new("A newer search for images was asked for.", None)
                            .with_code(ErrorCode::Cancelled),
                    );
                }
                images.extend(session.page_images(chunk.to_vec())?);
                let done = (n * CHUNK + chunk.len()).min(pages.len());
                if done == pages.len() || last.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
                    last = Some(Instant::now());
                    progress(ExportProgress {
                        done: count(done),
                        total: count(pages.len()),
                    });
                }
            }
            Ok(Arc::new(Found { pages, images }))
        })();
        if let Ok(mut finding) = self.finding.lock()
            && finding.get(&doc).is_some_and(|s| Arc::ptr_eq(s, &stop))
        {
            finding.remove(&doc);
        }
        let found = result?;
        lock(&self.found)?.insert(doc, found.clone());
        Ok(found)
    }

    /// The plan for `settings`' pages (found again if they aren't the pages found last),
    /// and the name the Save dialog suggests for it, from a document named `stem` of
    /// `page_count` pages with `labels`.
    pub fn suggestion(
        &self,
        doc: u32,
        session: &Session,
        settings: &ImagesSettings,
        stem: &str,
        page_count: usize,
        labels: Option<&[String]>,
    ) -> Result<(Plan, String), AppError> {
        let found = self.found_on(doc, session, settings, |_| {})?;
        let plan = embedded::plan(&found.images, settings.min_side());
        if plan.images.is_empty() {
            return Err(AppError::new(
                "There are no images to export on these pages.",
                Some("Choose other pages, or export smaller images too."),
            ));
        }
        let suggested = names::suggested_for_images(
            stem,
            &names_of(&plan),
            page_count,
            naming(settings.naming),
            labels,
        );
        Ok((plan, suggested))
    }

    /// Plans document `doc`'s images (`plan`) to `picked`, what the Save dialog gave: the
    /// files written, and which of them are there already.
    pub fn plan(
        &self,
        doc: u32,
        settings: ImagesSettings,
        plan: &Plan,
        picked: &Path,
        page_count: usize,
        labels: Option<&[String]>,
    ) -> Result<ExportTarget, AppError> {
        let paths = names::image_file_names(
            picked,
            &names_of(plan),
            page_count,
            naming(settings.naming),
            labels,
        );
        let files: Vec<ImageFile> = plan
            .images
            .iter()
            .zip(paths)
            .map(|(i, path)| ImageFile {
                num: i.num,
                kind: i.kind,
                path,
            })
            .collect();
        let paths: Vec<&PathBuf> = files.iter().map(|f| &f.path).collect();
        let target = target_of(picked, &paths);
        lock(&self.planned)?.insert(doc, Planned { settings, files });
        Ok(target)
    }

    /// The settings of document `doc`'s planned export.
    pub fn planned_settings(&self, doc: u32) -> Option<ImagesSettings> {
        self.planned
            .lock()
            .ok()?
            .get(&doc)
            .map(|p| p.settings.clone())
    }

    /// Forgets document `doc`'s planned export (the Save dialog was given up).
    pub fn unplan(&self, doc: u32) {
        if let Ok(mut planned) = self.planned.lock() {
            planned.remove(&doc);
        }
    }

    /// Forgets what was found and planned for document `doc`, and stops a search (the
    /// dialog was closed).
    pub fn forget(&self, doc: u32) {
        self.unplan(doc);
        if let Ok(mut found) = self.found.lock() {
            found.remove(&doc);
        }
        if let Ok(finding) = self.finding.lock()
            && let Some(stop) = finding.get(&doc)
        {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// Exports document `doc`'s images (its `session`) as planned, leaving out those whose
    /// files are there if `skip_existing`, reporting progress. Stopped with `stop`, it
    /// fails with the `cancelled` code and nothing is written. `remember` keeps the first
    /// file for Show in folder and gives its token.
    pub fn run(
        &self,
        doc: u32,
        session: &Session,
        skip_existing: bool,
        progress: impl Fn(ExportProgress) + Send + 'static,
        remember: impl FnOnce(PathBuf) -> u32,
    ) -> Result<ImagesDone, AppError> {
        let Planned { mut files, .. } = lock(&self.planned)?.remove(&doc).ok_or_else(|| {
            AppError::new(
                "Lectrix lost track of where to export to.",
                Some("Choose Export again."),
            )
        })?;
        if skip_existing {
            files.retain(|f| !f.path.exists());
        }
        let Some(first) = files.first().map(|f| f.path.clone()) else {
            // Every file was there and kept.
            return Ok(ImagesDone {
                files: 0,
                unreadable: 0,
                folder: String::new(),
                first: String::new(),
                token: 0,
            });
        };
        let folder = first.parent().map(name_of).unwrap_or_default();
        let total = files.len();
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.running)?.insert(doc, cancel.clone());
        let started = Instant::now();
        let last: Mutex<Option<Instant>> = Mutex::new(None);
        let result = session.write_images(files.clone(), cancel, move |done, total| {
            if let Ok(mut last) = last.lock()
                && (done == total || last.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL))
            {
                *last = Some(Instant::now());
                progress(ExportProgress {
                    done: count(done),
                    total: count(total),
                });
            }
        });
        if let Ok(mut running) = self.running.lock() {
            running.remove(&doc);
        }
        match result {
            Ok(written) => {
                crate::applog::info(format!(
                    "exported {} of {total} images to {} in {:.2} s ({} unreadable)",
                    written.files,
                    first
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    started.elapsed().as_secs_f64(),
                    written.unreadable
                ));
                // The first file written: an unreadable image has none.
                let first = files
                    .iter()
                    .map(|f| &f.path)
                    .find(|p| p.exists())
                    .cloned()
                    .unwrap_or(first);
                Ok(ImagesDone {
                    files: count(written.files),
                    unreadable: count(written.unreadable),
                    folder,
                    first: name_of(&first),
                    token: remember(first),
                })
            }
            Err(pdf_core::Error::Cancelled) => Err(AppError::new(
                "Exporting was stopped. No file was written.",
                None,
            )
            .with_code(ErrorCode::Cancelled)),
            Err(pdf_core::Error::Io(e)) => {
                crate::applog::warn(format!("image export failed: {e:?}"));
                Err(AppError::new(
                    format!("Lectrix couldn’t write the images in {folder}."),
                    Some(
                        "Check that the drive has room and that you may save files there, then try again, or pick another folder.",
                    ),
                ))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Stops document `doc`'s export.
    pub fn stop(&self, doc: u32) {
        if let Ok(running) = self.running.lock()
            && let Some(cancel) = running.get(&doc)
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
