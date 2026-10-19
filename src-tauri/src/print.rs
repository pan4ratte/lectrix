//! Print (section 6.12, ADR 0028): the printers, what each takes, the layout of a job's
//! sheets for the dialog's preview, and the job itself, on a thread of its own with
//! progress and Stop. The printing is `pdf_core::print`'s, on the platform's printers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_core::platform::Printers;
use pdf_core::print::{self, Job, PrintSetup, Sheet};
use pdf_core::session::Session;

use crate::ipc::{
    AppError, ErrorCode, PrintDone, PrintLayout, PrintProgress, PrintSettings, PrinterDetails,
    SheetPlacement,
};

/// Progress is reported at most this often (and at the end).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// LECTRIX_PRINT_OUTPUT has every job written into that file instead of printed (with
/// Windows' Microsoft Print to PDF, which then asks for no name; on CUPS, the PDF `lp`
/// would get). For automated tests only.
fn test_output() -> Option<std::path::PathBuf> {
    std::env::var_os("LECTRIX_PRINT_OUTPUT").map(Into::into)
}

/// What a sheet is: a printer, its paper (`None`: the one it is set to), landscape.
type SheetKey = (String, Option<i32>, bool);

#[derive(Default)]
pub struct Print {
    /// Sheets asked of printers, kept: a network printer can take a moment to answer.
    sheets: Mutex<HashMap<SheetKey, Sheet>>,
    /// Jobs printing, by document, with the flag that stops them.
    running: Mutex<HashMap<u32, Arc<AtomicBool>>>,
}

