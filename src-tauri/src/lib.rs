//! Folio app crate: a thin layer of IPC commands, the page-image protocol and windowing
//! over `pdf-core`.

mod ipc;
mod protocol;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use pdf_core::session::Session;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use ipc::{AppError, DocumentInfo, PageSize, StartupTimings};

/// Working product name. Change it here, in `src/lib/config.ts` and in tauri.conf.json.
pub const APP_NAME: &str = "Folio";

static MAIN_START: OnceLock<Instant> = OnceLock::new();

#[derive(Default)]
pub struct AppState {
    documents: Mutex<HashMap<u32, Session>>,
    next_id: AtomicU32,
    /// A PDF passed on the command line (file association or `folio file.pdf`).
    startup_path: Mutex<Option<PathBuf>>,
}

impl AppState {
    fn session(&self, id: u32) -> Option<Session> {
        self.documents.lock().ok()?.get(&id).cloned()
    }

    /// Opens a path that came from the dialog, drag-and-drop or the command line. The
    /// webview never supplies paths itself (section 2, security).
    fn open(&self, path: &Path) -> Result<DocumentInfo, AppError> {
        let (session, info) = Session::open(path)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.documents
            .lock()
            .map_err(|_| AppError::new("The app is in a bad state.", Some("Restart Folio.")))?
            .insert(id, session);
        Ok(DocumentInfo {
            id,
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Document".into()),
            page_count: u32::try_from(info.page_count).unwrap_or(u32::MAX),
            pages: info
                .pages
                .iter()
                .map(|p| PageSize {
                    width: p.width,
                    height: p.height,
                })
                .collect(),
            open_ms: info.open_time.as_secs_f64() * 1000.0,
        })
    }
}

/// Shows the Open dialog and opens the chosen PDF. Runs off the main thread (`async`) so
/// the blocking dialog call cannot stall the event loop.
#[tauri::command(async)]
fn open_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<DocumentInfo>, AppError> {
    let picked = app
        .dialog()
        .file()
        .add_filter("PDF documents", &["pdf"])
        .blocking_pick_file();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|_| {
        AppError::new(
            "That location can't be opened.",
            Some("Pick a file on a local or network drive."),
        )
    })?;
    state.open(&path).map(Some)
}

/// Opens the PDF given on the command line, once.
#[tauri::command(async)]
fn open_startup_document(state: State<'_, AppState>) -> Result<Option<DocumentInfo>, AppError> {
    let path = state.startup_path.lock().ok().and_then(|mut p| p.take());
    match path {
        Some(path) => state.open(&path).map(Some),
        None => Ok(None),
    }
}

#[tauri::command]
fn close_document(state: State<'_, AppState>, id: u32) {
    if let Some(session) = state.documents.lock().ok().and_then(|mut d| d.remove(&id)) {
        session.close();
    }
}

/// Called by the frontend on first mount; reports startup time.
#[tauri::command]
fn app_ready() -> StartupTimings {
    let elapsed = MAIN_START.get().map(Instant::elapsed).unwrap_or_default();
    StartupTimings {
        main_to_ready_ms: elapsed.as_secs_f64() * 1000.0,
    }
}

/// Prints a frontend timing to stdout as `[folio-metric] name=value`, for scripted
/// performance measurements (section 2 targets).
#[tauri::command]
fn log_metric(name: String, ms: f64) {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(64)
        .collect();
    println!("[folio-metric] {name}={ms:.1}");
}

fn startup_path_from_args() -> Option<PathBuf> {
    std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) && p.is_file())
}

pub fn run() {
    MAIN_START.get_or_init(Instant::now);
    let state = AppState {
        startup_path: Mutex::new(startup_path_from_args()),
        ..AppState::default()
    };
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .register_asynchronous_uri_scheme_protocol("folio", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // Rendering is CPU-bound: keep it off the webview's thread.
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                responder.respond(protocol::handle(&state, &request));
            });
        })
        .invoke_handler(tauri::generate_handler![
            open_with_dialog,
            open_startup_document,
            close_document,
            app_ready,
            log_metric
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("{APP_NAME} could not start: {e}");
        std::process::exit(1);
    }
}
