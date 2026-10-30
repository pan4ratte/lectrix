//! Lectrix's windows (AGENTS.md section 8). Every window is the whole app with tabs of its
//! own: the first is `main`, and each later one is made for a tab moved out of another.
//! Documents live in Rust, so moving a tab only changes which window shows it. This module
//! keeps that, and what else differs from window to window.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::ipc::{OpenResult, TabHandoff};
use crate::{AppState, applog, placement, platform};

/// The first window's label; later ones are `window-2`, `window-3`…
pub const MAIN: &str = "main";

/// Event carrying a document id, for the window that shows it: a file opened from another
/// window is open here already.
const SHOW_DOCUMENT: &str = "show-document";
/// Event telling a window that tabs were moved into it (`list_open_documents` has them).
const DOCUMENTS_MOVED: &str = "documents-moved";

/// How long a window made for a tab may take to start. Until its page runs, it cannot ask
/// about unsaved changes, so it is not closed meanwhile; past this, it is taken for broken
/// and closes like any window.
const STARTING: Duration = Duration::from_secs(15);

/// How many closed tabs a window keeps to reopen, the last closed first.
const CLOSED_KEPT: usize = 25;

#[derive(Default)]
pub struct Windows {
    /// The window that shows each open document (tabs and sources), by id.
    owners: Mutex<HashMap<u32, String>>,
    /// What tabs keep of their own while they move to another window.
    handoffs: Mutex<HashMap<u32, TabHandoff>>,
    /// The window that last had the focus: files from a second launch open there.
    focused: Mutex<Option<String>>,
    /// Windows whose dropped files go to the grid of pages they show (the Combine view, a
    /// document's pages being organized) instead of tabs.
    drop_to_combine: Mutex<HashSet<String>>,
    /// Minimized windows (their webviews then use less memory).
    minimized: Mutex<HashSet<String>>,
    /// Windows made for a tab whose page does not show it yet, and when they were made.
    starting: Mutex<HashMap<String, Instant>>,
    /// The files of the tabs each window closed, oldest first, to reopen (the paths
    /// Lectrix had them open from, never ones from the webview).
    closed: Mutex<HashMap<String, Vec<PathBuf>>>,
    made: AtomicU32,
}

impl Windows {
    /// Gives document `id` to the window `label`.
    pub fn own(&self, id: u32, label: &str) {
        if let Ok(mut owners) = self.owners.lock() {
            owners.insert(id, label.to_owned());
        }
    }

    pub fn owner(&self, id: u32) -> Option<String> {
        self.owners.lock().ok()?.get(&id).cloned()
    }

