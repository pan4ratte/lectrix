//! macOS: WKWebView in an AppKit window, whose own buttons (the traffic lights) lie over
//! the app's title bar (`windows::create`).

use super::Platform;
use crate::ipc::WindowChrome;

pub struct Macos;

impl Platform for Macos {
    /// Finder's Reveal: the folder with the file selected.
    fn show_in_folder(&self, file: &std::path::Path) -> std::io::Result<()> {
        std::process::Command::new("open")
            .arg("-R")
            .arg(file)
            .spawn()
            .map(|_| ())
    }

    /// None: the system draws the window's buttons.
    fn window_chrome(&self) -> WindowChrome {
        WindowChrome {
            left: Vec::new(),
            right: Vec::new(),
        }
    }
}
