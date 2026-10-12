//! The window's place between runs: its size and position as a normal window, and
//! whether it was maximized. Remembered in app data with the panes, and given to the window
//! builder, so the window is made there and never flashes at the default size. (Setting
//! the size of a window already made comes out taller by the title bar Lectrix draws
//! itself: Windows still counts the caption it hides.)
//!
//! The normal bounds are kept while the window is maximized, so restoring it after a
//! restart goes back to the size the user gave it. A window closed while minimized comes
//! back as it was before (normal or maximized), and so does one closed in full screen,
//! which is not remembered. Bounds that no longer fit on any monitor (one was unplugged, the
//! resolution changed) are dropped and Windows places the window.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Window};

use crate::store::Store;

/// The window's outer position and inner size as a normal window, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPlacement {
    /// None until the window has been a normal window (it may start maximized).
    pub bounds: Option<Bounds>,
    pub maximized: bool,
}

/// A monitor's area in physical pixels, and its scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// How much of the window's top edge, where the title bar is, must be on a monitor for
/// the user to reach it and drag the window.
const REACHABLE_WIDTH: i64 = 96;
const TITLE_BAR_HEIGHT: i64 = 32;

/// The saved bounds, clamped to the monitor they are on, and that monitor's scale; None
/// when the title bar would be out of reach on every monitor.
pub fn fit(bounds: Bounds, monitors: &[Monitor]) -> Option<(Bounds, f64)> {
    let (x, y, w) = (
        i64::from(bounds.x),
        i64::from(bounds.y),
        i64::from(bounds.width),
    );
    monitors.iter().find_map(|m| {
        let (mx, my, mw, mh) = (
            i64::from(m.x),
            i64::from(m.y),
            i64::from(m.width),
            i64::from(m.height),
        );
        let overlap = (x + w).min(mx + mw) - x.max(mx);
        let top_on_monitor = y >= my && y + TITLE_BAR_HEIGHT <= my + mh;
        (overlap >= REACHABLE_WIDTH && top_on_monitor).then(|| {
            let clamped = Bounds {
                width: bounds.width.min(m.width),
                height: bounds.height.min(m.height),
                ..bounds
            };
            (clamped, m.scale)
        })
    })
}

/// How the main window is first made.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Initial {
    /// Position and inner size in logical units, as the window builder takes them: the
    /// saved pixels at the scale of the monitor they are on, which is the scale the
    /// builder turns them back into pixels with.
    pub bounds: Option<(f64, f64, f64, f64)>,
    pub maximized: bool,
}

pub fn initial(saved: Option<WindowPlacement>, monitors: &[Monitor]) -> Initial {
    let Some(placement) = saved else {
        return Initial::default();
    };
    Initial {
        bounds: placement
            .bounds
            .and_then(|b| fit(b, monitors))
            .map(|(b, scale)| {
                (
                    f64::from(b.x) / scale,
                    f64::from(b.y) / scale,
                    f64::from(b.width) / scale,
                    f64::from(b.height) / scale,
                )
            }),
        maximized: placement.maximized,
    }
}

/// Where the cursor is in a window made for a tab dropped there, from the window's top-left
/// corner in logical pixels: about where its first tab is (after the icon, the name and
/// the menus), in the middle of the title bar's height.
const TAB_GRAB: (f64, f64) = (400.0, 17.0);

/// How far down and right of a window the one made for one of its tabs goes, logical pixels.
const CASCADE: f64 = 32.0;

fn monitor_at(x: f64, y: f64, monitors: &[Monitor]) -> Option<&Monitor> {
    monitors.iter().find(|m| {
        x >= f64::from(m.x)
            && x < f64::from(m.x) + f64::from(m.width)
            && y >= f64::from(m.y)
            && y < f64::from(m.y) + f64::from(m.height)
    })
}

