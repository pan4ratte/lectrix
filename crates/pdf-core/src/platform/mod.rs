//! Platform-specific services `pdf-core` needs, behind traits (AGENTS.md section 2), so
//! macOS and Linux can be added without touching the code that uses them. The app crate
//! has its own `platform` module for windowing; this one serves the PDF engine.

use crate::fonts::index::FontFace;

#[cfg(windows)]
mod windows;

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

/// Platforms without a font source yet (Phase 6 adds fontconfig and Core Text):
/// non-embedded fonts other than the base 14 render with MuPDF's substitutes.
#[cfg(not(windows))]
struct NoSystemFonts;

#[cfg(not(windows))]
impl SystemFonts for NoSystemFonts {
    fn installed_faces(&self) -> Result<Vec<FontFace>, String> {
        Ok(Vec::new())
    }
}
