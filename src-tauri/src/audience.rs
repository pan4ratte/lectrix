//! The audience screen of a presentation (AGENTS.md section 6.1): with a second screen, the
//! pages show there, full screen, in a window of their own, and the window presenting shows
//! the presenter view. That window, `audience-<label of the window presenting>`, is only a
//! picture of the presentation: it owns no documents, opens nothing, is left out of what
//! Lectrix's windows share (focus, placement, Move to window, quitting), and goes with the
//! window presenting. Its page gets what to show from that window, and sends it the keys
//! and clicks it receives, by events between the two (`src/lib/features/presentation/`).

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, window::Color,
};

use crate::ipc::AppError;
use crate::{applog, windows};

const PREFIX: &str = "audience-";
/// Event to the window presenting: its audience screen was closed (Alt+F4 there).
const AUDIENCE_CLOSED: &str = "audience-closed";

/// Whether `label` is an audience screen's window.
pub fn is_audience(label: &str) -> bool {
    label.starts_with(PREFIX)
}

fn label_for(owner: &str) -> String {
    format!("{PREFIX}{owner}")
}

/// The labels of Lectrix's own windows, the audience screens left out.
pub fn app_windows(app: &AppHandle) -> impl Iterator<Item = String> {
    app.webview_windows()
        .into_keys()
        .filter(|label| !is_audience(label))
}

/// Where a window presenting was before its screen was swapped, to go back there.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placement {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    maximized: bool,
    /// The screen it was on (its top-left corner).
    screen: (i32, i32),
}

/// The windows presenting, by label: where each was as its presentation started.
#[derive(Default)]
pub struct Audiences {
    placed: Mutex<HashMap<String, Placement>>,
}

/// A screen: its top-left corner and size, in physical pixels.
type Screen = (i32, i32, u32, u32);

fn screen_of(m: &tauri::Monitor) -> Screen {
    let (p, s) = (m.position(), m.size());
    (p.x, p.y, s.width, s.height)
}

/// The screen for the audience, of `screens`: the first that is not `current` (the one
/// the window presenting is on), or none with a single screen.
fn other_screen(screens: &[Screen], current: Option<(i32, i32)>) -> Option<usize> {
    if screens.len() < 2 {
        return None;
    }
    let current = current.unwrap_or((screens[0].0, screens[0].1));
    screens.iter().position(|s| (s.0, s.1) != current)
}

/// LECTRIX_AUDIENCE_ANY_SCREEN=1: with a single screen, the audience screen goes on it too,
/// over the window presenting, so tests on a machine with one screen can drive both
/// (tests/e2e/specs/presentation.test.mjs).
fn same_screen_for_tests(screens: &[Screen], current: Option<(i32, i32)>) -> Option<usize> {
    if std::env::var("LECTRIX_AUDIENCE_ANY_SCREEN").as_deref() != Ok("1") || screens.is_empty() {
        return None;
    }
    Some(
        current
            .and_then(|c| screens.iter().position(|s| (s.0, s.1) == c))
            .unwrap_or(0),
    )
}

fn failed(what: &str, e: tauri::Error) -> AppError {
    applog::warn(format!("audience screen: {what}: {e}"));
    AppError::new(
        "The presentation couldn't be shown on the other screen.",
        Some("Try again, or turn off the presenter view in Settings > Viewing."),
    )
}

/// Shows the presentation of `window` on another screen, if there is one: true when it is
/// shown there (or was already), false with a single screen. Call it before the window
/// presenting fills its screen.
#[tauri::command(async)]
pub fn open_audience_screen(
    app: AppHandle,
    window: WebviewWindow,
    audiences: tauri::State<'_, Audiences>,
) -> Result<bool, AppError> {
    let label = label_for(window.label());
    if app.get_webview_window(&label).is_some() {
        return Ok(true);
    }
    let monitors = window
        .available_monitors()
        .map_err(|e| failed("screens", e))?;
    let screens: Vec<Screen> = monitors.iter().map(screen_of).collect();
    let current = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| screen_of(&m));
    let at = current.map(|c| (c.0, c.1));
    let Some(other) = other_screen(&screens, at).or_else(|| same_screen_for_tests(&screens, at))
    else {
        return Ok(false);
    };
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size())
        && let Ok(mut placed) = audiences.placed.lock()
    {
        placed.insert(
            window.label().to_owned(),
            Placement {
                position,
                size,
                maximized: window.is_maximized().unwrap_or(false),
                screen: current.map_or((0, 0), |c| (c.0, c.1)),
            },
        );
    }
    create(&app, &label, screens[other]).map_err(|e| failed("window", e))?;
    Ok(true)
}

