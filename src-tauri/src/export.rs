//! Pages exported as images (section 6.14, ADR 0034): a page as it will be exported, for the
//! dialog's preview; where the files go, once the Save dialog has said; and the export
//! itself, with progress and Stop. The work is `pdf_core::export`'s.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_core::export::names::{self, Naming};
use pdf_core::export::{self, Job, Output, encode};
use pdf_core::session::Session;

use crate::ipc::{
    AppError, ErrorCode, ExportDone, ExportPreview, ExportProgress, ExportSettings, ExportTarget,
    FileNaming,
};

/// The most pixels a preview is made with: larger pages are previewed at a lower
/// resolution, and their file's size is estimated.
pub const PREVIEW_MAX_PIXELS: u64 = 16_000_000;

/// Previews kept for the protocol to serve.
const PREVIEWS_KEPT: usize = 4;

/// Progress is reported at most this often (and at the end).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// A preview image: its token, bytes and media type.
type PreviewImage = (u32, Arc<Vec<u8>>, &'static str);

/// What an export, once the Save dialog has said where, writes.
struct Planned {
    settings: ExportSettings,
    pages: Vec<usize>,
    output: Output,
}

#[derive(Default)]
pub struct Export {
    previews: Mutex<VecDeque<PreviewImage>>,
    /// The preview being made of each document, with the flag that stops it once another
    /// is asked for.
    previewing: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    /// Where each document's export goes.
    planned: Mutex<HashMap<u32, Planned>>,
    /// Exports running, by document, with the flag that stops them.
    running: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    /// The first file of each export done, by token, for Show in folder.
    done: Mutex<HashMap<u32, PathBuf>>,
    next_token: AtomicU32,
}

fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
    m.lock().map_err(|_| AppError::bad_state())
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn naming(n: FileNaming) -> Naming {
    match n {
        FileNaming::Numbers => Naming::Numbers,
        FileNaming::Labels => Naming::Labels,
    }
}

impl Export {
    fn token(&self) -> u32 {
        self.next_token.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Page `page` of document `doc` (its `session`) as `settings` export it: drawn,
    /// written and, where the webview can't show the file, read back. A preview asked for
    /// later stops this one, which then fails with the `cancelled` code.
    pub fn preview(
        &self,
        doc: u32,
        session: &Session,
        page: usize,
        settings: &ExportSettings,
    ) -> Result<ExportPreview, AppError> {
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(older) = lock(&self.previewing)?.insert(doc, stop.clone()) {
            older.store(true, Ordering::Relaxed);
        }
        let result = self.make_preview(session, page, settings, &stop);
        if let Ok(mut previewing) = self.previewing.lock()
            && previewing.get(&doc).is_some_and(|s| Arc::ptr_eq(s, &stop))
        {
            previewing.remove(&doc);
        }
        result.map_err(|e| match e {
            pdf_core::Error::Cancelled => AppError::new("A newer preview was asked for.", None)
                .with_code(ErrorCode::Cancelled),
            e => e.into(),
        })
    }

    fn make_preview(
        &self,
        session: &Session,
        page: usize,
        settings: &ExportSettings,
        stop: &AtomicBool,
    ) -> pdf_core::Result<ExportPreview> {
        let options = settings.options();
        options.check()?;
        let facts = session.print_facts()?;
        let size = facts
            .pages
            .get(page)
            .ok_or(pdf_core::Error::PageOutOfRange(page))?;
        // The page as exported must fit its format, whatever the preview's resolution.
        let (width, height) = options.pixel_size(page, size.width, size.height)?;
        let full = u64::from(width) * u64::from(height);
        let mut drawn = options;
        let (shown_w, shown_h) = options.extent(size.width, size.height);
        drawn.dpi = preview_dpi(options.dpi, shown_w, shown_h);
        let list = session.export_list(page, options.annotations)?;
        let raster = export::render(&list, page, &drawn)?;
        drop(list);
        let file = encode::encode(&raster, &drawn, stop)?;
        let shown = u64::from(raster.width) * u64::from(raster.height);
        // A file drawn smaller: its size as the page's pixels would make it.
        let bytes = if shown < full {
            (file.len() as f64 * full as f64 / shown.max(1) as f64).round() as u64
        } else {
            file.len() as u64
        };
        let (image, media) = encode::viewable(&raster, &drawn, file)?;
        let token = self.token();
        if let Ok(mut previews) = self.previews.lock() {
            previews.push_back((token, Arc::new(image), media));
            while previews.len() > PREVIEWS_KEPT {
                previews.pop_front();
            }
        }
        Ok(ExportPreview {
            token,
            width: raster.width,
            height: raster.height,
            bytes,
            dpi: drawn.dpi,
        })
    }

    /// The preview image `token` names, and its media type.
    pub fn preview_image(&self, token: u32) -> Option<(Arc<Vec<u8>>, &'static str)> {
        self.previews
            .lock()
            .ok()?
            .iter()
            .find(|(t, _, _)| *t == token)
            .map(|(_, image, media)| (image.clone(), *media))
    }

    /// Plans document `doc`'s export to `picked` (what the Save dialog gave), from a
    /// document of `page_count` pages with `labels`: the files it writes, and which of them
    /// are there already.
    pub fn plan(
        &self,
        doc: u32,
        settings: ExportSettings,
        picked: &Path,
        page_count: usize,
        labels: Option<&[String]>,
    ) -> Result<ExportTarget, AppError> {
        let format: export::Format = settings.format.into();
        let picked = names::with_extension(picked, format.extension(), format.extensions());
        let mut pages: Vec<usize> = settings.pages.iter().map(|&p| p as usize).collect();
        // An area is of one page.
        if pages.is_empty()
            || pages.iter().any(|&p| p >= page_count)
            || (settings.area.is_some() && pages.len() > 1)
        {
            return Err(AppError::new(
                "Those pages aren't in this document.",
                Some("Check the pages to export."),
            ));
        }
        let output = if settings.one_tiff() {
            Output::OneTiff(picked.clone())
        } else {
            // A page has one file, however often it was named.
            let mut seen = std::collections::HashSet::new();
            pages.retain(|p| seen.insert(*p));
            Output::Files(names::file_names(
                &picked,
                &pages,
                page_count,
                naming(settings.naming),
                labels,
            ))
        };
        let files: Vec<&PathBuf> = match &output {
            Output::OneTiff(path) => vec![path],
            Output::Files(paths) => paths.iter().collect(),
        };
        // The Save dialog asked about the file it named; Lectrix asks about the others.
        let existing: Vec<String> = if files.len() > 1 {
            files
                .iter()
                .filter(|p| p.exists())
                .map(|p| name_of(p))
                .collect()
        } else {
            Vec::new()
        };
        let target = ExportTarget {
            folder: picked
                .parent()
                .map(name_of)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    picked
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                }),
            first: files.first().map(|p| name_of(p)).unwrap_or_default(),
            last: files.last().map(|p| name_of(p)).unwrap_or_default(),
            files: u32::try_from(files.len()).unwrap_or(u32::MAX),
            existing_count: u32::try_from(existing.len()).unwrap_or(u32::MAX),
            existing: existing.into_iter().take(3).collect(),
        };
        lock(&self.planned)?.insert(
            doc,
            Planned {
                settings,
                pages,
                output,
            },
        );
        Ok(target)
    }

    /// The settings of document `doc`'s planned export.
    pub fn planned_settings(&self, doc: u32) -> Option<ExportSettings> {
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

    /// Forgets document `doc`'s planned export, and the previews (the dialog was closed).
    pub fn forget(&self, doc: u32) {
        self.unplan(doc);
        self.drop_previews();
    }

    /// Lets the previews go: a page at 16 million pixels is tens of MB.
    fn drop_previews(&self) {
        if let Ok(mut previews) = self.previews.lock() {
            previews.clear();
        }
    }

    /// Exports document `doc` (its `session`) as planned, leaving out pages whose files are
    /// there if `skip_existing`, reporting progress. Stopped with `stop`, it fails with the
    /// `cancelled` code and nothing is written.
    pub fn run(
        &self,
        doc: u32,
        session: &Session,
        skip_existing: bool,
        progress: impl Fn(ExportProgress),
    ) -> Result<ExportDone, AppError> {
        let Planned {
            settings,
            mut pages,
            mut output,
        } = lock(&self.planned)?.remove(&doc).ok_or_else(|| {
            AppError::new(
                "Lectrix lost track of where to export to.",
                Some("Choose Export again."),
            )
        })?;
        if skip_existing && let Output::Files(paths) = &mut output {
            let keep: Vec<bool> = paths.iter().map(|p| !p.exists()).collect();
            let mut k = keep.iter();
            pages.retain(|_| *k.next().unwrap_or(&true));
            let mut k = keep.iter();
            paths.retain(|_| *k.next().unwrap_or(&true));
        }
        let first = match &output {
            Output::OneTiff(path) => Some(path.clone()),
            Output::Files(paths) => paths.first().cloned(),
        };
        let Some(first) = first else {
            // Every file was there and kept.
            return Ok(ExportDone {
                pages: 0,
                files: 0,
                folder: String::new(),
                first: String::new(),
                token: 0,
            });
        };
        let files = match &output {
            Output::OneTiff(_) => 1,
            Output::Files(paths) => paths.len(),
        };
        let count = pages.len();
        let job = Job {
            pages,
            options: settings.options(),
            output,
        };
        // The dialog closes once the export is done; its previews go now.
        self.drop_previews();
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.running)?.insert(doc, cancel.clone());
        let started = Instant::now();
        let mut last: Option<Instant> = None;
        let result = export::run(session, &job, &cancel, &mut |done, total| {
            if done == total || last.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
                last = Some(Instant::now());
                let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
                progress(ExportProgress {
                    done: n(done),
                    total: n(total),
                });
            }
        });
        if let Ok(mut running) = self.running.lock() {
            running.remove(&doc);
        }
        match result {
            Ok(()) => {
                crate::applog::info(format!(
                    "exported {count} pages as {:?} at {} dpi to {} in {:.2} s",
                    settings.format,
                    settings.dpi,
                    first
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    started.elapsed().as_secs_f64()
                ));
                let token = self.token();
                if let Ok(mut done) = self.done.lock() {
                    done.insert(token, first.clone());
                }
                Ok(ExportDone {
                    pages: u32::try_from(count).unwrap_or(u32::MAX),
                    files: u32::try_from(files).unwrap_or(u32::MAX),
                    folder: first.parent().map(name_of).unwrap_or_default(),
                    first: name_of(&first),
                    token,
                })
            }
            Err(pdf_core::Error::Cancelled) => Err(AppError::new(
                "Exporting was stopped. No file was written.",
                None,
            )
            .with_code(ErrorCode::Cancelled)),
            Err(pdf_core::Error::Io(e)) => {
                crate::applog::warn(format!("export failed: {e:?}"));
                Err(AppError::new(
                    format!(
                        "Lectrix couldn’t write the images in {}.",
                        first.parent().map(name_of).unwrap_or_default()
                    ),
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

    /// The first file of the export `token` names.
    pub fn exported(&self, token: u32) -> Option<PathBuf> {
        self.done.lock().ok()?.get(&token).cloned()
    }
}

/// The resolution a page `width` x `height` points is previewed at: `dpi`, or lower to
/// keep within [`PREVIEW_MAX_PIXELS`].
pub fn preview_dpi(dpi: u16, width: f32, height: f32) -> u16 {
    export::copy::fit_dpi(dpi, width, height, PREVIEW_MAX_PIXELS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_of_large_pages_are_drawn_smaller() {
        // Letter at 300 dpi: 8.4 million pixels, as asked.
        assert_eq!(preview_dpi(300, 612.0, 792.0), 300);
        // At 600 dpi it would be 33.7 million: drawn at the most that fits.
        let dpi = preview_dpi(600, 612.0, 792.0);
        assert!(dpi < 600 && dpi > 400, "{dpi}");
        let px = (612.0 * f64::from(dpi) / 72.0) * (792.0 * f64::from(dpi) / 72.0);
        assert!(px <= PREVIEW_MAX_PIXELS as f64);
    }
}
