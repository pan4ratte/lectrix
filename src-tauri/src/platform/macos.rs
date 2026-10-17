//! macOS: WKWebView in an AppKit window, whose own buttons (the traffic lights) lie over
//! the app's title bar (`windows::create`).

use super::Platform;
use crate::ipc::WindowChrome;

pub struct Macos;

impl Platform for Macos {
    /// None: the system draws the window's buttons.
    fn window_chrome(&self) -> WindowChrome {
        WindowChrome {
            left: Vec::new(),
            right: Vec::new(),
        }
    }
}