/// A window of `size` (logical) with its corner at a point of monitor `m` (physical),
/// shrunk and moved as far as needed to be wholly on that monitor: position and inner
/// size in the monitor's logical units, as the window builder takes them.
fn on_monitor(x: f64, y: f64, size: (f64, f64), m: &Monitor) -> (f64, f64, f64, f64) {
    let (left, top) = (f64::from(m.x), f64::from(m.y));
    let width = (size.0 * m.scale).min(f64::from(m.width));
    let height = (size.1 * m.scale).min(f64::from(m.height));
    let x = x.clamp(left, left + f64::from(m.width) - width);
    let y = y.clamp(top, top + f64::from(m.height) - height);
    (x / m.scale, y / m.scale, width / m.scale, height / m.scale)
}

/// Where a window made for a tab dragged out of another goes: its title bar under the
/// cursor (physical pixels), where the tab was dropped. `size` is its inner size in
/// logical pixels. None when the cursor is on no monitor: the system places the window.
pub fn at_cursor(
    cursor: (f64, f64),
    size: (f64, f64),
    monitors: &[Monitor],
) -> Option<(f64, f64, f64, f64)> {
    let m = monitor_at(cursor.0, cursor.1, monitors)?;
    Some(on_monitor(
        cursor.0 - TAB_GRAB.0 * m.scale,
        cursor.1 - TAB_GRAB.1 * m.scale,
        size,
        m,
    ))
}

/// Where a window made for a tab from the menu goes: a little down and right of the
/// window the tab was in, whose outer position is `origin` (physical pixels), on the same
/// monitor.
pub fn beside(
    origin: (i32, i32),
    size: (f64, f64),
    monitors: &[Monitor],
) -> Option<(f64, f64, f64, f64)> {
    let (x, y) = (f64::from(origin.0), f64::from(origin.1));
    // A maximized window's corner is just off its monitor: look a little inside it.
    let m = monitor_at(x + 64.0, y + 64.0, monitors)?;
    Some(on_monitor(
        x + CASCADE * m.scale,
        y + CASCADE * m.scale,
        size,
        m,
    ))
}

/// The monitors connected now.
pub fn monitors(app: &AppHandle) -> Vec<Monitor> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Monitor {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale: m.scale_factor(),
        })
        .collect()
}

