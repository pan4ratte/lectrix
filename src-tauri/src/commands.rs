//! IPC commands: thin wrappers that call `pdf-core` through the document registry.
//!
//! The webview never supplies file paths (AGENTS.md section 2): files come from Rust-side
//! dialogs, drag-and-drop, the command line, or the recent-files list kept in Rust.
//! Commands that may block (dialogs, opening, saving, search) run off the main thread
//! (`async`).

use std::path::PathBuf;
use std::time::Instant;

use pdf_core::save::SaveKind;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::ipc::{
    AppError, Backdrop, DocumentChange, DocumentInfo, ImageFormat, OpenResult, OperationInput,
    PageHits, PageText, RecentFile, SaveResult, SearchChunk, StartupInfo, ViewState,
};
use crate::{AppState, MAIN_START, platform};

/// The most pages one `search_text` call visits; the frontend searches in chunks so it
/// can show progress and stop early.
const MAX_SEARCH_CHUNK: u32 = 64;

fn picked_path(picked: tauri_plugin_dialog::FilePath) -> Result<PathBuf, AppError> {
    picked.into_path().map_err(|_| {
        AppError::new(
            "That location can't be used.",
            Some("Pick a file on a local or network drive."),
        )
    })
}

/// Shows the Open dialog (several files may be picked) and opens the chosen PDFs.
#[tauri::command(async)]
pub fn open_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<OpenResult>, AppError> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("PDF documents", &["pdf"])
        .blocking_pick_files()
    else {
        return Ok(Vec::new());
    };
    picked
        .into_iter()
        .map(|p| Ok(state.open(&picked_path(p)?)))
        .collect()
}

/// Opens the PDFs given on the command line (file association or `folio a.pdf b.pdf`),
/// once.
#[tauri::command(async)]
pub fn open_startup_documents(state: State<'_, AppState>) -> Vec<OpenResult> {
    let paths = state
        .startup_paths
        .lock()
        .map(|mut p| std::mem::take(&mut *p))
        .unwrap_or_default();
    paths.iter().map(|p| state.open(p)).collect()
}

#[tauri::command(async)]
pub fn open_recent(state: State<'_, AppState>, index: u32) -> Result<OpenResult, AppError> {
    let path = state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .recent()
        .get(index as usize)
        .map(|e| e.path.clone())
        .ok_or_else(|| AppError::new("That file is no longer in the recent list.", None))?;
    Ok(state.open(&path))
}

#[tauri::command]
pub fn remove_recent(state: State<'_, AppState>, index: u32) -> Result<(), AppError> {
    state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .remove_recent(index as usize);
    Ok(())
}

#[tauri::command]
pub fn list_recent_files(state: State<'_, AppState>) -> Result<Vec<RecentFile>, AppError> {
    let store = state.store.lock().map_err(|_| AppError::bad_state())?;
    Ok(store
        .recent()
        .iter()
        .enumerate()
        .map(|(i, e)| RecentFile {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            name: e
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            folder: e
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            exists: e.path.is_file(),
        })
        .collect())
}

/// Retries an encrypted document with the password the user typed.
#[tauri::command(async)]
pub fn unlock_document(
    state: State<'_, AppState>,
    token: u32,
    password: String,
) -> Result<OpenResult, AppError> {
    state
        .documents
        .unlock(token, &password, state.platform, &state.store)
}

#[tauri::command]
pub fn cancel_unlock(state: State<'_, AppState>, token: u32) {
    state.documents.cancel_unlock(token);
}

#[tauri::command]
pub fn close_document(state: State<'_, AppState>, id: u32) {
    if state.documents.close(id).is_some() {
        state.image_cache.remove_document(id);
    }
}

#[tauri::command(async)]
pub fn get_document_info(state: State<'_, AppState>, id: u32) -> Result<DocumentInfo, AppError> {
    let session = state.documents.session(id)?;
    let path = state.documents.path(id)?;
    let view = state
        .store
        .lock()
        .ok()
        .and_then(|s| s.view(&state.platform.file_key(&path)));
    Ok(crate::documents::document_info(id, session.info()?, view))
}

#[tauri::command(async)]
pub fn get_page_text(state: State<'_, AppState>, id: u32, page: u32) -> Result<PageText, AppError> {
    let (text, revision) = state.documents.session(id)?.page_text(page as usize)?;
    Ok(PageText::new(page as usize, revision, text))
}

