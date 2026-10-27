//! Compare (section 6.13): two documents open as sources (not tabs) in a window's Compare
//! view, compared; the comparison waits here, by the pair of documents, for the annotated
//! copies and the report to be saved from it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_core::compare::annotate::{self, Which};
use pdf_core::compare::report::{self, ReportInfo};
use pdf_core::compare::{Comparison, Difference};
use pdf_core::session::Session;

use crate::ipc::{AppError, CompareOutcome, CompareProgress, CompareSettings, CompareSide};

/// Progress is reported at most this often (and always as a stage starts).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// A pair of documents: the first (old) and the second (new).
type Pair = (u32, u32);

struct Done {
    comparison: Comparison,
    settings: CompareSettings,
}

#[derive(Default)]
pub struct Compare {
    results: Mutex<HashMap<Pair, Done>>,
    running: Mutex<HashMap<Pair, Arc<AtomicBool>>>,
}

impl Compare {
    /// Compares the document of `old` with that of `new`, reporting progress; the result
    /// waits for saving. Stopped with [`Compare::stop`], it fails with `cancelled`.
    pub fn run(
        &self,
        pair: Pair,
        sessions: (Session, Session),
        settings: CompareSettings,
        on_progress: impl Fn(CompareProgress) + Sync,
    ) -> Result<CompareOutcome, AppError> {
        let stop = Arc::new(AtomicBool::new(false));
        if let Ok(mut running) = self.running.lock() {
            running.insert(pair, stop.clone());
        }
        let last = Mutex::new(None::<(Instant, std::mem::Discriminant<CompareProgress>)>);
        let progress = |p: pdf_core::compare::Progress| {
            let p = CompareProgress::from(p);
            let kind = std::mem::discriminant(&p);
            let Ok(mut last) = last.lock() else { return };
            // A new stage at once; within one, now and then.
            let due = last.is_none_or(|(at, k)| k != kind || at.elapsed() >= PROGRESS_INTERVAL);
            if due {
                *last = Some((Instant::now(), kind));
                on_progress(p);
            }
        };
        let start = Instant::now();
        let result = pdf_core::compare::run(
            &sessions.0,
            &sessions.1,
            settings.options(),
            settings.mode(),
            &progress,
            &|| stop.load(Ordering::Relaxed),
        );
        if let Ok(mut running) = self.running.lock() {
            running.remove(&pair);
        }
        let comparison = result?;
        crate::applog::info(format!(
            "compared documents {} and {}: {} differences in {:.0} ms",
            pair.0,
            pair.1,
            comparison.differences.len(),
            start.elapsed().as_secs_f64() * 1000.0
        ));
        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        let outcome = CompareOutcome {
            differences: comparison.differences.iter().map(Into::into).collect(),
            anchors: comparison
                .anchors
                .iter()
                .map(|a| crate::ipc::CompareAnchor {
                    old: a.old,
                    new: a.new,
                })
                .collect(),
            old_words: n(comparison.old_words),
            new_words: n(comparison.new_words),
        };
        if let Ok(mut results) = self.results.lock() {
            results.insert(
                pair,
                Done {
                    comparison,
                    settings,
                },
            );
        }
        Ok(outcome)
    }

    pub fn stop(&self, pair: Pair) {
        if let Ok(running) = self.running.lock()
            && let Some(stop) = running.get(&pair)
        {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// Forgets the comparison of a pair (its view closed, or a document replaced).
    pub fn forget(&self, pair: Pair) {
        self.stop(pair);
        if let Ok(mut results) = self.results.lock() {
            results.remove(&pair);
        }
    }

    /// Runs `f` with the differences of a pair's comparison but those numbered in `skip`
    /// (the ones the user ignored), and the options compared with.
    fn with<T>(
        &self,
        pair: Pair,
        skip: &[u32],
        f: impl FnOnce(&[&Difference], CompareSettings) -> T,
    ) -> Result<T, AppError> {
        let results = self.results.lock().map_err(|_| AppError::bad_state())?;
        let done = results.get(&pair).ok_or_else(|| {
            AppError::new(
                "These documents are not compared yet.",
                Some("Compare them first."),
            )
        })?;
        let kept: Vec<&Difference> = done
            .comparison
            .differences
            .iter()
            .enumerate()
            .filter(|(i, _)| !skip.contains(&(*i as u32)))
            .map(|(_, d)| d)
            .collect();
        Ok(f(&kept, done.settings))
    }

    /// Writes a copy of the `side` document (its file `source`, opened with `password`) to
    /// `target`, with each difference of the comparison but `skip` marked and commented;
    /// `other` is the other document's name, `author` the annotations' author.
    #[allow(clippy::too_many_arguments)]
    pub fn save_annotated(
        &self,
        pair: Pair,
        side: CompareSide,
        skip: &[u32],
        source: &Path,
        password: Option<&str>,
        target: &Path,
        other: &str,
        author: &str,
    ) -> Result<(), AppError> {
        let which = match side {
            CompareSide::Old => Which::Old,
            CompareSide::New => Which::New,
        };
        let marks = self.with(pair, skip, |kept, _| {
            annotate::annotations(kept, which, other, author)
        })?;
        annotate::write_copy(source, password, target, marks)?;
        Ok(())
    }

    /// Writes the report of a pair's comparison (but `skip`) to `target`.
    #[allow(clippy::too_many_arguments)]
    pub fn save_report(
        &self,
        pair: Pair,
        skip: &[u32],
        target: &Path,
        names: (&str, &str),
        labels: (Option<Vec<String>>, Option<Vec<String>>),
        when: &str,
        grouped: bool,
    ) -> Result<(), AppError> {
        let page_name = |labels: &Option<Vec<String>>, page: usize| -> String {
            labels
                .as_ref()
                .and_then(|l| l.get(page))
                .filter(|l| !l.is_empty())
                .cloned()
                .unwrap_or_else(|| (page + 1).to_string())
        };
        let old_page = |p: usize| page_name(&labels.0, p);
        let new_page = |p: usize| page_name(&labels.1, p);
        self.with(pair, skip, |kept, settings| {
            let info = ReportInfo {
                old_name: names.0,
                new_name: names.1,
                when,
                options: settings.options(),
                grouped,
                old_page: &old_page,
                new_page: &new_page,
            };
            report::write(target, kept, &info)
        })??;
        Ok(())
    }
}
