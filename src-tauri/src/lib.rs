//! Lectrix app crate: a thin layer of IPC commands, the page-image protocol, windowing and
//! file handling over `pdf-core`.

mod annotations;
mod applog;
mod combine;
mod commands;
mod documents;
mod ipc;
mod menu;
mod placement;
mod platform;
mod protect;
mod protocol;
mod recovery;
mod store;
mod unprotect;
mod update;
mod windows;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use pdf_core::render::ImageCache;
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, RunEvent, WindowEvent};

use documents::Documents;
use ipc::OpenResult;
use platform::Platform;
use protocol::RenderGate;
use recovery::Recovery;
use store::Store;

/// Working product name. Change it here, in `src/lib/config.ts` and in tauri.conf.json.
pub const APP_NAME: &str = "Lectrix";

/// Memory for rendered page images kept for reuse (AGENTS.md section 3). Override with
/// the LECTRIX_IMAGE_CACHE_MB environment variable.
const IMAGE_CACHE_MB: usize = 64;

/// MuPDF's own resource store (decoded images, fonts). Its default is 256 MB, more than
/// the whole idle-memory target (ADR 0002).
const MUPDF_STORE_MB: usize = 96;

pub(crate) static MAIN_START: OnceLock<Instant> = OnceLock::new();

/// Event carrying `Vec<OpenResult>` for files opened outside a command (drag-and-drop,
/// a second launch with a file), for the window they open in.
const DOCUMENTS_OPENED: &str = "documents-opened";
/// Event carrying `FileChangedEvent`, for every window: the one that shows the document
/// acts on it.
const FILE_CHANGED: &str = "file-changed";
/// Event carrying `Vec<OpenResult>` for files dropped on a window's Combine view (opened
/// as sources, not tabs).
const MERGE_SOURCES_ADDED: &str = "merge-sources-added";
/// Event carrying `Settings`, for the windows other than the one that changed them.
pub(crate) const SETTINGS_CHANGED: &str = "settings-changed";
/// Event carrying `DocumentSaved`, for the window that shows a document another one saved.
pub(crate) const DOCUMENT_SAVED: &str = "document-saved";

pub struct AppState {
    documents: Documents,
    store: Mutex<Store>,
    image_cache: ImageCache,
    render_gate: RenderGate,
    renders: protocol::Renders,
    /// First-page previews of recent files, for the start screen's grid.
    previews: protocol::PreviewCache,
    /// PDFs passed on the command line, or handed over by the system (macOS: Finder) or a
    /// second launch before the first window asked for them, opened once the frontend is
    /// ready. `None` once that window has taken them.
    startup_paths: Mutex<Option<Vec<PathBuf>>>,
    platform: &'static dyn Platform,
    /// Which window shows which document, and what else differs between windows.
    windows: windows::Windows,
    /// Set while combining; storing `true` in it stops the merge.
    merge_cancel: Mutex<Option<Arc<AtomicBool>>>,
    updates: update::Updates,
    /// Files waiting to be unlocked (the start screen's Unlock).
    unprotect: unprotect::Unprotect,
    /// Files waiting to be protected (Protect as a tool on a file).
    protect: protect::Protect,
}

impl AppState {
    /// Opens a path that came from the dialog, drag-and-drop, the command line or the
    /// recent list. The webview never supplies paths itself (section 2, security).
    fn open(&self, path: &Path) -> OpenResult {
        self.documents.open(path, None, self.platform, &self.store)
    }

    /// [`AppState::open`] for a file whose password Lectrix knows (one it just protected).
    fn open_with_password(&self, path: &Path, password: Option<&str>) -> OpenResult {
        self.documents
            .open(path, password, self.platform, &self.store)
    }

