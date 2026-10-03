//! Folio app crate: a thin layer of IPC commands, the page-image protocol, windowing and
//! file handling over `pdf-core`.

mod applog;
mod commands;
mod documents;
mod ipc;
mod platform;
mod protocol;
mod store;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use pdf_core::render::ImageCache;
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, WindowEvent};

use documents::Documents;
use ipc::OpenResult;
use platform::Platform;
use protocol::RenderGate;
use store::Store;

/// Working product name. Change it here, in `src/lib/config.ts` and in tauri.conf.json.
pub const APP_NAME: &str = "Folio";

/// Memory for rendered page images kept for reuse (AGENTS.md section 3). Override with
/// the FOLIO_IMAGE_CACHE_MB environment variable.
const IMAGE_CACHE_MB: usize = 64;

/// MuPDF's own resource store (decoded images, fonts). Its default is 256 MB, more than
/// the whole idle-memory target (ADR 0002).
const MUPDF_STORE_MB: usize = 96;

pub(crate) static MAIN_START: OnceLock<Instant> = OnceLock::new();

/// Event carrying `Vec<OpenResult>` for files opened outside a command (drag-and-drop,
/// a second launch with a file).
const DOCUMENTS_OPENED: &str = "documents-opened";
/// Event carrying `FileChangedEvent`.
const FILE_CHANGED: &str = "file-changed";

pub struct AppState {
    documents: Documents,
    store: Mutex<Store>,
    image_cache: ImageCache,
    render_gate: RenderGate,
    /// PDFs passed on the command line, opened once the frontend is ready.
    startup_paths: Mutex<Vec<PathBuf>>,
    platform: &'static dyn Platform,
}

impl AppState {
    /// Opens a path that came from the dialog, drag-and-drop, the command line or the
    /// recent list. The webview never supplies paths itself (section 2, security).
    fn open(&self, path: &Path) -> OpenResult {
        self.documents.open(path, None, self.platform, &self.store)
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

/// Opens paths off the main thread and tells the frontend.
fn open_in_background(app: &AppHandle, paths: Vec<PathBuf>) {
    let paths: Vec<PathBuf> = paths.into_iter().filter(|p| is_pdf(p)).collect();
    if paths.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let results: Vec<OpenResult> = paths.iter().map(|p| state.open(p)).collect();
        if let Err(e) = app.emit(DOCUMENTS_OPENED, results) {
            applog::warn(format!("could not notify the window of opened files: {e}"));
        }
    });
}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
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

fn env_mb(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn run() {
    MAIN_START.get_or_init(Instant::now);
    if let Err(e) = mupdf::set_store_max_size(env_mb("FOLIO_MUPDF_STORE_MB", MUPDF_STORE_MB) << 20)
    {
        eprintln!("could not limit MuPDF's store: {e}");
    }
    pdf_core::fonts::install();
    pdf_core::fonts::warm_up_in_background();

    let cwd = std::env::current_dir().unwrap_or_default();
    let mut startup_paths = pdf_args(std::env::args().skip(1), &cwd);
    // FOLIO_OPEN (paths separated by ';') opens files like command-line arguments. For
    // automation: under tauri-driver on Windows, launch arguments go to WebView2, not to
    // the app (tests/e2e).
    if let Some(list) = std::env::var_os("FOLIO_OPEN") {
        let list = list.to_string_lossy().into_owned();
        startup_paths.extend(pdf_args(
            list.split(';').filter(|s| !s.is_empty()).map(str::to_owned),
            &cwd,
        ));
    }

    let result = tauri::Builder::default()
        // Must be the first plugin: a second launch (for example double-clicking another
        // PDF) hands its arguments to this instance and exits.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let paths = pdf_args(args.into_iter().skip(1), Path::new(&cwd));
            open_in_background(app, paths);
            focus_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            if let Ok(dir) = app.path().app_log_dir() {
                applog::init(&dir);
            }
            applog::info(format!("{APP_NAME} {} starting", env!("CARGO_PKG_VERSION")));
            // FOLIO_EPHEMERAL (used by tests/perf/measure.ps1) keeps measurement runs out
            // of the user's recent files and remembered views.
            let store_file = if std::env::var_os("FOLIO_EPHEMERAL").is_some() {
                None
            } else {
                app.path()
                    .app_local_data_dir()
                    .ok()
                    .map(|d| d.join("state.json"))
            };
            app.manage(AppState {
                documents: Documents::default(),
                store: Mutex::new(Store::load(store_file)),
                image_cache: ImageCache::new(env_mb("FOLIO_IMAGE_CACHE_MB", IMAGE_CACHE_MB) << 20),
                render_gate: RenderGate::new(),
                startup_paths: Mutex::new(startup_paths.clone()),
                platform: platform::current(),
            });
            watch_files(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                open_in_background(window.app_handle(), paths.clone());
            }
        })
        .register_asynchronous_uri_scheme_protocol("folio", |ctx, request, responder| {
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
            commands::list_recent_files,
            commands::unlock_document,
            commands::cancel_unlock,
            commands::close_document,
            commands::get_document_info,
            commands::get_page_text,
            commands::search_text,
            commands::apply_operation,
            commands::set_bookmark_open,
            commands::undo,
            commands::redo,
            commands::save,
            commands::save_as,
            commands::reload_document,
            commands::remember_view,
            commands::app_ready,
            commands::log_metric,
            commands::log_error,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        applog::error(format!("{APP_NAME} could not start: {e}"));
        eprintln!("{APP_NAME} could not start: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