    /// The documents a window shows, in the order they were opened.
    pub fn owned_by(&self, label: &str) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .owners
            .lock()
            .map(|owners| {
                owners
                    .iter()
                    .filter(|(_, l)| l.as_str() == label)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    /// The document was closed.
    pub fn release(&self, id: u32) {
        if let Ok(mut owners) = self.owners.lock() {
            owners.remove(&id);
        }
        self.take_handoff(id);
    }

    /// The window is gone: forgets it, and returns the documents it showed.
    pub fn forget(&self, label: &str) -> Vec<u32> {
        let ids = self.owned_by(label);
        for &id in &ids {
            self.release(id);
        }
        if let Ok(mut set) = self.drop_to_combine.lock() {
            set.remove(label);
        }
        if let Ok(mut set) = self.minimized.lock() {
            set.remove(label);
        }
        self.set_ready(label);
        if let Ok(mut closed) = self.closed.lock() {
            closed.remove(label);
        }
        if let Ok(mut focused) = self.focused.lock()
            && focused.as_deref() == Some(label)
        {
            *focused = None;
        }
        ids
    }

    /// Window `label` closed the tab of `path`: the next to reopen. Returns how many it has
    /// to reopen.
    pub fn remember_closed(&self, label: &str, path: PathBuf) -> usize {
        let Ok(mut closed) = self.closed.lock() else {
            return 0;
        };
        let list = closed.entry(label.to_owned()).or_default();
        list.retain(|p| p != &path);
        list.push(path);
        if list.len() > CLOSED_KEPT {
            list.remove(0);
        }
        list.len()
    }

    /// The file of the tab window `label` closed last, taken from those it can reopen.
    pub fn take_closed(&self, label: &str) -> Option<PathBuf> {
        self.closed.lock().ok()?.get_mut(label)?.pop()
    }

    /// How many closed tabs window `label` can reopen.
    pub fn closed_count(&self, label: &str) -> usize {
        self.closed
            .lock()
            .ok()
            .and_then(|closed| closed.get(label).map(Vec::len))
            .unwrap_or(0)
    }

    pub fn set_handoff(&self, id: u32, handoff: TabHandoff) {
        if let Ok(mut handoffs) = self.handoffs.lock() {
            handoffs.insert(id, handoff);
        }
    }

    /// What the tab of document `id` brought from the window it left, once.
    pub fn take_handoff(&self, id: u32) -> Option<TabHandoff> {
        self.handoffs.lock().ok()?.remove(&id)
    }

    pub fn set_focused(&self, label: &str) {
        if let Ok(mut focused) = self.focused.lock() {
            *focused = Some(label.to_owned());
        }
    }

    /// The window used last, if one has had the focus.
    pub fn focused(&self) -> Option<String> {
        self.focused.lock().ok().and_then(|f| f.clone())
    }

    pub fn set_drop_to_combine(&self, label: &str, combine: bool) {
        if let Ok(mut set) = self.drop_to_combine.lock() {
            if combine {
                set.insert(label.to_owned());
            } else {
                set.remove(label);
            }
        }
    }

    pub fn drops_to_combine(&self, label: &str) -> bool {
        self.drop_to_combine
            .lock()
            .is_ok_and(|set| set.contains(label))
    }

    /// Notes whether a window is minimized; true if that changed.
    pub fn set_minimized(&self, label: &str, minimized: bool) -> bool {
        self.minimized.lock().is_ok_and(|mut set| {
            if minimized {
                set.insert(label.to_owned())
            } else {
                set.remove(label)
            }
        })
    }

    fn set_starting(&self, label: &str) {
        if let Ok(mut starting) = self.starting.lock() {
            starting.insert(label.to_owned(), Instant::now());
        }
    }

    /// The window's page has its tabs (`list_open_documents`): it handles its own close
    /// requests now.
    pub fn set_ready(&self, label: &str) {
        if let Ok(mut starting) = self.starting.lock() {
            starting.remove(label);
        }
    }

    /// Whether a window made for a tab is still starting, so it can't yet ask about the
    /// unsaved changes it may hold.
    pub fn is_starting(&self, label: &str) -> bool {
        self.starting.lock().is_ok_and(|starting| {
            starting
                .get(label)
                .is_some_and(|at| at.elapsed() < STARTING)
        })
    }

    fn next_label(&self) -> String {
        format!("window-{}", self.made.fetch_add(1, Ordering::Relaxed) + 2)
    }
}

/// Where a new window goes.
pub enum Place {
    /// Where Lectrix's window was when it last closed, maximized if it was (the first
    /// window; placement.rs).
    Remembered,
    /// Position and inner size in logical units, or wherever the system puts a window of
    /// the default size.
    At(Option<(f64, f64, f64, f64)>),
}

/// Makes a window. Not from tauri.conf.json (`create: false` there), so it can get browser
/// arguments the platform needs, and be made where it belongs instead of moved there.
///
/// On Windows this must not run in a synchronous command or an event handler (the window
/// is made on the main thread, which those block).
pub fn create(app: &AppHandle, label: &str, place: Place) -> tauri::Result<WebviewWindow> {
    let mut config = main_config(app)?;
    config.label = label.to_owned();
    let platform = platform::current();
    let mut builder = builder(app, &config)?;
    // On macOS the window keeps the system's frame (its shadow, rounded corners and
    // resizing) and buttons, which lie over the app's title bar at its left end, centred in
    // its 35 px (TitleBar.svelte leaves room for them). Elsewhere the app draws its own.
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .decorations(true)
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(13.0, 17.0));
    }
    let state = app.state::<AppState>();
    // The page knows from its first frame whether the window starts with documents, so it
    // shows the start screen at once or not at all (`StartupInfo::opens_documents` says
    // the same once it asks).
    if opens_documents(&state, label) {
        builder = builder.initialization_script("window.__LECTRIX_OPENING__ = true;");
    }
    let (bounds, maximized) = match place {
        Place::Remembered => {
            let saved = state.store.lock().ok().and_then(|s| s.window_placement());
            let initial = placement::initial(saved, &placement::monitors(app));
            (initial.bounds, initial.maximized)
        }
        Place::At(bounds) => (bounds, false),
    };
    if let Some((x, y, width, height)) = bounds {
        builder = builder.position(x, y).inner_size(width, height);
    } else if let Some((width, height)) = size_for_tests() {
        builder = builder.inner_size(width, height);
    }
    if maximized {
        builder = builder.maximized(true);
    }
    let window = builder.build()?;
    platform.route_touchpad_pinch(&window);
    platform.skip_covered_window_background(&window);
    let appearance = state
        .store
        .lock()
        .map(|s| s.settings().appearance)
        .unwrap_or_default();
    crate::apply_appearance_to(&window, appearance);
    Ok(window)
}

