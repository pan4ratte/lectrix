//! Windows 10 (1809+) and Windows 11.

use std::path::Path;

use super::Platform;

pub struct Windows;

impl Platform for Windows {
    fn same_file(&self, a: &Path, b: &Path) -> bool {
        // canonicalize resolves links and gives the on-disk spelling; NTFS is
        // case-insensitive, so compare case-insensitively as well.
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => {
                a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
            }
            _ => a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase(),
        }
    }

    fn set_low_memory(&self, window: &tauri::WebviewWindow, low: bool) {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_19,
        };
        use windows_core::Interface;
        let level = if low {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
        } else {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
        };
        let result = window.with_webview(move |webview| {
            // SAFETY: the controller is live for the duration of this callback, which
            // Tauri runs on the thread that owns the webview. ICoreWebView2_19 exists in
            // WebView2 runtimes from 1.0.1774; on older ones `cast` fails and nothing
            // happens.
            let outcome = unsafe {
                webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.cast::<ICoreWebView2_19>())
                    .and_then(|core| core.SetMemoryUsageTargetLevel(level))
            };
            if let Err(e) = outcome {
                crate::applog::warn(format!("could not set the webview's memory target: {e}"));
            }
        });
        if let Err(e) = result {
            crate::applog::warn(format!("could not reach the webview: {e}"));
        }
    }

    /// The shell opens the address with the default browser, as Run (Win+R) would.
    fn open_web_page(&self, url: &str) -> std::io::Result<()> {
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show: i32,
            ) -> isize;
        }
        /// SW_SHOWNORMAL: the browser's window as it last was.
        const SW_SHOWNORMAL: i32 = 1;
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (operation, file) = (wide("open"), wide(url));
        // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call; the window,
        // parameters and directory may be null. ShellExecuteW returns a value above 32 on
        // success and an error code otherwise.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result > 32 {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "ShellExecuteW returned {result}"
            )))
        }
    }

    /// WebView2 drops touchpad pinches unless `IsPinchZoomEnabled` is on, which wry ties
    /// to Tauri's zoom hotkeys setting. A change at runtime would only apply after the
    /// next navigation, so it is set when the webview is made.
    fn webview_needs_zoom_controls(&self) -> bool {
        true
    }

    /// Tauri always hands WebView2 its default arguments through the API. Some WebView2
    /// runtimes then ignore `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, which is how
    /// msedgedriver turns on remote debugging (tests/e2e; seen on WebView2 153). When the
    /// variable is set, pass both, merged.
    fn webview_browser_args(&self) -> Option<String> {
        let extra = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").ok()?;
        Some(merge_browser_args(TAURI_DEFAULT_ARGS, &extra))
    }

    fn file_key(&self, path: &Path) -> String {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_lowercase()
    }
}

/// What Tauri (wry 0.57) passes when no arguments are given: no mini menu, no SmartScreen,
/// autoplay allowed.
const TAURI_DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// Joins two argument lists. Chromium honours only one `--disable-features` switch, so
/// their feature lists are combined; other arguments are kept once, in order.
fn merge_browser_args(first: &str, second: &str) -> String {
    let mut features: Vec<&str> = Vec::new();
    let mut rest: Vec<&str> = Vec::new();
    for arg in first.split_whitespace().chain(second.split_whitespace()) {
        if let Some(list) = arg.strip_prefix("--disable-features=") {
            for feature in list.split(',').filter(|f| !f.is_empty()) {
                if !features.contains(&feature) {
                    features.push(feature);
                }
            }
        } else if !rest.contains(&arg) {
            rest.push(arg);
        }
    }
    let mut out = Vec::new();
    if !features.is_empty() {
        out.push(format!("--disable-features={}", features.join(",")));
    }
    out.extend(rest.into_iter().map(str::to_owned));
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_args_merge_feature_lists() {
        let merged = merge_browser_args(
            TAURI_DEFAULT_ARGS,
            "--disable-features=IgnoreDuplicateNavs,msWebOOUI --remote-debugging-port=0 --autoplay-policy=no-user-gesture-required",
        );
        assert_eq!(
            merged,
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,IgnoreDuplicateNavs --autoplay-policy=no-user-gesture-required --remote-debugging-port=0"
        );
    }

    #[test]
    fn compares_paths_case_insensitively() {
        let dir = std::env::temp_dir();
        let upper = dir.join("LECTRIX-SAME-FILE-TEST.tmp");
        std::fs::write(&upper, b"x").unwrap();
        let lower = dir.join("lectrix-same-file-test.tmp");
        assert!(Windows.same_file(&upper, &lower));
        assert_eq!(Windows.file_key(&upper), Windows.file_key(&lower));
        let _ = std::fs::remove_file(&upper);
    }
}