impl Print {
    fn printers(&self) -> &'static dyn Printers {
        pdf_core::platform::printers()
    }

    pub fn list(&self) -> Result<Vec<crate::ipc::PrinterEntry>, AppError> {
        Ok(self
            .printers()
            .list()?
            .into_iter()
            .map(|p| crate::ipc::PrinterEntry {
                name: p.name,
                default: p.default,
            })
            .collect())
    }

    pub fn details(&self, printer: &str) -> Result<PrinterDetails, AppError> {
        let printers = self.printers();
        Ok(PrinterDetails::new(
            printers.capabilities(printer)?,
            printers.has_settings_dialog(),
        ))
    }

    /// Shows `printer`'s own settings over the window `parent`; its details after, or
    /// `None` if the dialog was cancelled.
    pub fn settings_dialog(
        &self,
        printer: &str,
        parent: Option<isize>,
    ) -> Result<Option<PrinterDetails>, AppError> {
        if !self.printers().settings_dialog(printer, parent)? {
            return Ok(None);
        }
        // Its paper and margins may have changed.
        if let Ok(mut sheets) = self.sheets.lock() {
            sheets.retain(|(p, _, _), _| p != printer);
        }
        self.details(printer).map(Some)
    }

    fn sheet(&self, printer: &str, paper: Option<i32>, landscape: bool) -> Result<Sheet, AppError> {
        let key = (printer.to_owned(), paper, landscape);
        if let Some(sheet) = self.sheets.lock().ok().and_then(|s| s.get(&key).copied()) {
            return Ok(sheet);
        }
        let sheet = self.printers().sheet(printer, paper, landscape)?;
        if let Ok(mut sheets) = self.sheets.lock() {
            sheets.insert(key, sheet);
        }
        Ok(sheet)
    }

    /// Where each page `settings` asks for goes on its sheet (one copy).
    pub fn layout(
        &self,
        session: &Session,
        settings: &PrintSettings,
    ) -> Result<PrintLayout, AppError> {
        let facts = session.print_facts()?;
        let pages = pages(settings, facts.pages.len())?;
        let size = |p: usize| (facts.pages[p].width, facts.pages[p].height);
        let landscape =
            print::job_landscape(settings.orientation(), pages.iter().map(|&p| size(p)));
        let sheet = self.sheet(&settings.printer, settings.paper, landscape)?;
        let sheets = pages
            .iter()
            .map(|&p| {
                let (w, h) = size(p);
                let placement = print::place(
                    w,
                    h,
                    facts.pages[p].rotation,
                    &sheet,
                    settings.sizing(),
                    settings.orientation(),
                );
                SheetPlacement {
                    page: p as u32,
                    turn: placement.turn.into(),
                    area: placement.area.into(),
                }
            })
            .collect();
        Ok(PrintLayout {
            width: sheet.width,
            height: sheet.height,
            printable: sheet.printable.into(),
            landscape,
            sheets,
            low_resolution: facts.printing == pdf_core::protection::Printing::LowResolution,
        })
    }

    /// Prints document `id` (its `session`, named `title` in the print queue) as
    /// `settings` asks, on a thread of its own, reporting progress. Stopped with `stop`, it
    /// fails with the `cancelled` code and nothing is printed.
    pub fn run(
        &self,
        id: u32,
        session: Session,
        title: String,
        settings: PrintSettings,
        progress: impl Fn(PrintProgress) + Send + 'static,
    ) -> Result<PrintDone, AppError> {
        let facts = session.print_facts()?;
        let max_dpi = print::allowed_dpi(facts.printing)?;
        let pages = pages(&settings, facts.pages.len())?;
        let size = |p: usize| (facts.pages[p].width, facts.pages[p].height);
        let landscape =
            print::job_landscape(settings.orientation(), pages.iter().map(|&p| size(p)));
        let setup = PrintSetup {
            paper: settings.paper,
            landscape,
            duplex: settings.duplex.into(),
            grey: settings.grey,
            copies: settings.copies.clamp(1, 999),
            collate: settings.collate,
            title,
            output: test_output(),
            max_dpi,
        };
        let job = Job {
            pages,
            sizing: settings.sizing(),
            orientation: settings.orientation(),
            annotations: settings.annotations,
            grey: settings.grey,
            copies: setup.copies,
            collate: settings.collate,
            duplex: setup.duplex,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.running)?.insert(id, cancel.clone());
        let printer = settings.printer.clone();
        let started = Instant::now();
        let result = std::thread::Builder::new()
            .name("print".into())
            .spawn(move || -> pdf_core::Result<usize> {
                // The job is made on the thread that prints it: a printer's device context
                // belongs to the thread that made it.
                let target = pdf_core::platform::printers().start(&printer, &setup)?;
                let mut last: Option<Instant> = None;
                let mut sheets = 0;
                print::print(&session, &job, target, &mut |done, total| {
                    sheets = total;
                    if cancel.load(Ordering::Relaxed) {
                        return false;
                    }
                    if done == total || last.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
                        last = Some(Instant::now());
                        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
                        progress(PrintProgress {
                            done: n(done),
                            total: n(total),
                        });
                    }
                    true
                })?;
                Ok(sheets)
            })
            .map_err(pdf_core::Error::from)?
            .join()
            .map_err(|_| AppError::from(pdf_core::Error::ActorGone))?;
        if let Ok(mut running) = self.running.lock() {
            running.remove(&id);
        }
        match result {
            Ok(sheets) => {
                crate::applog::info(format!(
                    "printed {sheets} sheets on {} in {:.2} s",
                    settings.printer,
                    started.elapsed().as_secs_f64()
                ));
                Ok(PrintDone {
                    sheets: u32::try_from(sheets).unwrap_or(u32::MAX),
                })
            }
            Err(pdf_core::Error::Cancelled) => Err(AppError::new(
                "Printing was stopped. Sheets already sent may still print.",
                None,
            )
            .with_code(ErrorCode::Cancelled)),
            Err(e) => Err(e.into()),
        }
    }

    /// Stops printing document `id`.
    pub fn stop(&self, id: u32) {
        if let Ok(running) = self.running.lock()
            && let Some(cancel) = running.get(&id)
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// The pages `settings` asks for, checked against the document's `count`.
fn pages(settings: &PrintSettings, count: usize) -> Result<Vec<usize>, AppError> {
    let pages: Vec<usize> = settings.pages.iter().map(|&p| p as usize).collect();
    if pages.is_empty() || pages.iter().any(|&p| p >= count) {
        return Err(AppError::new(
            "Those pages aren't in this document.",
            Some("Check the pages to print."),
        ));
    }
    Ok(pages)
}

fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
    m.lock().map_err(|_| AppError::bad_state())
}
