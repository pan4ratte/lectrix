//! Platform-specific behavior behind one trait (AGENTS.md section 2), so macOS and Linux
//! can be added without touching feature code.

use std::path::Path;

#[cfg(windows)]
mod windows;

/// Window backdrop the frontend should style for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// The system draws a translucent material (Mica) behind transparent areas.
    Mica,
    /// No material: the frontend paints solid colors everywhere.
    Solid,
}

pub trait Platform: Send + Sync {
    /// The backdrop the main window can use on this system.
    fn backdrop(&self) -> Backdrop;

    /// The user's accent color as `#rrggbb`, if the system has one.
    fn accent_color(&self) -> Option<String>;

    /// True if `a` and `b` name the same file (case rules and links of the platform).
    fn same_file(&self, a: &Path, b: &Path) -> bool {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        }
    }

    /// Asks the webview to use less memory (while the window is minimized) or to go back
    /// to normal (ADR 0002). Best effort: does nothing where the platform has no such
    /// control.
    fn set_low_memory(&self, _window: &tauri::WebviewWindow, _low: bool) {}

    /// Browser arguments for the webview, when they must differ from Tauri's defaults.
    fn webview_browser_args(&self) -> Option<String> {
        None
    }

    /// The name of the signed-in user, the default author of annotations (section 5.1
    /// rule 6).
    fn user_name(&self) -> Option<String> {
        ["USERNAME", "USER"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .map(|v| v.trim().to_owned())
            .find(|v| !v.is_empty())
    }

    /// A stable key for remembering things about a file (recent files, view position).
    fn file_key(&self, path: &Path) -> String {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned()
    }
}

/// The platform this build runs on.
pub fn current() -> &'static dyn Platform {
    #[cfg(windows)]
    {
        &windows::Windows
    }
    #[cfg(not(windows))]
    {
        &Generic
    }
}

/// Fallback for platforms without a specific implementation yet.
#[cfg(not(windows))]
struct Generic;

#[cfg(not(windows))]
impl Platform for Generic {
    fn backdrop(&self) -> Backdrop {
        Backdrop::Solid
    }

    fn accent_color(&self) -> Option<String> {
        None
    }
}
