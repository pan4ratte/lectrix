//! Compress (section 6.11, ADR 0027): a file picked in a dialog, or the file of a document
//! open in a tab, is compressed into a file of Lectrix's own first (the app's local data,
//! `compress/`); the dialog shows the result and a preview of it, and only then is it saved
//! over the file or as a copy the Save dialog names. Between the steps, the file, its
//! password and the result wait here under a token, so the webview never names a path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_core::compress::{CompressReport, Progress};

use crate::ipc::{
    AppError, CompressOutcome, CompressPresets, CompressProgress, CompressSettings, CompressStage,
    CompressStep,
};
use crate::unprotect::{Picked, Waiting, file_name};

/// Progress is reported at most this often (and always at each stage's end).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// LECTRIX_COMPRESS_IMAGE_DELAY_MS slows compressing down by that much per image, so
/// automated tests can watch the progress and press Stop. For tests only.
fn image_delay() -> Option<Duration> {
    std::env::var("LECTRIX_COMPRESS_IMAGE_DELAY_MS")
        .ok()?
        .parse()
        .ok()
        .map(Duration::from_millis)
}

/// A result waiting to be saved.
#[derive(Clone)]
pub struct Done {
    pub file: PathBuf,
    pub report: CompressReport,
}

pub struct Compress {
    files: Waiting,
    results: Mutex<HashMap<u32, Done>>,
    running: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    /// Where results wait.
    folder: PathBuf,
}

impl Compress {
    pub fn new(folder: PathBuf) -> Self {
        Compress {
            files: Waiting::new(gone),
            results: Mutex::default(),
            running: Mutex::default(),
            folder,
        }
    }

    /// Deletes the results an earlier run left behind (it crashed, or quit with the
    /// dialog open), and, when Lectrix quits, this run's.
    pub fn sweep(&self) {
        if self.folder.exists()
            && let Err(e) = std::fs::remove_dir_all(&self.folder)
        {
            crate::applog::warn(format!(
                "could not delete compressed files in {}: {e}",
                self.folder.display()
            ));
        }
    }

    /// Opens `path` with `password`, reads what its size is made of, and says what comes
    /// next, keeping the file under `token` unless there is nothing more to do.
    pub fn inspect(
        &self,
        token: u32,
        path: PathBuf,
        password: Option<String>,
        remembered: Option<CompressSettings>,
    ) -> CompressStep {
        let name = file_name(&path);
        match pdf_core::compress::analyze(&path, password.as_deref()) {
            Ok(analysis) => {
                self.files.keep(token, Picked { path, password });
                CompressStep::Ready {
                    token,
                    name,
                    signed: analysis.signed,
                    pdfa: analysis.pdfa.map(Into::into),
                    analysis: analysis.into(),
                    presets: Box::new(CompressPresets::new()),
                    remembered: remembered.map(Box::new),
                }
            }
            Err(e @ (pdf_core::Error::PasswordRequired | pdf_core::Error::WrongPassword)) => {
                self.files.keep(
                    token,
                    Picked {
                        path,
                        password: None,
                    },
                );
                CompressStep::NeedsPassword {
                    token,
                    name,
                    retry: matches!(e, pdf_core::Error::WrongPassword),
                }
            }
            Err(e) => {
                crate::applog::warn(format!("could not compress {}: {e:?}", path.display()));
                CompressStep::Failed {
                    name,
                    error: e.into(),
                }
            }
        }
    }

    /// Tries the password the user typed for the file waiting under `token`.
    pub fn with_password(
        &self,
        token: u32,
        password: String,
        remembered: Option<CompressSettings>,
    ) -> Result<CompressStep, AppError> {
        let picked = self.files.take(token)?;
        Ok(self.inspect(token, picked.path, Some(password), remembered))
    }

    /// The file waiting under `token`, and the password that opened it.
    pub fn ready(&self, token: u32) -> Result<(PathBuf, Option<String>), AppError> {
        self.files.ready(token)
    }

