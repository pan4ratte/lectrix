//! Platform-specific behavior behind one trait (AGENTS.md section 2), so macOS and Linux
//! can be added without touching feature code.

use std::path::Path;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

pub trait Platform: Send + Sync {
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

    /// Light or dark window (`None`: as the system is). The webview's color scheme follows
    /// the window theme.
    fn set_appearance(
        &self,
        window: &tauri::WebviewWindow,
        dark: Option<bool>,
    ) -> tauri::Result<()> {
        window.set_theme(dark.map(|d| {
            if d {
                tauri::Theme::Dark
            } else {
                tauri::Theme::Light
            }
        }))
    }

    /// Whether the webview's zoom controls must be on for touchpad pinches to reach the
    /// page (as Ctrl+wheel events, which the viewer turns into document zoom). The
    /// frontend then keeps the webview from zooming the app itself.
    fn webview_needs_zoom_controls(&self) -> bool {
        false
    }

    /// Makes touchpad pinches reach the page as document zoom where the webview would
    /// otherwise magnify the whole app with them. Called once the main window is made.
    fn route_touchpad_pinch(&self, _window: &tauri::WebviewWindow) {}

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

    /// Opens a web page in the user's default browser, outside the app (the webview never
    /// goes online). Used only with addresses fixed in Rust.
    fn open_web_page(&self, url: &str) -> std::io::Result<()> {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program)
            .arg(url)
            .spawn()
            .map(|_| ())
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
    #[cfg(target_os = "linux")]
    {
        &linux::Linux
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        &Generic
    }
}

/// Fallback for platforms without a specific implementation yet.
#[cfg(not(any(windows, target_os = "linux")))]
struct Generic;

#[cfg(not(any(windows, target_os = "linux")))]
impl Platform for Generic {}
