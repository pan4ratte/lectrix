//! An export job (section 6.14): its pages drawn and written on worker threads, each into a
//! temporary file beside where it goes, which takes its name only once every page has been
//! written. Stopped, or failing, the job leaves nothing behind.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::encode::{self, TiffPages};
use super::{Options, Raster, render};
use crate::error::{Error, Result};
use crate::session::Session;

/// Memory the pages being drawn and written may take at once, in bytes.
const MEMORY_BUDGET: u64 = 1 << 30;

/// How often the calling thread looks whether it was asked to stop.
const POLL: Duration = Duration::from_millis(50);

/// Where an export's pages go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// A file a page, in the job's order of pages.
    Files(Vec<PathBuf>),
    /// Every page in one TIFF file, in order.
    OneTiff(PathBuf),
}

/// Pages to export: 0-based, in order (a page may come twice in one TIFF file).
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub pages: Vec<usize>,
    pub options: Options,
    pub output: Output,
}

/// Exports `job` from `session`, telling `progress(done, total)` each page written. Once
/// `stop` is set, it stops as soon as it can and fails with [`Error::Cancelled`]. Every
/// page is checked against the format's limits before any is drawn.
pub fn run(
    session: &Session,
    job: &Job,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<()> {
    job.options.check()?;
    if let Output::Files(paths) = &job.output
        && paths.len() != job.pages.len()
    {
        return Err(Error::InvalidArgument(
            "a file name is needed for each page".into(),
        ));
    }
    if job.pages.is_empty() {
        return Err(Error::InvalidArgument("no pages to export".into()));
    }
    let facts = session.print_facts()?;
    let mut largest = 0u64;
    for &page in &job.pages {
        let size = facts.pages.get(page).ok_or(Error::PageOutOfRange(page))?;
        let (w, h) = job.options.pixel_size(page, size.width, size.height)?;
        largest = largest.max(u64::from(w) * u64::from(h));
    }
    let workers = workers(&job.options, largest, job.pages.len());
    match &job.output {
        Output::Files(paths) => files(session, job, paths, workers, stop, progress),
        Output::OneTiff(path) => one_tiff(session, job, path, workers, stop, progress),
    }
}

/// How many pages are drawn and written at once: one a core but one, as many as the memory
/// budget allows for the largest page.
fn workers(options: &Options, largest_pixels: u64, pages: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    let per_page = (largest_pixels * options.bytes_per_pixel()).max(1);
    let by_memory = usize::try_from(MEMORY_BUDGET / per_page).unwrap_or(usize::MAX);
    by_memory.min(cores.saturating_sub(1)).min(pages).max(1)
}

/// The temporary file beside `path` it is written to first.
pub(super) fn temporary(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".lectrix-part");
    path.with_file_name(name)
}

fn draw(session: &Session, page: usize, options: &Options) -> Result<Raster> {
    let list = session.export_list(page, options.annotations)?;
    render(&list, page, options)
}