    /// Opens a path from the same places as a source for combining or inserting pages.
    fn open_source(&self, path: &Path) -> OpenResult {
        self.documents
            .open_source(path, None, self.platform, &self.store)
    }
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// PDF paths among command-line arguments, made absolute against `cwd`.
fn pdf_args(args: impl IntoIterator<Item = String>, cwd: &Path) -> Vec<PathBuf> {
    args.into_iter()
        .map(PathBuf::from)
        .map(|p| if p.is_absolute() { p } else { cwd.join(p) })
        .filter(|p| is_pdf(p) && p.is_file())
        .collect()
}

/// Files handed over before the app was set up (macOS can send its open event before
/// that), for the first window to open with the command line's.
static EARLY_PATHS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Opens PDFs handed over while Lectrix runs: by a second launch (a double-click on a PDF
/// in File Explorer or a file manager) or by macOS (Finder, Open With, another app). Until
/// the first window has asked for the files it starts with, they are among those; after
/// that they open in the window used last, which comes to the front.
fn hand_over(app: &AppHandle, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        if let Ok(mut early) = EARLY_PATHS.lock() {
            early.extend(paths);
        }
        return;
    };
    let paths = match state.startup_paths.lock() {
        Ok(mut startup) => match startup.as_mut() {
            Some(waiting) => {
                waiting.extend(paths);
                return;
            }
            None => paths,
        },
        Err(_) => paths,
    };
    if let Some(label) = windows::for_new_files(app) {
        open_in_background(app, paths, false, label.clone());
        windows::focus(app, &label);
    }
}

/// The PDFs among files macOS asks Lectrix to open (`file://` URLs).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn opened_paths(urls: &[tauri::Url]) -> Vec<PathBuf> {
    urls.iter()
        .filter_map(|url| url.to_file_path().ok())
        .filter(|p| is_pdf(p) && p.is_file())
        .collect()
}

/// Opens paths off the main thread and tells the window `label`, which gets them: as
/// tabs, or as sources when they were dropped on its Combine view (`as_sources`).
fn open_in_background(app: &AppHandle, paths: Vec<PathBuf>, as_sources: bool, label: String) {
    let paths: Vec<PathBuf> = paths.into_iter().filter(|p| is_pdf(p)).collect();
    if paths.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let opened: Vec<OpenResult> = if as_sources {
            paths.iter().map(|p| state.open_source(p)).collect()
        } else {
            paths.iter().map(|p| state.open(p)).collect()
        };
        let results: Vec<OpenResult> = opened
            .into_iter()
            .map(|result| windows::adopt(&app, &label, result))
            .collect();
        let event = if as_sources {
            MERGE_SOURCES_ADDED
        } else {
            DOCUMENTS_OPENED
        };
        if let Err(e) = app.emit_to(label.as_str(), event, results) {
            applog::warn(format!("could not notify the window of opened files: {e}"));
        }
    });
}

/// Polls open files for changes made by other programs (section 7).
fn watch_files(app: AppHandle) {
    std::thread::Builder::new()
        .name("file-watch".into())
        .spawn(move || {
            loop {
                std::thread::sleep(documents::WATCH_INTERVAL);
                let state = app.state::<AppState>();
                for event in state.documents.poll_changes() {
                    if let Err(e) = app.emit(FILE_CHANGED, event) {
                        applog::warn(format!("could not report a file change: {e}"));
                    }
                }
            }
        })
        .map(drop)
        .unwrap_or_else(|e| applog::error(format!("file watching is off: {e}")));
}

/// Writes recovery copies of documents with unsaved changes, every two minutes
/// (section 7).
fn write_recovery_copies(app: AppHandle) {
    let interval = std::env::var("LECTRIX_RECOVERY_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or(recovery::INTERVAL);
    std::thread::Builder::new()
        .name("recovery".into())
        .spawn(move || {
            loop {
                std::thread::sleep(interval);
                app.state::<AppState>().documents.write_recovery_copies();
            }
        })
        .map(drop)
        .unwrap_or_else(|e| applog::error(format!("crash recovery is off: {e}")));
}

/// Where recovery copies go: the app's local data folder, or LECTRIX_RECOVERY_DIR (for
/// tests). Off for measurement and test runs (LECTRIX_EPHEMERAL) unless that is set.
fn recovery_dir(app: &AppHandle) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LECTRIX_RECOVERY_DIR") {
        return Some(PathBuf::from(dir));
    }
    if std::env::var_os("LECTRIX_EPHEMERAL").is_some() {
        return None;
    }
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join("recovery"))
}

fn dark(appearance: ipc::Appearance) -> Option<bool> {
    match appearance {
        ipc::Appearance::System => None,
        ipc::Appearance::Light => Some(false),
        ipc::Appearance::Dark => Some(true),
    }
}

/// Applies the Settings appearance to every window: its frame, background and the
/// webview's `prefers-color-scheme` (the frontend also marks the forced theme itself).
pub(crate) fn apply_appearance(app: &AppHandle, appearance: ipc::Appearance) {
    for window in app.webview_windows().values() {
        if let Err(e) = platform::current().set_appearance(window, dark(appearance)) {
            applog::warn(format!("could not change the window theme: {e}"));
        }
        paint_window_background(window);
    }
}