/// The first window's configuration (tauri.conf.json), which every window starts from.
pub fn main_config(app: &AppHandle) -> tauri::Result<tauri::utils::config::WindowConfig> {
    app.config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)
}

/// A window's builder with what every Lectrix webview needs.
pub fn builder<'a>(
    app: &'a AppHandle,
    config: &tauri::utils::config::WindowConfig,
) -> tauri::Result<tauri::WebviewWindowBuilder<'a, tauri::Wry, AppHandle>> {
    let platform = platform::current();
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, config)?
        .zoom_hotkeys_enabled(platform.webview_needs_zoom_controls());
    // The same for every window: webviews that share a data folder must agree on them.
    if let Some(args) = platform.webview_browser_args() {
        builder = builder.additional_browser_args(&args);
    }
    Ok(builder)
}

/// LECTRIX_WINDOW_SIZE=<width>x<height>, in logical pixels: the size of a window that has
/// no place of its own (none remembered, none given), instead of the default one. For test
/// runs that show the app as CI's runners do, whose screen leaves the window less room
/// (tests/e2e, `npm run test:ci`).
fn size_for_tests() -> Option<(f64, f64)> {
    parse_size(&std::env::var("LECTRIX_WINDOW_SIZE").ok()?)
}

fn parse_size(text: &str) -> Option<(f64, f64)> {
    let (width, height) = text.split_once('x')?;
    let width = width.trim().parse::<f64>().ok()?;
    let height = height.trim().parse::<f64>().ok()?;
    (width >= 1.0 && height >= 1.0).then_some((width, height))
}

/// Where a window made for a tab of `from` goes: with its title bar under the cursor (the
/// tab was dragged out and dropped there), or beside `from`. It gets the size `from` has
/// as a normal window.
fn place_beside(
    app: &AppHandle,
    from: &WebviewWindow,
    at_cursor: bool,
) -> Option<(f64, f64, f64, f64)> {
    let monitors = placement::monitors(app);
    let size = match (from.is_maximized(), from.inner_size(), from.scale_factor()) {
        (Ok(false), Ok(size), Ok(scale)) if scale > 0.0 => (
            f64::from(size.width) / scale,
            f64::from(size.height) / scale,
        ),
        // Maximized, it has no size of its own to give: the default one.
        _ => app
            .config()
            .app
            .windows
            .iter()
            .find(|w| w.label == MAIN)
            .map(|w| (w.width, w.height))?,
    };
    if at_cursor {
        let cursor = app.cursor_position().ok()?;
        placement::at_cursor((cursor.x, cursor.y), size, &monitors)
    } else {
        let origin = from.outer_position().ok()?;
        placement::beside((origin.x, origin.y), size, &monitors)
    }
}

/// Whether the window `label` starts with documents to open: files from the command line
/// (which the first window to ask takes), or tabs Rust has for it (a tab moved out of
/// another window). The start screen waits for them rather than showing for a moment.
pub fn opens_documents(state: &AppState, label: &str) -> bool {
    state
        .startup_paths
        .lock()
        .is_ok_and(|p| p.as_ref().is_some_and(|p| !p.is_empty()))
        || !state.windows.owned_by(label).is_empty()
}

