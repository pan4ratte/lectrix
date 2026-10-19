//! Platform-specific services `pdf-core` needs, behind traits (AGENTS.md section 2), so
//! macOS and Linux can be added without touching the code that uses them. The app crate
//! has its own `platform` module for windowing; this one serves the PDF engine.

use crate::fonts::index::FontFace;
use crate::print::{PrintSetup, Sheet, Target};

#[cfg(any(unix, test))]
mod cups;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_print;

/// Lists the fonts installed on the system (ADR 0005).
pub trait SystemFonts: Send + Sync {
    /// Every installed face, without opening font files where the platform allows it.
    fn installed_faces(&self) -> Result<Vec<FontFace>, String>;
}

/// The font source of the platform this build runs on.
pub fn system_fonts() -> &'static dyn SystemFonts {
    #[cfg(windows)]
    {
        &windows::DirectWrite
    }
    #[cfg(not(windows))]
    {
        &NoSystemFonts
    }
}

/// Platforms without a font source yet (fontconfig and Core Text would go here):
/// non-embedded fonts other than the base 14 render with MuPDF's substitutes.
#[cfg(not(windows))]
struct NoSystemFonts;

#[cfg(not(windows))]
impl SystemFonts for NoSystemFonts {
    fn installed_faces(&self) -> Result<Vec<FontFace>, String> {
        Ok(Vec::new())
    }
}

/// A printer the system has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterEntry {
    pub name: String,
    /// The system's default printer.
    pub default: bool,
}

/// A paper size a printer takes.
#[derive(Debug, Clone, PartialEq)]
pub struct Paper {
    /// The printer's own id for it.
    pub id: i32,
    pub name: String,
    /// In points, portrait.
    pub width: f32,
    pub height: f32,
}

/// What a printer can do, with its current settings.
#[derive(Debug, Clone, PartialEq)]
pub struct PrinterCapabilities {
    pub papers: Vec<Paper>,
    /// The paper it is set to.
    pub paper: Option<i32>,
    /// It prints on both sides.
    pub duplex: bool,
    /// It prints in colour.
    pub colour: bool,
    /// The printer was last set to print on both sides (its driver's settings).
    pub duplex_on: bool,
}

/// The printers of the system, and printing on them (section 6.12, ADR 0028).
pub trait Printers: Send + Sync {
    fn list(&self) -> crate::Result<Vec<PrinterEntry>>;
    fn capabilities(&self, printer: &str) -> crate::Result<PrinterCapabilities>;
    /// The sheet `paper` (the printer's current one for `None`) gives, in portrait or
    /// landscape: its size and its printable area.
    fn sheet(&self, printer: &str, paper: Option<i32>, landscape: bool) -> crate::Result<Sheet>;
    /// The printer's own settings dialog exists (a driver's, on Windows).
    fn has_settings_dialog(&self) -> bool {
        false
    }
    /// Shows the printer's own settings dialog over the window `parent` (a raw window
    /// handle); its settings are kept for the jobs that follow while Lectrix runs. False if
    /// it was cancelled.
    fn settings_dialog(&self, _printer: &str, _parent: Option<isize>) -> crate::Result<bool> {
        Ok(false)
    }
    /// Starts a job on `printer`.
    fn start(&self, printer: &str, setup: &PrintSetup) -> crate::Result<Box<dyn Target>>;
}

/// The printers of the platform this build runs on.
pub fn printers() -> &'static dyn Printers {
    #[cfg(windows)]
    {
        &windows_print::GDI
    }
    #[cfg(unix)]
    {
        &cups::Cups
    }
    #[cfg(not(any(windows, unix)))]
    {
        &NoPrinters
    }
}

#[cfg(not(any(windows, unix)))]
struct NoPrinters;

#[cfg(not(any(windows, unix)))]
impl Printers for NoPrinters {
    fn list(&self) -> crate::Result<Vec<PrinterEntry>> {
        Ok(Vec::new())
    }
    fn capabilities(&self, printer: &str) -> crate::Result<PrinterCapabilities> {
        Err(crate::Error::Printer {
            printer: printer.into(),
            detail: "no printing on this platform".into(),
        })
    }
    fn sheet(&self, printer: &str, _paper: Option<i32>, _landscape: bool) -> crate::Result<Sheet> {
        Err(crate::Error::Printer {
            printer: printer.into(),
            detail: "no printing on this platform".into(),
        })
    }
    fn start(&self, printer: &str, _setup: &PrintSetup) -> crate::Result<Box<dyn Target>> {
        Err(crate::Error::Printer {
            printer: printer.into(),
            detail: "no printing on this platform".into(),
        })
    }
}