/// Gives a window just made the Settings appearance.
pub(crate) fn apply_appearance_to(window: &tauri::WebviewWindow, appearance: ipc::Appearance) {
    // As the system is: the window is that already.
    if appearance != ipc::Appearance::System
        && let Err(e) = platform::current().set_appearance(window, dark(appearance))
    {
        applog::warn(format!("could not change the window theme: {e}"));
    }
    paint_window_background(window);
}

/// The title bar's colour in the light and the dark theme (`--lectrix-chrome` in
/// src/app.css, ADR 0014; a test keeps them in step).
const CHROME_LIGHT: [u8; 3] = [0xf2, 0xf4, 0xf7];
const CHROME_DARK: [u8; 3] = [0x13, 0x14, 0x16];

/// Gives the window and the webview the chrome's colour for the theme in use, so the window
/// shows that, not white, before the page first paints and at its edges while resizing.
fn paint_window_background(window: &tauri::WebviewWindow) {
    let dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
    let [r, g, b] = if dark { CHROME_DARK } else { CHROME_LIGHT };
    if let Err(e) = window.set_background_color(Some(tauri::window::Color(r, g, b, 255))) {
        applog::warn(format!("could not set the window background: {e}"));
    }
}

/// Notes where a window is, for the next start (placement.rs).
fn remember_placement(window: &tauri::Window, write: bool) {
    if let Ok(mut store) = window.app_handle().state::<AppState>().store.lock() {
        placement::remember(window, &mut store, write);
    }
}