/// Notes the window's place after it moved or changed size. Kept in memory (moves come
/// many times a second while dragging) and written when Lectrix quits; a change between
/// normal and maximized is written at once.
///
/// Any of Lectrix's windows is noted, also when it gets the focus, so what is kept is the
/// place of the window used last: with several open, the one closed last. Getting the
/// focus (`write` false) writes nothing by itself.
pub fn remember(window: &Window, store: &mut Store, write: bool) {
    // Minimized, or full screen (which fills the monitor without being maximized): its
    // place is the one before.
    if window.is_minimized().unwrap_or(true) || window.is_fullscreen().unwrap_or(true) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let mut placement = store.window_placement().unwrap_or_default();
    let toggled = placement.maximized != maximized;
    placement.maximized = maximized;
    if !maximized && let (Ok(p), Ok(s)) = (window.outer_position(), window.inner_size()) {
        placement.bounds = Some(Bounds {
            x: p.x,
            y: p.y,
            width: s.width,
            height: s.height,
        });
    }
    store.set_window_placement(placement);
    if toggled && write {
        store.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Monitor = Monitor {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        scale: 1.5,
    };
    const LEFT: Monitor = Monitor {
        x: -1280,
        y: 200,
        width: 1280,
        height: 1024,
        scale: 1.0,
    };

    fn at(x: i32, y: i32, width: u32, height: u32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn bounds_on_a_monitor_are_kept_with_its_scale() {
        assert_eq!(
            fit(at(100, 80, 1200, 800), &[SCREEN]),
            Some((at(100, 80, 1200, 800), 1.5))
        );
        assert_eq!(
            fit(at(-1100, 300, 900, 700), &[SCREEN, LEFT]),
            Some((at(-1100, 300, 900, 700), 1.0))
        );
    }

    #[test]
    fn bounds_off_every_monitor_are_dropped() {
        // The monitor on the left was unplugged.
        assert_eq!(fit(at(-1100, 300, 900, 700), &[SCREEN]), None);
        // Only a sliver of the title bar would show.
        assert_eq!(fit(at(1880, 100, 800, 600), &[SCREEN]), None);
        // The title bar above the top of the screen.
        assert_eq!(fit(at(100, -20, 800, 600), &[SCREEN]), None);
        // The title bar below the bottom.
        assert_eq!(fit(at(100, 1060, 800, 600), &[SCREEN]), None);
    }

    #[test]
    fn partly_off_screen_is_fine_while_the_title_bar_is_reachable() {
        assert!(fit(at(1700, 100, 800, 600), &[SCREEN]).is_some());
    }

    #[test]
    fn a_window_larger_than_its_monitor_is_shrunk_to_it() {
        assert_eq!(
            fit(at(0, 0, 2560, 1440), &[SCREEN]),
            Some((at(0, 0, 1920, 1080), 1.5))
        );
    }

    #[test]
    fn a_tab_dropped_outside_gets_a_window_under_the_cursor() {
        // 800 x 400 logical is 1,200 x 600 on this monitor; the cursor ends up over the new
        // window's tabs.
        let (x, y, w, h) = at_cursor((900.0, 300.0), (800.0, 400.0), &[SCREEN]).unwrap();
        assert_eq!((w, h), (800.0, 400.0));
        assert_eq!((x, y), (200.0, 183.0));
        // Near an edge it is moved to stay wholly on the monitor.
        let (x, y, ..) = at_cursor((100.0, 5.0), (1000.0, 600.0), &[SCREEN]).unwrap();
        assert_eq!((x, y), (0.0, 0.0));
        let (x, y, ..) = at_cursor((1900.0, 1070.0), (1000.0, 600.0), &[SCREEN]).unwrap();
        assert_eq!((x, y), (280.0, 120.0));
        // On the other monitor, at its scale.
        let (x, y, w, h) = at_cursor((-700.0, 500.0), (1000.0, 600.0), &[SCREEN, LEFT]).unwrap();
        assert_eq!((x, y, w, h), (-1100.0, 483.0, 1000.0, 600.0));
        // Larger than the monitor: shrunk to it.
        let (x, y, w, h) = at_cursor((-700.0, 500.0), (1600.0, 1200.0), &[LEFT]).unwrap();
        assert_eq!((x, y, w, h), (-1280.0, 200.0, 1280.0, 1024.0));
        // The cursor on no monitor: the system places the window.
        assert_eq!(
            at_cursor((5000.0, 5000.0), (1000.0, 600.0), &[SCREEN]),
            None
        );
    }

    #[test]
    fn a_tab_moved_from_the_menu_gets_a_window_beside_its_own() {
        // 32 logical pixels down and right of the window it came from.
        let (x, y, w, h) = beside((150, 120), (800.0, 500.0), &[SCREEN]).unwrap();
        assert_eq!((x, y, w, h), (132.0, 112.0, 800.0, 500.0));
        // From a maximized window, whose corner is just off its monitor.
        let (x, y, ..) = beside((-12, -12), (800.0, 500.0), &[SCREEN]).unwrap();
        assert_eq!((x, y), (24.0, 24.0));
        // No room further down and right: it stays on the monitor.
        let (x, y, ..) = beside((700, 300), (800.0, 500.0), &[SCREEN]).unwrap();
        assert_eq!((x, y), (480.0, 220.0));
        assert_eq!(beside((9000, 9000), (800.0, 500.0), &[SCREEN]), None);
    }

    #[test]
    fn the_builder_gets_logical_units_of_the_monitor_and_the_maximized_state() {
        let saved = WindowPlacement {
            bounds: Some(at(150, 120, 1500, 900)),
            maximized: true,
        };
        assert_eq!(
            initial(Some(saved), &[SCREEN]),
            Initial {
                bounds: Some((100.0, 80.0, 1000.0, 600.0)),
                maximized: true,
            }
        );
        // Off every monitor: maximized still, wherever Windows puts it.
        assert_eq!(
            initial(Some(saved), &[LEFT]),
            Initial {
                bounds: None,
                maximized: true,
            }
        );
        assert_eq!(initial(None, &[SCREEN]), Initial::default());
    }
}