    /// Compresses the file waiting under `token` with `settings`, on a thread of its own,
    /// into a file of Lectrix's own, and keeps the result there for `result`. A result
    /// made before for the same token goes. Stopped with `stop`, it fails with the
    /// `cancelled` code and leaves nothing.
    pub fn run(
        &self,
        token: u32,
        settings: CompressSettings,
        progress: impl Fn(CompressProgress) + Send + 'static,
    ) -> Result<CompressOutcome, AppError> {
        let (path, password) = self.ready(token)?;
        let options = settings.to_core();
        pdf_core::compress::validate(&options)?;
        self.discard_result(token);
        std::fs::create_dir_all(&self.folder).map_err(pdf_core::Error::from)?;
        let target = self.folder.join(format!("{token}.pdf"));
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.running)?.insert(token, cancel.clone());
        let thread_target = target.clone();
        let started = Instant::now();
        let result = std::thread::Builder::new()
            .name("compress".into())
            .spawn(move || {
                let delay = image_delay();
                let mut last: Option<Instant> = None;
                pdf_core::compress::compress_file(
                    &path,
                    password.as_deref(),
                    &options,
                    &thread_target,
                    &mut |p| {
                        if let (Some(delay), Progress::Images { .. }) = (delay, p) {
                            std::thread::sleep(delay);
                        }
                        if cancel.load(Ordering::Relaxed) {
                            return false;
                        }
                        let (message, last_of_stage) = message(p);
                        if last_of_stage || last.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
                            last = Some(Instant::now());
                            progress(message);
                        }
                        true
                    },
                )
            })
            .map_err(pdf_core::Error::from)?
            .join()
            .map_err(|_| AppError::from(pdf_core::Error::ActorGone))?;
        if let Ok(mut running) = self.running.lock() {
            running.remove(&token);
        }
        let report = match result {
            Ok(report) => report,
            Err(e) => {
                let _ = std::fs::remove_file(&target);
                if !matches!(e, pdf_core::Error::Cancelled) {
                    crate::applog::warn(format!("compressing failed: {e:?}"));
                }
                return Err(e.into());
            }
        };
        crate::applog::info(format!(
            "compressed {} bytes to {} in {:.2} s ({:?})",
            report.before,
            report.after,
            started.elapsed().as_secs_f64(),
            report.images
        ));
        lock(&self.results)?.insert(
            token,
            Done {
                file: target,
                report,
            },
        );
        Ok(outcome(&report))
    }

    /// Stops compressing the file waiting under `token`.
    pub fn stop(&self, token: u32) {
        if let Ok(running) = self.running.lock()
            && let Some(cancel) = running.get(&token)
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// The result made for `token`.
    pub fn result(&self, token: u32) -> Result<Done, AppError> {
        lock(&self.results)?.get(&token).cloned().ok_or_else(|| {
            AppError::new(
                "That compressed file is no longer waiting to be saved.",
                Some("Choose File > Compress… again."),
            )
        })
    }

    fn discard_result(&self, token: u32) {
        if let Ok(mut results) = self.results.lock()
            && let Some(done) = results.remove(&token)
        {
            let _ = std::fs::remove_file(&done.file);
        }
    }

    /// Forgets the file waiting under `token`, and deletes its result.
    pub fn cancel(&self, token: u32) {
        self.stop(token);
        self.discard_result(token);
        self.files.cancel(token);
    }
}

/// The progress message for `p`, and whether it ends its stage.
fn message(p: Progress) -> (CompressProgress, bool) {
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    let (stage, done, total) = match p {
        Progress::Reading { done, total } => (CompressStage::Reading, done, total),
        Progress::Images { done, total } => (CompressStage::Images, done, total),
        Progress::Fonts => (CompressStage::Fonts, 0, 0),
        Progress::Writing => (CompressStage::Writing, 0, 0),
    };
    (
        CompressProgress {
            stage,
            done: n(done),
            total: n(total),
        },
        done == total,
    )
}

fn outcome(report: &CompressReport) -> CompressOutcome {
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    let r = report.removed;
    CompressOutcome {
        before: report.before,
        after: report.after,
        images: n(report.images.total),
        images_replaced: n(report.images.replaced),
        images_downsampled: n(report.images.downsampled),
        images_skipped: n(report.images.skipped),
        fonts_subset: n(report.fonts_subset),
        removed: crate::ipc::CompressFound {
            thumbnails: n(r.thumbnails),
            private_data: n(r.private_data),
            attachments: n(r.attachments),
            javascript: n(r.javascript),
            tags: r.tags,
        },
        preview_page: n(report.images.preview_page.unwrap_or(0)),
        pages: n(report.pages),
    }
}

/// Where results wait: beside the recovery copies in the app's local data, or, without
/// them (measurement and test runs), in a folder of this process in the system's temporary
/// files.
pub fn folder(recovery: Option<&Path>) -> PathBuf {
    match recovery.and_then(Path::parent) {
        Some(data) => data.join("compress"),
        None => std::env::temp_dir().join(format!("lectrix-compress-{}", std::process::id())),
    }
}

fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
    m.lock().map_err(|_| AppError::bad_state())
}

fn gone() -> AppError {
    AppError::new(
        "That file is no longer waiting to be compressed.",
        Some("Choose File > Compress… again."),
    )
}
