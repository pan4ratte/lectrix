//! Platform-specific behavior behind one trait (AGENTS.md section 2), so macOS and Linux
//! can be added without touching feature code.

use std::path::Path;

use crate::ipc::{WindowButton, WindowChrome};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
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
    /// otherwise magnify the whole app with them. Called once a window is made.
    fn route_touchpad_pinch(&self, _window: &tauri::WebviewWindow) {}

    /// Keeps the window from painting its background under the webview on every frame
    /// where the toolkit would otherwise do so although the webview covers it. Called once
    /// a window is made.
    fn skip_covered_window_background(&self, _window: &tauri::WebviewWindow) {}

    /// The Lectrix window the cursor is over, for a tab dropped outside its own window.
    /// Here, without knowing which window is on top: one whose frame the cursor is in, the
    /// focused one first.
    fn window_under_cursor(&self, app: &tauri::AppHandle) -> Option<String> {
        use tauri::Manager;
        let cursor = app.cursor_position().ok()?;
        let mut over: Vec<(String, tauri::WebviewWindow)> = app
            .webview_windows()
            .into_iter()
            .filter(|(_, w)| {
                let (Ok(false), Ok(p), Ok(s)) =
                    (w.is_minimized(), w.outer_position(), w.outer_size())
                else {
                    return false;
                };
                let (x, y) = (cursor.x - f64::from(p.x), cursor.y - f64::from(p.y));
                x >= 0.0 && y >= 0.0 && x < f64::from(s.width) && y < f64::from(s.height)
            })
            .collect();
        over.sort_by_key(|(label, w)| (!w.is_focused().unwrap_or(false), label.clone()));
        over.into_iter().next().map(|(label, _)| label)
    }

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

    /// Opens `folder` in the system's file manager.
    fn open_folder(&self, folder: &Path) -> std::io::Result<()> {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program)
            .arg(folder)
            .spawn()
            .map(|_| ())
    }

    /// Opens the folder of `file` in the system's file manager with the file selected
    /// (section 6.1, recent files); here, the folder alone.
    fn show_in_folder(&self, file: &Path) -> std::io::Result<()> {
        match file.parent() {
            Some(folder) => self.open_folder(folder),
            None => Err(std::io::Error::other("the file has no folder")),
        }
    }

    /// The window buttons the app draws in its title bar (section 8): Windows' three at the
    /// right, unless the platform places them otherwise or draws them itself.
    fn window_chrome(&self) -> WindowChrome {
        WindowChrome {
            left: Vec::new(),
            right: vec![
                WindowButton::Minimize,
                WindowButton::Maximize,
                WindowButton::Close,
            ],
        }
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
    #[cfg(target_os = "macos")]
    {
        &macos::Macos
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        &Generic
    }
}

/// Fallback for platforms without a specific implementation yet.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
struct Generic;

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
impl Platform for Generic {}

/// The window buttons a GTK decoration layout names (`gtk-decoration-layout`, which GNOME
/// takes from its `button-layout` setting and KDE writes from its own): the buttons before
/// the colon go at the left, those after it at the right, in their order. Other names
/// (`appmenu`, `icon`, `spacer`) and unknown ones are left out, as is a button named twice.
/// None for text that is not a layout.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_decoration_layout(layout: &str) -> Option<WindowChrome> {
    let (left, right) = layout.split_once(':')?;
    let mut seen = Vec::new();
    let mut side = |names: &str| {
        names
            .split(',')
            .filter_map(|name| match name.trim() {
                "minimize" => Some(WindowButton::Minimize),
                "maximize" => Some(WindowButton::Maximize),
                "close" => Some(WindowButton::Close),
                _ => None,
            })
            .filter(|button| {
                let new = !seen.contains(button);
                seen.push(*button);
                new
            })
            .collect::<Vec<_>>()
    };
    let left = side(left);
    let right = side(right);
    Some(WindowChrome { left, right })
}

/// A `file://` URI for an absolute path given as its bytes: everything but the characters a
/// URI may carry as they are (letters, digits, `-._~` and the slashes) percent-encoded,
/// so spaces, commas and non-ASCII names survive D-Bus and the file manager.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn file_uri(path: &[u8]) -> String {
    let mut uri = String::from("file://");
    for &b in path {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            uri.push(char::from(b));
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_encode_what_a_uri_cannot_carry() {
        assert_eq!(file_uri(b"/home/ada/a.pdf"), "file:///home/ada/a.pdf");
        assert_eq!(
            file_uri("/home/ada/My files, 100%/Ü b.pdf".as_bytes()),
            "file:///home/ada/My%20files%2C%20100%25/%C3%9C%20b.pdf"
        );
    }
    use WindowButton::{Close, Maximize, Minimize};

    fn chrome(left: &[WindowButton], right: &[WindowButton]) -> Option<WindowChrome> {
        Some(WindowChrome {
            left: left.to_vec(),
            right: right.to_vec(),
        })
    }

    #[test]
    fn decoration_layouts() {
        // GNOME's default: close alone, at the right.
        assert_eq!(
            parse_decoration_layout("appmenu:close"),
            chrome(&[], &[Close])
        );
        // KDE (kde-gtk-config), and GNOME with minimize and maximize turned on.
        assert_eq!(
            parse_decoration_layout("icon:minimize,maximize,close"),
            chrome(&[], &[Minimize, Maximize, Close])
        );
        // elementary OS: close at the left, maximize at the right.
        assert_eq!(
            parse_decoration_layout("close:maximize"),
            chrome(&[Close], &[Maximize])
        );
        // As on a Mac, with spaces and names that are not buttons.
        assert_eq!(
            parse_decoration_layout("close, minimize, maximize, spacer:menu"),
            chrome(&[Close, Minimize, Maximize], &[])
        );
        // A button named twice keeps its first place.
        assert_eq!(
            parse_decoration_layout("close:minimize,close"),
            chrome(&[Close], &[Minimize])
        );
        assert_eq!(parse_decoration_layout(":"), chrome(&[], &[]));
        assert_eq!(parse_decoration_layout("close"), None);
    }
}