/// Searches pages `start .. start + count` (at most 64 per call).
#[tauri::command(async)]
pub fn search_text(
    state: State<'_, AppState>,
    id: u32,
    query: String,
    start: u32,
    count: u32,
) -> Result<SearchChunk, AppError> {
    let session = state.documents.session(id)?;
    let start = start as usize;
    let end = start + count.min(MAX_SEARCH_CHUNK) as usize;
    let (pages, revision) = session.search(&query, start..end)?;
    Ok(SearchChunk {
        revision,
        pages: pages
            .into_iter()
            .map(|p| PageHits {
                page: u32::try_from(p.page).unwrap_or(u32::MAX),
                hits: p
                    .hits
                    .into_iter()
                    .map(|h| h.quads.into_iter().flatten().collect())
                    .collect(),
            })
            .collect(),
    })
}

#[tauri::command(async)]
pub fn apply_operation(
    state: State<'_, AppState>,
    id: u32,
    operation: OperationInput,
) -> Result<DocumentChange, AppError> {
    let change = state.documents.session(id)?.apply(operation.into())?;
    Ok(change.into())
}

#[tauri::command(async)]
pub fn undo(state: State<'_, AppState>, id: u32) -> Result<DocumentChange, AppError> {
    Ok(state.documents.session(id)?.undo()?.into())
}

#[tauri::command(async)]
pub fn redo(state: State<'_, AppState>, id: u32) -> Result<DocumentChange, AppError> {
    Ok(state.documents.session(id)?.redo()?.into())
}

/// Saves the document to its own file (incrementally where possible).
#[tauri::command(async)]
pub fn save(state: State<'_, AppState>, id: u32) -> Result<SaveResult, AppError> {
    state.documents.save(
        id,
        SaveKind::Incremental,
        None,
        state.platform,
        &state.store,
    )
}

/// Shows the Save As dialog and saves there. `optimized` rewrites the file with garbage
/// collection and compression ("Save As (optimized)"). Returns null if the user cancels.
#[tauri::command(async)]
pub fn save_as(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u32,
    optimized: bool,
) -> Result<Option<SaveResult>, AppError> {
    let current = state.documents.path(id)?;
    let mut dialog = app.dialog().file().add_filter("PDF documents", &["pdf"]);
    if let Some(name) = current.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    if let Some(dir) = current.parent() {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let mut target = picked_path(picked)?;
    if target.extension().is_none() {
        target.set_extension("pdf");
    }
    let kind = if optimized {
        SaveKind::Optimized
    } else if state.platform.same_file(&target, &current) {
        SaveKind::Incremental
    } else {
        SaveKind::Full
    };
    state
        .documents
        .save(id, kind, Some(target), state.platform, &state.store)
        .map(Some)
}

/// Reads the file again after another program changed it, discarding unsaved changes.
#[tauri::command(async)]
pub fn reload_document(state: State<'_, AppState>, id: u32) -> Result<DocumentInfo, AppError> {
    state.image_cache.remove_document(id);
    state.documents.reload(id, state.platform, &state.store)
}

/// Remembers where the user is in a document, for the next time the file is opened.
#[tauri::command]
pub fn remember_view(state: State<'_, AppState>, id: u32, view: ViewState) -> Result<(), AppError> {
    let path = state.documents.path(id)?;
    let key = state.platform.file_key(&path);
    state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .set_view(key, view);
    Ok(())
}

/// Called by the frontend on first mount: startup time and how to style the window.
#[tauri::command]
pub fn app_ready() -> StartupInfo {
    let elapsed = MAIN_START.get().map(Instant::elapsed).unwrap_or_default();
    let platform = platform::current();
    StartupInfo {
        main_to_ready_ms: elapsed.as_secs_f64() * 1000.0,
        backdrop: match platform.backdrop() {
            platform::Backdrop::Mica => Backdrop::Mica,
            platform::Backdrop::Solid => Backdrop::Solid,
        },
        accent_color: platform.accent_color(),
        perf_mode: std::env::var_os("FOLIO_PERF").is_some(),
        image_format: match std::env::var("FOLIO_IMAGE_FORMAT").as_deref() {
            Ok("png") => ImageFormat::Png,
            _ => ImageFormat::Rgba,
        },
    }
}

/// Prints a frontend timing to stdout as `[folio-metric] name=value`, for scripted
/// performance measurements (section 2 targets).
#[tauri::command]
pub fn log_metric(name: String, ms: f64) {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(64)
        .collect();
    println!("[folio-metric] {name}={ms:.1}");
}

/// Records a frontend error in the app log (the user sees a plain message instead).
#[tauri::command]
pub fn log_error(message: String) {
    let message: String = message.chars().take(2000).collect();
    crate::applog::error(format!("frontend: {message}"));
}
