//! Combining files (AGENTS.md section 6.4), and extracting pages of an open document
//! into a new file ([`extract`]), which reports progress and is cancelled the same way.
//!
//! The merge runs on a thread of its own: MuPDF documents stay on the thread that opened
//! them, so the sources are opened again there, from their files as saved. Progress is
//! reported as pages are copied. Cancelling stops at the next page, or after writing but
//! before the new file replaces anything, so a cancelled merge never leaves a file behind.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pdf_core::merge::{self, MergeOptions, MergeReport, MergeSource, PagePick};
use pdf_core::save::{SaveKind, save_atomic_checked};
use pdf_core::session::{ExtractStep, Session};
use pdf_core::{Error, Result};

/// Progress is reported at most this often (and always for the last page).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// LECTRIX_COMBINE_PAGE_DELAY_MS slows combining down by that much per page, so automated
/// tests can watch the progress bar and press Stop (combining is usually too fast for
/// that). For tests only, like LECTRIX_PERF.
fn page_delay() -> Option<Duration> {
    std::env::var("LECTRIX_COMBINE_PAGE_DELAY_MS")
        .ok()?
        .parse()
        .ok()
        .map(Duration::from_millis)
}

pub struct Job {
    /// Each source's file and password.
    pub sources: Vec<(PathBuf, Option<String>)>,
    pub picks: Vec<PagePick>,
    pub options: MergeOptions,
    pub target: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Copying {
        done: usize,
        total: usize,
    },
    /// All pages are copied; the file is being written.
    Writing,
}

/// Combines on a new thread and waits for it.
pub fn run(
    job: Job,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Progress) + Send + 'static,
) -> Result<MergeReport> {
    std::thread::Builder::new()
        .name("combine".into())
        .spawn(move || run_here(&job, &cancel, &progress))?
        .join()
        .map_err(|_| Error::ActorGone)?
}

fn run_here(job: &Job, cancel: &AtomicBool, progress: &dyn Fn(Progress)) -> Result<MergeReport> {
    let sources = job
        .sources
        .iter()
        .map(|(path, password)| MergeSource::open(path, password.as_deref()))
        .collect::<Result<Vec<_>>>()?;
    let mut last: Option<Instant> = None;
    let delay = page_delay();
    let (doc, report) = merge::merge(&sources, &job.picks, job.options, &mut |done, total| {
        if let Some(delay) = delay {
            std::thread::sleep(delay);
        }
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        if done == total || last.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
            progress(Progress::Copying { done, total });
            last = Some(Instant::now());
        }
        true
    })?;
    progress(Progress::Writing);
    // Copied streams keep their compression; nothing is left unreferenced, so a plain full
    // save is enough (and much faster than an optimizing one on large scans).
    save_atomic_checked(&doc, SaveKind::Full, None, &job.target, &mut || {
        if cancel.load(Ordering::Relaxed) {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    })?;
    Ok(report)
}

/// Copies `pages` of the document in `session` into a new file at `target`, as the
/// document is now. It runs on the document's own thread, where its MuPDF document lives;
/// this waits for it. Cancelling stops at the next page, or before the new file takes its
/// name.
pub fn extract(
    session: &Session,
    pages: Vec<usize>,
    target: &Path,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Progress) + Send + 'static,
) -> Result<MergeReport> {
    let mut last: Option<Instant> = None;
    let delay = page_delay();
    session.extract(pages, target, move |step| {
        if let (ExtractStep::Copying { .. }, Some(delay)) = (step, delay) {
            std::thread::sleep(delay);
        }
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        match step {
            ExtractStep::Copying { done, total } => {
                if done == total || last.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
                    progress(Progress::Copying { done, total });
                    last = Some(Instant::now());
                }
            }
            ExtractStep::Writing => progress(Progress::Writing),
            ExtractStep::Replacing => {}
        }
        true
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use pdf_core::testgen::{SampleSpec, sample_document};

    use super::*;

    fn setup(name: &str) -> (PathBuf, Vec<(PathBuf, Option<String>)>) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/test-output/combine")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut sources = Vec::new();
        for name in ["a.pdf", "b.pdf"] {
            let path = dir.join(name);
            let doc = sample_document(&SampleSpec {
                pages: 3,
                ..SampleSpec::default()
            })
            .unwrap();
            pdf_core::save::save_atomic(&doc, SaveKind::Full, None, &path).unwrap();
            sources.push((path, None));
        }
        (dir, sources)
    }

    fn job(dir: &Path, sources: Vec<(PathBuf, Option<String>)>) -> Job {
        let picks = (0..2)
            .flat_map(|source| {
                (0..3).map(move |page| PagePick {
                    source,
                    page,
                    rotate: 0,
                })
            })
            .collect();
        Job {
            sources,
            picks,
            options: MergeOptions::default(),
            target: dir.join("combined.pdf"),
        }
    }

    #[test]
    fn combines_and_reports_progress_to_the_end() {
        let (dir, sources) = setup("ok");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let report = run(
            job(&dir, sources),
            Arc::new(AtomicBool::new(false)),
            move |p| {
                log.lock().unwrap().push(p);
            },
        )
        .unwrap();
        assert_eq!(report.pages, 6);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.first(), Some(&Progress::Copying { done: 1, total: 6 }));
        assert!(seen.contains(&Progress::Copying { done: 6, total: 6 }));
        assert_eq!(seen.last(), Some(&Progress::Writing));
        assert!(dir.join("combined.pdf").is_file());
    }

    #[test]
    fn cancelling_leaves_no_file_and_keeps_an_existing_target() {
        let (dir, sources) = setup("cancel");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        // Cancelled while copying.
        let result = run(job(&dir, sources.clone()), cancel.clone(), move |p| {
            if p == (Progress::Copying { done: 1, total: 6 }) {
                flag.store(true, Ordering::Relaxed);
            }
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(!dir.join("combined.pdf").exists());

        // Cancelled while writing: an existing file of that name stays as it was.
        std::fs::write(dir.join("combined.pdf"), b"keep me").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let result = run(job(&dir, sources), cancel, move |p| {
            if p == Progress::Writing {
                flag.store(true, Ordering::Relaxed);
            }
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(std::fs::read(dir.join("combined.pdf")).unwrap(), b"keep me");
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().all(|n| !n.ends_with(".lectrix-tmp")),
            "no temporary file left: {names:?}"
        );
    }

    #[test]
    fn extracts_with_progress_and_stops_when_cancelled() {
        let (dir, sources) = setup("extract");
        let (session, _) = Session::open(&sources[0].0, None).unwrap();
        let target = dir.join("pages.pdf");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let report = extract(
            &session,
            vec![2, 0],
            &target,
            Arc::new(AtomicBool::new(false)),
            move |p| log.lock().unwrap().push(p),
        )
        .unwrap();
        assert_eq!(report.pages, 2);
        assert_eq!(
            *seen.lock().unwrap(),
            [
                Progress::Copying { done: 1, total: 2 },
                Progress::Copying { done: 2, total: 2 },
                Progress::Writing
            ]
        );
        assert!(target.is_file());

        // Stopped once the pages are copied: the file of that name stays as it was.
        std::fs::write(&target, b"keep me").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let result = extract(&session, vec![1], &target, cancel, move |p| {
            if p == Progress::Writing {
                flag.store(true, Ordering::Relaxed);
            }
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
        session.close();
    }
}
