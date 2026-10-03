//! Windows 10 (1809+) and Windows 11.

use std::path::Path;

use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

use super::{Backdrop, Platform};

pub struct Windows;

/// First Windows 11 build; Mica exists from here on.
const WINDOWS_11_BUILD: u32 = 22000;

fn build_number() -> u32 {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..OSVERSIONINFOW::default()
    };
    // SAFETY: `info` is a properly sized, writable OSVERSIONINFOW with its size field set,
    // as RtlGetVersion requires. It always succeeds for this structure.
    let status = unsafe { RtlGetVersion(&mut info) };
    if status == 0 { info.dwBuildNumber } else { 0 }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads a DWORD from HKEY_CURRENT_USER.
fn read_user_dword(subkey: &str, value: &str) -> Option<u32> {
    let (subkey, value) = (wide(subkey), wide(value));
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: the key and value names are NUL-terminated UTF-16 buffers that outlive the
    // call; `data` and `size` describe a writable 4-byte buffer, and RRF_RT_REG_DWORD
    // limits the result to a DWORD.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(data)
}

impl Platform for Windows {
    /// The window theme, and Mica in the same tone: the plain Mica effect follows the
    /// system's theme, not the window's, so a forced theme would show a light material
    /// behind dark text (or the reverse).
    fn set_appearance(
        &self,
        window: &tauri::WebviewWindow,
        dark: Option<bool>,
    ) -> tauri::Result<()> {
        use tauri::window::{Effect, EffectsBuilder};
        window.set_theme(dark.map(|d| {
            if d {
                tauri::Theme::Dark
            } else {
                tauri::Theme::Light
            }
        }))?;
        if self.backdrop() == Backdrop::Mica {
            let effect = match dark {
                Some(true) => Effect::MicaDark,
                Some(false) => Effect::MicaLight,
                None => Effect::Mica,
            };
            window.set_effects(EffectsBuilder::new().effect(effect).build())?;
        }
        Ok(())
    }

    fn backdrop(&self) -> Backdrop {
        if build_number() >= WINDOWS_11_BUILD {
            Backdrop::Mica
        } else {
            Backdrop::Solid
        }
    }

    fn accent_color(&self) -> Option<String> {
        // The accent color chosen in Settings > Personalization > Colors, as 0xAABBGGRR.
        let abgr = read_user_dword(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent",
            "AccentColorMenu",
        )?;
        let [r, g, b, _] = abgr.to_le_bytes();
        Some(format!("#{r:02x}{g:02x}{b:02x}"))
    }

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
    fn reads_a_plausible_build_number() {
        // Windows 10 1809 is build 17763, the oldest supported version.
        assert!(build_number() >= 17763);
    }

    #[test]
    fn compares_paths_case_insensitively() {
        let dir = std::env::temp_dir();
        let upper = dir.join("FOLIO-SAME-FILE-TEST.tmp");
        std::fs::write(&upper, b"x").unwrap();
        let lower = dir.join("folio-same-file-test.tmp");
        assert!(Windows.same_file(&upper, &lower));
        assert_eq!(Windows.file_key(&upper), Windows.file_key(&lower));
        let _ = std::fs::remove_file(&upper);
    }
}