fn env_mb(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn run() {
    MAIN_START.get_or_init(Instant::now);
    if let Err(e) =
        mupdf::set_store_max_size(env_mb("LECTRIX_MUPDF_STORE_MB", MUPDF_STORE_MB) << 20)
    {
        eprintln!("could not limit MuPDF's store: {e}");
    }
    pdf_core::fonts::install();
    pdf_core::fonts::warm_up_in_background();

    let cwd = std::env::current_dir().unwrap_or_default();
    let mut startup_paths = pdf_args(std::env::args().skip(1), &cwd);
    // LECTRIX_OPEN (paths separated by ';') opens files like command-line arguments. For
    // automation: under tauri-driver on Windows, launch arguments go to WebView2, not to
    // the app (tests/e2e).
    if let Some(list) = std::env::var_os("LECTRIX_OPEN") {
        let list = list.to_string_lossy().into_owned();
        startup_paths.extend(pdf_args(
            list.split(';').filter(|s| !s.is_empty()).map(str::to_owned),
            &cwd,
        ));
    }

    let mut builder = tauri::Builder::default();
    // Must be the first plugin: a second launch (for example double-clicking another
    // PDF) hands its arguments to this instance and exits. Test and measurement runs
    // (LECTRIX_EPHEMERAL) are instances of their own: they neither hand their files to a
    // Lectrix the user has open, nor take the files of one started meanwhile.
    if std::env::var_os("LECTRIX_EPHEMERAL").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            hand_over(app, pdf_args(args.into_iter().skip(1), Path::new(&cwd)));
        }));
    }
    let result = builder
        .plugin(tauri_plugin_dialog::init())
        // Updates from GitHub releases (ADR 0011). Only Rust uses it: the webview has no
        // updater permission and goes through Lectrix's own commands in update.rs.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            if let Ok(dir) = app.path().app_log_dir() {
                applog::init(&dir);
            }
            applog::info(format!(
                "{APP_NAME} {} starting",
                app.package_info().version
            ));
            // LECTRIX_EPHEMERAL (used by tests/perf/measure.ps1) keeps measurement runs out
            // of the user's recent files and remembered views. LECTRIX_STATE_FILE gives a
            // test run a state file of its own (the e2e test of the window's place).
            let store_file = if let Some(file) = std::env::var_os("LECTRIX_STATE_FILE") {
                Some(PathBuf::from(file))
            } else if std::env::var_os("LECTRIX_EPHEMERAL").is_some() {
                None
            } else {
                app.path()
                    .app_local_data_dir()
                    .ok()
                    .map(|d| d.join("state.json"))
            };
            let recovery = Recovery::new(recovery_dir(app.handle()));
            let recovering = recovery.enabled();
            app.manage(AppState {
                documents: Documents::new(recovery),
                store: Mutex::new(Store::load(store_file)),
                image_cache: ImageCache::new(
                    env_mb("LECTRIX_IMAGE_CACHE_MB", IMAGE_CACHE_MB) << 20,
                ),
                render_gate: RenderGate::new(),
                renders: protocol::Renders::default(),
                previews: protocol::PreviewCache::default(),
                startup_paths: Mutex::new(Some(
                    startup_paths
                        .iter()
                        .cloned()
                        .chain(
                            EARLY_PATHS
                                .lock()
                                .map(|mut p| std::mem::take(&mut *p))
                                .unwrap_or_default(),
                        )
                        .collect(),
                )),
                platform: platform::current(),
                windows: windows::Windows::default(),
                merge_cancel: Mutex::new(None),
                updates: update::Updates::default(),
                unprotect: unprotect::Unprotect::default(),
                protect: protect::Protect::default(),
            });
            app.manage(menu::Menus::default());
            // Working copies of documents an earlier run never saved, and that no recovery
            // copy waits to bring back.
            app.state::<AppState>().documents.sweep_working_copies();
            watch_files(app.handle().clone());
            if recovering {
                write_recovery_copies(app.handle().clone());
            }
            // The first window, where the window was last time, maximized if it was.
            windows::create(app.handle(), windows::MAIN, windows::Place::Remembered)?;
            Ok(())
        })
        // The system's menu bar (macOS): a choice goes to the window whose menus it shows.
        .on_menu_event(|app, event| menu::chosen(app, &event))
        .on_window_event(|window, event| match event {
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                let app = window.app_handle();
                let label = window.label();
                let to_combine = app.state::<AppState>().windows.drops_to_combine(label);
                open_in_background(app, paths.clone(), to_combine, label.to_owned());
            }
            // A window made for a tab, asked to close before its page runs: the page could
            // not ask about the tab's unsaved changes yet, so the window stays.
            WindowEvent::CloseRequested { api, .. }
                if window
                    .app_handle()
                    .state::<AppState>()
                    .windows
                    .is_starting(window.label()) =>
            {
                api.prevent_close();
            }
            WindowEvent::Moved(_) => remember_placement(window, true),
            // A minimized window asks WebView2 to use less memory (ADR 0002).
            WindowEvent::Resized(_) => {
                remember_placement(window, true);
                let app = window.app_handle();
                if let (Ok(minimized), Some(webview)) = (
                    window.is_minimized(),
                    app.get_webview_window(window.label()),
                ) && app
                    .state::<AppState>()
                    .windows
                    .set_minimized(window.label(), minimized)
                {
                    platform::current().set_low_memory(&webview, minimized);
                }
            }
            // The window in use: files from a second launch open here, and its place is
            // the one Lectrix starts at next time.
            WindowEvent::Focused(true) => {
                window
                    .app_handle()
                    .state::<AppState>()
                    .windows
                    .set_focused(window.label());
                remember_placement(window, false);
                menu::focused(window.app_handle(), window.label());
            }
            WindowEvent::Destroyed => {
                menu::forget(window.app_handle(), window.label());
                windows::destroyed(window.app_handle(), window.label());
            }
            // The system switched between light and dark (with Settings on System).
            WindowEvent::ThemeChanged(_) => {
                if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                    paint_window_background(&webview);
                }
            }
            _ => {}
        })
        .register_asynchronous_uri_scheme_protocol("lectrix", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // Rendering is CPU-bound: keep it off the webview's thread.
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                responder.respond(protocol::handle(&state, &request));
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_with_dialog,
            commands::open_startup_documents,
            commands::open_recent,
            commands::remove_recent,
            commands::set_recent_pinned,
            commands::show_recent_in_folder,
            commands::list_recent_files,
            commands::list_recovered,
            commands::restore_recovered,
            commands::discard_recovered,
            commands::exit_confirmed,
            commands::unlock_document,
            commands::cancel_unlock,
            commands::unprotect_pick,
            commands::unprotect_password,
            commands::unprotect_save,
            commands::unprotect_cancel,
            commands::unprotect_document,
            commands::protect_pick,
            commands::protect_password,
            commands::protect_save,
            commands::protect_cancel,
            commands::set_document_protection,
            commands::get_document_protection,
            commands::close_document,
            commands::get_document_info,
            commands::list_open_documents,
            commands::get_page_text,
            commands::search_text,
            commands::apply_operation,
            commands::open_merge_sources,
            commands::open_insert_sources,
            commands::set_drop_target,
            commands::plan_merge,
            commands::execute_merge,
            commands::extract_pages,
            commands::cancel_merge,
            commands::set_bookmark_open,
            commands::enable_editing,
            commands::undo,
            commands::redo,
            commands::save,
            commands::save_as,
            commands::reload_document,
            commands::remember_view,
            commands::scan_annotations_for_repair,
            commands::repair_annotations,
            commands::get_settings,
            commands::set_settings,
            commands::reset_settings,
            commands::app_ready,
            commands::set_pane_layout,
            commands::log_metric,
            commands::log_error,
            commands::open_web_page,
            commands::window_chrome,
            commands::set_menu,
            commands::open_link,
            commands::move_document,
            commands::list_windows,
            commands::close_other_windows,
            update::check_for_update,
            update::skip_update,
            update::download_update,
            update::cancel_update_download,
            update::restart_to_update,
        ])
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|app, event| {
                // macOS hands files over as an event, never as arguments: at launch (a
                // PDF opened from Finder, Open With, another app) and while Lectrix runs.
                #[cfg(target_os = "macos")]
                if let RunEvent::Opened { urls } = &event {
                    hand_over(app, opened_paths(urls));
                }
                // Quitting normally: the user saved or chose not to save, so this run's
                // recovery copies go. Only a crash leaves them behind. The window's close
                // prompt already deleted them (exit_confirmed); this covers other exits.
                if let RunEvent::Exit = event {
                    let state = app.state::<AppState>();
                    // The window's place, kept in memory while it moved (placement.rs).
                    if let Ok(store) = state.store.lock() {
                        store.save();
                    }
                    state.documents.recovery().close();
                    // And so do the working copies of documents never saved under a name.
                    state.documents.discard_unsaved();
                    // Windows: an update downloaded in this run is installed now.
                    update::install_on_exit(&state);
                }
            })
        });
    if let Err(e) = result {
        applog::error(format!("{APP_NAME} could not start: {e}"));
        eprintln!("{APP_NAME} could not start: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Files macOS hands over arrive as `file://` URLs; only existing PDFs are opened.
    #[test]
    fn opened_urls_give_the_pdfs_that_exist() {
        let dir = std::env::temp_dir().join(format!("lectrix-opened-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("report with space.PDF");
        let text = dir.join("notes.txt");
        std::fs::write(&pdf, b"%PDF-1.7").unwrap();
        std::fs::write(&text, b"").unwrap();
        let url = |p: &Path| tauri::Url::from_file_path(p).unwrap();
        let urls = vec![
            url(&pdf),
            url(&text),
            url(&dir.join("gone.pdf")),
            tauri::Url::parse("https://example.com/a.pdf").unwrap(),
        ];
        assert_eq!(opened_paths(&urls), vec![pdf]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A command the webview may not call fails at run time only ("Something went wrong"),
    /// so every command handled here must be declared in build.rs and granted in the
    /// capability file.
    #[test]
    fn every_command_is_declared_and_granted() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        let lib = read("src/lib.rs");
        let declared = read("build.rs");
        let granted = read("capabilities/default.json");
        let handled = lib
            .split("generate_handler![")
            .nth(1)
            .and_then(|rest| rest.split("])").next())
            .unwrap();
        let names: Vec<&str> = handled
            .lines()
            .filter_map(|line| line.trim().trim_end_matches(',').rsplit("::").next())
            .filter(|name| !name.is_empty())
            .collect();
        assert!(names.len() > 40, "found only {} commands", names.len());
        for name in names {
            assert!(
                declared.contains(&format!("\"{name}\"")),
                "{name} is not in build.rs"
            );
            assert!(
                granted.contains(&format!("\"allow-{}\"", name.replace('_', "-"))),
                "{name} is not granted in capabilities/default.json"
            );
        }
    }

    #[test]
    fn window_background_is_the_chrome_colour() {
        let css =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/app.css"))
                .unwrap();
        let hex = |[r, g, b]: [u8; 3]| format!("--lectrix-chrome: #{r:02x}{g:02x}{b:02x};");
        // Once in the light theme; twice in the dark one (system and forced).
        assert_eq!(css.matches(&hex(CHROME_LIGHT)).count(), 1);
        assert_eq!(css.matches(&hex(CHROME_DARK)).count(), 2);
    }

    #[test]
    fn command_line_keeps_existing_pdfs_only() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-output/args");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.PDF"), b"%PDF").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let args = vec![
            "a.PDF".to_string(),
            "notes.txt".to_string(),
            "missing.pdf".to_string(),
            "--flag".to_string(),
        ];
        assert_eq!(pdf_args(args, &dir), vec![dir.join("a.PDF")]);
    }
}