/// Makes the window `label` for a tab moved out of `from`. The caller gives it the
/// document first, so the new window finds it when it starts.
pub fn create_for_tab(
    app: &AppHandle,
    from: &WebviewWindow,
    at_cursor: bool,
    label: &str,
) -> tauri::Result<()> {
    let place = Place::At(place_beside(app, from, at_cursor));
    let windows = &app.state::<AppState>().windows;
    windows.set_starting(label);
    create(app, label, place)
        .map(drop)
        .inspect_err(|_| windows.set_ready(label))
}

/// Waits until a window made for a tab has started (not long: `STARTING`), so that a
/// request to close it reaches its page.
pub fn wait_until_started(app: &AppHandle, label: &str) {
    let windows = &app.state::<AppState>().windows;
    while windows.is_starting(label) {
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub fn next_label(app: &AppHandle) -> String {
    app.state::<AppState>().windows.next_label()
}

/// Brings a window to the front.
pub fn focus(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The window that files from outside (a second launch of Lectrix) open in: the one that
/// last had the focus.
pub fn for_new_files(app: &AppHandle) -> Option<String> {
    let windows = app.webview_windows();
    let focused = app
        .state::<AppState>()
        .windows
        .focused
        .lock()
        .ok()
        .and_then(|f| f.clone());
    focused
        .filter(|label| windows.contains_key(label))
        .or_else(|| windows.contains_key(MAIN).then(|| MAIN.to_owned()))
        .or_else(|| windows.into_keys().min())
}

/// The other Lectrix window the cursor is over, if any.
pub fn under_cursor(app: &AppHandle, except: &str) -> Option<String> {
    platform::current()
        .window_under_cursor(app)
        .filter(|label| label != except && !crate::audience::is_audience(label))
}

/// Gives what was just opened to the window that asked for it. A file that is open
/// already in another window is shown there instead (section 6.1: opening a file that is
/// open switches to its tab).
pub fn adopt(app: &AppHandle, label: &str, result: OpenResult) -> OpenResult {
    let windows = &app.state::<AppState>().windows;
    match &result {
        OpenResult::Opened { document } => windows.own(document.id, label),
        OpenResult::AlreadyOpen { id } => {
            if let Some(owner) = windows.owner(*id)
                && owner != label
            {
                if let Err(e) = app.emit_to(owner.as_str(), SHOW_DOCUMENT, *id) {
                    applog::warn(format!("could not show document {id} in its window: {e}"));
                }
                focus(app, &owner);
            }
        }
        OpenResult::NeedsPassword { .. } | OpenResult::Failed { .. } => {}
    }
    result
}

/// Hands document `id` to the window `to`, which is open: it adds the tab and comes to the
/// front.
pub fn move_into(app: &AppHandle, id: u32, to: &str) {
    app.state::<AppState>().windows.own(id, to);
    if let Err(e) = app.emit_to(to, DOCUMENTS_MOVED, ()) {
        applog::warn(format!("could not tell window {to} of its new tab: {e}"));
    }
    focus(app, to);
}

/// A window was destroyed. While other windows remain, the documents it showed are
/// closed (the user saved them or chose not to, at its close prompt). With the last
/// window Lectrix quits, and the process takes its documents with it.
pub fn destroyed(app: &AppHandle, label: &str) {
    if crate::audience::is_audience(label) {
        crate::audience::destroyed(app, label);
        return;
    }
    // Its presentation's screen goes with it.
    crate::audience::close_for(app, label);
    let state = app.state::<AppState>();
    let ids = state.windows.forget(label);
    if !crate::audience::app_windows(app).any(|l| l != label) {
        return;
    }
    for id in ids {
        if state.documents.close(id).is_some() {
            state.image_cache.remove_document(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ScrollMode;

    #[test]
    fn a_window_reopens_its_closed_tabs_the_last_closed_first() {
        let windows = Windows::default();
        assert_eq!(windows.remember_closed("main", PathBuf::from("a.pdf")), 1);
        assert_eq!(windows.remember_closed("main", PathBuf::from("b.pdf")), 2);
        assert_eq!(
            windows.remember_closed("window-2", PathBuf::from("c.pdf")),
            1
        );
        // Closed again, a file moves to the end rather than being in the list twice.
        assert_eq!(windows.remember_closed("main", PathBuf::from("a.pdf")), 2);
        assert_eq!(windows.take_closed("main"), Some(PathBuf::from("a.pdf")));
        assert_eq!(windows.take_closed("main"), Some(PathBuf::from("b.pdf")));
        assert_eq!(windows.take_closed("main"), None);
        assert_eq!(windows.closed_count("window-2"), 1);
        // A window that goes takes its closed tabs with it.
        windows.forget("window-2");
        assert_eq!(windows.closed_count("window-2"), 0);
    }

    #[test]
    fn a_window_keeps_its_last_closed_tabs_only() {
        let windows = Windows::default();
        for i in 0..CLOSED_KEPT + 5 {
            windows.remember_closed("main", PathBuf::from(format!("{i}.pdf")));
        }
        assert_eq!(windows.closed_count("main"), CLOSED_KEPT);
        let last = CLOSED_KEPT + 4;
        assert_eq!(
            windows.take_closed("main"),
            Some(PathBuf::from(format!("{last}.pdf")))
        );
    }

    #[test]
    fn a_window_size_for_tests_is_two_numbers() {
        assert_eq!(parse_size("1028x779"), Some((1028.0, 779.0)));
        assert_eq!(parse_size(" 800 x 600.5 "), Some((800.0, 600.5)));
        for wrong in ["", "1028", "1028x", "x779", "0x779", "wide x tall"] {
            assert_eq!(parse_size(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn documents_belong_to_one_window_and_move_between_them() {
        let windows = Windows::default();
        windows.own(1, MAIN);
        windows.own(3, MAIN);
        windows.own(2, "window-2");
        assert_eq!(windows.owned_by(MAIN), vec![1, 3]);
        assert_eq!(windows.owner(2).as_deref(), Some("window-2"));

        // Moved: the other window has it, and this one no longer does.
        windows.own(3, "window-2");
        assert_eq!(windows.owned_by(MAIN), vec![1]);
        assert_eq!(windows.owned_by("window-2"), vec![2, 3]);

        windows.release(2);
        assert_eq!(windows.owner(2), None);
        // A window that goes takes what it showed with it.
        assert_eq!(windows.forget("window-2"), vec![3]);
        assert_eq!(windows.owner(3), None);
        assert_eq!(windows.owned_by(MAIN), vec![1]);
    }

    #[test]
    fn a_tab_brings_its_own_state_once() {
        let windows = Windows::default();
        let handoff = TabHandoff {
            scroll_mode: ScrollMode::TwoPage,
            cover: true,
            signed_warning_accepted: false,
        };
        windows.set_handoff(4, handoff);
        assert_eq!(windows.take_handoff(4), Some(handoff));
        assert_eq!(windows.take_handoff(4), None);
        // A document closed on the way leaves nothing behind.
        windows.set_handoff(5, handoff);
        windows.release(5);
        assert_eq!(windows.take_handoff(5), None);
    }

    #[test]
    fn windows_are_numbered_from_the_second() {
        let windows = Windows::default();
        assert_eq!(windows.next_label(), "window-2");
        assert_eq!(windows.next_label(), "window-3");
    }

    #[test]
    fn per_window_flags() {
        let windows = Windows::default();
        assert!(!windows.drops_to_combine(MAIN));
        windows.set_drop_to_combine(MAIN, true);
        assert!(windows.drops_to_combine(MAIN));
        assert!(!windows.drops_to_combine("window-2"));
        // Minimizing reports a change once.
        assert!(windows.set_minimized(MAIN, true));
        assert!(!windows.set_minimized(MAIN, true));
        assert!(windows.set_minimized(MAIN, false));
        assert!(!windows.set_minimized(MAIN, false));
        windows.forget(MAIN);
        assert!(!windows.drops_to_combine(MAIN));
    }

    #[test]
    fn a_window_made_for_a_tab_is_starting_until_its_page_runs() {
        let windows = Windows::default();
        // The first window never is: it closes as it always did.
        assert!(!windows.is_starting(MAIN));
        windows.set_starting("window-2");
        assert!(windows.is_starting("window-2"));
        windows.set_ready("window-2");
        assert!(!windows.is_starting("window-2"));
        // Gone before it started.
        windows.set_starting("window-3");
        windows.forget("window-3");
        assert!(!windows.is_starting("window-3"));
    }
}