fn create(app: &AppHandle, label: &str, screen: Screen) -> tauri::Result<WebviewWindow> {
    let mut config = windows::main_config(app)?;
    config.label = label.to_owned();
    config.title = format!("{} presentation", app.package_info().name);
    config.visible = false;
    config.focus = false;
    config.decorations = false;
    config.skip_taskbar = true;
    config.maximized = false;
    config.fullscreen = false;
    config.x = None;
    config.y = None;
    let window = windows::builder(app, &config)?
        .background_color(Color(0, 0, 0, 255))
        .build()?;
    place(&window, screen)?;
    window.show()?;
    Ok(window)
}

/// Fills `screen` with `window`.
fn place(window: &WebviewWindow, screen: Screen) -> tauri::Result<()> {
    let (x, y, width, height) = screen;
    window.set_fullscreen(false)?;
    window.set_position(PhysicalPosition::new(x, y))?;
    window.set_size(PhysicalSize::new(width, height))?;
    window.set_fullscreen(true)
}

/// Swaps the screens of the presentation of `window` and of its presenter view.
#[tauri::command(async)]
pub fn swap_presentation_screens(app: AppHandle, window: WebviewWindow) -> Result<(), AppError> {
    let Some(audience) = app.get_webview_window(&label_for(window.label())) else {
        return Ok(());
    };
    let screen = |w: &WebviewWindow| w.current_monitor().ok().flatten().map(|m| screen_of(&m));
    let (Some(mine), Some(theirs)) = (screen(&window), screen(&audience)) else {
        return Ok(());
    };
    if mine == theirs {
        return Ok(());
    }
    place(&audience, mine).map_err(|e| failed("swap", e))?;
    place(&window, theirs).map_err(|e| failed("swap", e))?;
    // The keys stay with the presenter view.
    let _ = window.set_focus();
    Ok(())
}

/// Ends the presentation's audience screen of `window`. If the screens were swapped, the
/// window goes back where it was, full screen again when `full` (it was before the
/// presentation), else as a window.
#[tauri::command(async)]
pub fn close_audience_screen(
    app: AppHandle,
    window: WebviewWindow,
    audiences: tauri::State<'_, Audiences>,
    full: bool,
) {
    close_for(&app, window.label());
    let placed = audiences
        .placed
        .lock()
        .ok()
        .and_then(|mut p| p.remove(window.label()));
    let Some(placed) = placed else { return };
    let now = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| screen_of(&m));
    if now.is_none_or(|s| (s.0, s.1) == placed.screen) {
        return;
    }
    let back = || -> tauri::Result<()> {
        window.set_fullscreen(false)?;
        window.set_position(placed.position)?;
        window.set_size(placed.size)?;
        if full {
            window.set_fullscreen(true)?;
        } else if placed.maximized {
            window.maximize()?;
        }
        Ok(())
    };
    if let Err(e) = back() {
        applog::warn(format!("could not put window {} back: {e}", window.label()));
    }
}

/// Closes the audience screen of the window `owner`, if it has one.
pub fn close_for(app: &AppHandle, owner: &str) {
    if let Some(audience) = app.get_webview_window(&label_for(owner))
        && let Err(e) = audience.destroy()
    {
        applog::warn(format!(
            "could not close the audience screen of {owner}: {e}"
        ));
    }
}

/// An audience screen went: its window presenting ends the presentation, if it still runs.
pub fn destroyed(app: &AppHandle, label: &str) {
    let owner = &label[PREFIX.len()..];
    if app.get_webview_window(owner).is_some() {
        let _ = app.emit_to(owner, AUDIENCE_CLOSED, ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audience_windows_are_told_apart() {
        assert!(is_audience(&label_for("main")));
        assert!(is_audience(&label_for("window-2")));
        assert!(!is_audience("main"));
        assert!(!is_audience("window-3"));
    }

    #[test]
    fn the_audience_gets_another_screen() {
        let laptop = (0, 0, 2560, 1600);
        let projector = (2560, 0, 1920, 1080);
        let left = (-1920, 200, 1920, 1080);
        assert_eq!(other_screen(&[laptop], Some((0, 0))), None);
        assert_eq!(other_screen(&[laptop, projector], Some((0, 0))), Some(1));
        assert_eq!(other_screen(&[laptop, projector], Some((2560, 0))), Some(0));
        assert_eq!(
            other_screen(&[laptop, left, projector], Some((0, 0))),
            Some(1)
        );
        // The window's screen unknown: the first is taken for it.
        assert_eq!(other_screen(&[laptop, projector], None), Some(1));
        assert_eq!(other_screen(&[], None), None);
    }
}