/// Waits for the workers' results on the calling thread, which alone may call
/// `progress`; sets `halt` once `stop` is set or a page fails. The first error, if any.
fn collect<T>(
    rx: &mpsc::Receiver<Result<T>>,
    halt: &AtomicBool,
    stop: &AtomicBool,
    mut each: impl FnMut(T) -> Result<()>,
) -> Option<Error> {
    let mut failure = None;
    loop {
        match rx.recv_timeout(POLL) {
            Ok(Ok(item)) => {
                if failure.is_none()
                    && let Err(e) = each(item)
                {
                    failure = Some(e);
                    halt.store(true, Ordering::Relaxed);
                }
            }
            Ok(Err(e)) => {
                if failure.is_none() && !matches!(e, Error::Cancelled) {
                    failure = Some(e);
                }
                halt.store(true, Ordering::Relaxed);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if stop.load(Ordering::Relaxed) {
            halt.store(true, Ordering::Relaxed);
        }
    }
    failure
}

fn files(
    session: &Session,
    job: &Job,
    paths: &[PathBuf],
    workers: usize,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<()> {
    let temps: Vec<PathBuf> = paths.iter().map(|p| temporary(p)).collect();
    let total = job.pages.len();
    let next = AtomicUsize::new(0);
    let halt = AtomicBool::new(false);
    let failure = std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel::<Result<()>>();
        for _ in 0..workers {
            let tx = tx.clone();
            let (next, halt, temps) = (&next, &halt, &temps);
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= total || halt.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = (|| -> Result<()> {
                        let raster = draw(session, job.pages[i], &job.options)?;
                        let bytes = encode::encode(&raster, &job.options, halt)?;
                        drop(raster);
                        let mut file = File::create(&temps[i])?;
                        file.write_all(&bytes)?;
                        file.flush()?;
                        Ok(())
                    })();
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut done = 0;
        collect(&rx, &halt, stop, |()| {
            done += 1;
            progress(done, total);
            Ok(())
        })
    });
    let outcome = match failure {
        Some(e) => Err(e),
        None if stop.load(Ordering::Relaxed) => Err(Error::Cancelled),
        None => rename_all(&temps, paths),
    };
    if outcome.is_err() {
        for temp in &temps {
            let _ = std::fs::remove_file(temp);
        }
    }
    outcome
}

/// Gives each temporary file its name, replacing what is there.
fn rename_all(temps: &[PathBuf], paths: &[PathBuf]) -> Result<()> {
    for (temp, path) in temps.iter().zip(paths) {
        std::fs::rename(temp, path)?;
    }
    Ok(())
}

fn one_tiff(
    session: &Session,
    job: &Job,
    path: &Path,
    workers: usize,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<()> {
    let temp = temporary(path);
    let total = job.pages.len();
    // Pages drawn ahead of the one written next, at most: the file takes them in order.
    let window = workers * 2;
    let next = AtomicUsize::new(0);
    let halt = AtomicBool::new(false);
    let written = (Mutex::new(0usize), Condvar::new());
    let result = (|| -> Result<()> {
        let mut file = BufWriter::new(File::create(&temp)?);
        let mut tiff = TiffPages::new(&mut file, job.options.tiff)?;
        let failure = std::thread::scope(|scope| {
            let (tx, rx) = mpsc::channel::<Result<(usize, Raster)>>();
            for _ in 0..workers {
                let tx = tx.clone();
                let (next, halt, written) = (&next, &halt, &written);
                scope.spawn(move || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= total {
                            break;
                        }
                        // Waits while the file is too far behind.
                        let Ok(mut count) = written.0.lock() else {
                            break;
                        };
                        while i >= *count + window && !halt.load(Ordering::Relaxed) {
                            match written.1.wait_timeout(count, POLL) {
                                Ok((c, _)) => count = c,
                                Err(_) => return,
                            }
                        }
                        drop(count);
                        if halt.load(Ordering::Relaxed) {
                            break;
                        }
                        let result = draw(session, job.pages[i], &job.options).map(|r| (i, r));
                        if tx.send(result).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            let mut pending: BTreeMap<usize, Raster> = BTreeMap::new();
            let mut done = 0;
            collect(&rx, &halt, stop, |(i, raster)| {
                pending.insert(i, raster);
                while let Some(raster) = pending.remove(&done) {
                    tiff.add(&raster)?;
                    done += 1;
                    if let Ok(mut count) = written.0.lock() {
                        *count = done;
                    }
                    written.1.notify_all();
                    progress(done, total);
                }
                Ok(())
            })
        });
        if let Some(e) = failure {
            return Err(e);
        }
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        // Each page's directory is written with it, the last one ending the file; what is
        // buffered goes out here, where an error is seen.
        file.flush()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::Format;
    use crate::export::tests::options;

    #[test]
    fn workers_fit_the_memory_budget() {
        let o = options(Format::Png);
        assert_eq!(workers(&o, 1, 1), 1, "one page, one worker");
        // A page of 150 million pixels as JPEG: one at a time.
        let jpeg = options(Format::Jpeg);
        assert_eq!(workers(&jpeg, 150_000_000, 50), 1);
        // Pages at 150 dpi as PNG: as many as the cores allow.
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        assert_eq!(workers(&o, 2_000_000, 100), cores.saturating_sub(1).max(1));
    }
}
