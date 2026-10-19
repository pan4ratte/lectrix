//! IPC commands: thin wrappers that call `pdf-core` through the document registry.
//!
//! The webview never supplies file paths (AGENTS.md section 2): files come from Rust-side
//! dialogs, drag-and-drop, the command line, or the recent-files list kept in Rust.
//! Commands that may block (dialogs, opening, saving, search) run off the main thread
//! (`async`).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use pdf_core::merge::{LabelMode as CoreLabelMode, MergeOptions, PagePick};
use pdf_core::save::SaveKind;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::annotations::RepairSummary;
use crate::combine::{self, Progress};
use crate::ipc::{
    AppError, CompressOutcome, CompressProgress, CompressSaved, CompressSettings, CompressStep,
    DocumentChange, DocumentInfo, DocumentSaved, ImageFormat, LabelMode, LinkPlace, MenuEntry,
    MergeOutcome, MergePlan, MergeProgress, MergeRequest, MergeStage, MoveTarget, OpenResult,
    OperationInput, PageHits, PageText, PaneLayout, PrintDone, PrintLayout, PrintProgress,
    PrintSettings, PrintStart, PrinterDetails, ProtectSaved, ProtectStep, ProtectionChangeInput,
    ProtectionInfo, ProtectionInput, RecentFile, RecoveredDocument, SaveResult, SearchChunk,
    SearchWord, Settings, SettingsInput, StartupInfo, TabHandoff, UnprotectSaved, UnprotectStep,
    UnsavedSource, ViewState, WebPage, WindowChrome, WindowDocument, WindowEntry,
};
use crate::platform::Platform;
use crate::store::StoredSettings;
use crate::{AppState, MAIN_START, windows};

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

/// Answers file dialogs from the LECTRIX_DIALOG environment variable instead of showing
/// them, for automation (tests/e2e), like LECTRIX_OPEN: answers separated by `;`, one per
/// dialog in order, several files separated by `|`, an empty answer for Cancel. The paths
/// come from the environment the app was started with, never from the webview.
fn scripted_answer() -> Option<Option<Vec<PathBuf>>> {
    static ANSWERS: OnceLock<Option<Mutex<VecDeque<String>>>> = OnceLock::new();
    let answers = ANSWERS.get_or_init(|| {
        std::env::var("LECTRIX_DIALOG")
            .ok()
            .map(|v| Mutex::new(v.split(';').map(str::to_owned).collect()))
    });
    let answer = answers
        .as_ref()?
        .lock()
        .ok()?
        .pop_front()
        .unwrap_or_default();
    let paths: Vec<PathBuf> = answer
        .split('|')
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect();
    Some((!paths.is_empty()).then_some(paths))
}

/// Shows the Open dialog for PDFs. Returns no paths if the user cancels.
fn pick_pdfs(
    app: &AppHandle,
    title: Option<&str>,
    multiple: bool,
) -> Result<Vec<PathBuf>, AppError> {
    if let Some(answer) = scripted_answer() {
        return Ok(answer.unwrap_or_default());
    }
    let mut dialog = app.dialog().file().add_filter("PDF documents", &["pdf"]);
    if let Some(title) = title {
        dialog = dialog.set_title(title);
    }
    let picked = if multiple {
        dialog.blocking_pick_files().unwrap_or_default()
    } else {
        dialog.blocking_pick_file().into_iter().collect()
    };
    picked.into_iter().map(picked_path).collect()
}

/// Shows the Save dialog for a PDF and returns the chosen path (with `.pdf` added if it
/// has no extension), or None if the user cancels.
fn pick_save_target(
    app: &AppHandle,
    title: Option<&str>,
    file_name: Option<String>,
    directory: Option<&Path>,
) -> Result<Option<PathBuf>, AppError> {
    let picked = match scripted_answer() {
        Some(answer) => answer.and_then(|paths| paths.into_iter().next()),
        None => {
            let mut dialog = app.dialog().file().add_filter("PDF documents", &["pdf"]);
            if let Some(title) = title {
                dialog = dialog.set_title(title);
            }
            if let Some(name) = file_name {
                dialog = dialog.set_file_name(name);
            }
            if let Some(dir) = directory {
                dialog = dialog.set_directory(dir);
            }
            dialog.blocking_save_file().map(picked_path).transpose()?
        }
    };
    Ok(picked.map(|mut target| {
        if target.extension().is_none() {
            target.set_extension("pdf");
        }
        target
    }))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Shows the Open dialog (several files may be picked) and opens the chosen PDFs.
#[tauri::command(async)]
pub fn open_with_dialog(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<OpenResult>, AppError> {
    Ok(pick_pdfs(&app, None, true)?
        .iter()
        .map(|p| windows::adopt(&app, window.label(), state.open(p)))
        .collect())
}

/// Opens the PDFs given on the command line (file association or `lectrix a.pdf b.pdf`),
/// and those macOS or a second launch handed over meanwhile, once.
#[tauri::command(async)]
pub fn open_startup_documents(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Vec<OpenResult> {
    // Taken once: files handed over later open by themselves (`hand_over`).
    let paths = state
        .startup_paths
        .lock()
        .ok()
        .and_then(|mut p| p.take())
        .unwrap_or_default();
    paths
        .iter()
        .map(|p| windows::adopt(&app, window.label(), state.open(p)))
        .collect()
}

#[tauri::command(async)]
pub fn open_recent(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: u32,
) -> Result<OpenResult, AppError> {
    let path = state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .recent()
        .get(index as usize)
        .map(|e| e.path.clone())
        .ok_or_else(|| AppError::new("That file is no longer in the recent list.", None))?;
    Ok(windows::adopt(&app, window.label(), state.open(&path)))
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

/// Pins the recent file at `index` to the top of the list, or unpins it (section 6.1).
#[tauri::command]
pub fn set_recent_pinned(
    state: State<'_, AppState>,
    index: u32,
    pinned: bool,
) -> Result<(), AppError> {
    state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .set_recent_pinned(index as usize, pinned);
    Ok(())
}

/// Shows the recent file at `index` in the system's file manager, selected in its folder;
/// the folder alone when the file is no longer there (section 6.1). The path is the recent
/// list's, never one from the webview.
#[tauri::command(async)]
pub fn show_recent_in_folder(state: State<'_, AppState>, index: u32) -> Result<(), AppError> {
    let path = state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .recent()
        .get(index as usize)
        .map(|e| e.path.clone())
        .ok_or_else(|| AppError::new("That file is no longer in the recent list.", None))?;
    show_in_folder(state.platform, &path)
}

/// What the system's file manager is called, for messages.
const FILE_MANAGER: &str = if cfg!(windows) {
    "File Explorer"
} else if cfg!(target_os = "macos") {
    "Finder"
} else {
    "your file manager"
};

/// The file selected in its folder, or the folder alone if the file is gone. With
/// LECTRIX_FOLDER_LOG set (automation, tests/e2e), what would be shown is written to that
/// file instead and nothing opens.
fn show_in_folder(platform: &dyn Platform, path: &std::path::Path) -> Result<(), AppError> {
    let folder = path.parent().filter(|f| f.is_dir()).ok_or_else(|| {
        AppError::new(
            "The file’s folder is no longer there.",
            Some("It may have been moved, renamed or deleted, or be on a drive that isn’t connected."),
        )
    })?;
    let file = path.is_file().then_some(path);
    if let Some(log) = std::env::var_os("LECTRIX_FOLDER_LOG") {
        use std::io::Write;
        let shown = file.unwrap_or(folder);
        return std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .and_then(|mut f| writeln!(f, "{}", shown.display()))
            .map_err(|e| {
                AppError::new("The folder log couldn’t be written.", Some(&e.to_string()))
            });
    }
    let shown = match file {
        Some(file) => platform.show_in_folder(file),
        None => platform.open_folder(folder),
    };
    shown.map_err(|e| {
        crate::applog::warn(format!(
            "could not show {} in its folder: {e}",
            path.display()
        ));
        AppError::new(
            "The file’s folder couldn’t be opened.",
            Some(&format!("Open it from {FILE_MANAGER} instead.")),
        )
    })
}

#[tauri::command]
pub fn list_recent_files(state: State<'_, AppState>) -> Result<Vec<RecentFile>, AppError> {
    let store = state.store.lock().map_err(|_| AppError::bad_state())?;
    Ok(store
        .recent()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let size = std::fs::metadata(&e.path)
                .ok()
                .filter(std::fs::Metadata::is_file)
                .map(|m| m.len());
            RecentFile {
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
                exists: size.is_some(),
                opened_at: e.opened_at.saturating_mul(1000),
                size,
                pinned: e.pinned,
            }
        })
        .collect())
}

/// Unsaved changes that a crash left behind (section 7), newest first.
#[tauri::command]
pub fn list_recovered(state: State<'_, AppState>) -> Vec<RecoveredDocument> {
    state
        .documents
        .recovery()
        .pending()
        .into_iter()
        .map(|p| RecoveredDocument {
            name: file_name(&p.path),
            folder: p
                .path
                .parent()
                .map(|f| f.display().to_string())
                .unwrap_or_default(),
            saved_at: p
                .saved_at
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64() * 1000.0)
                .unwrap_or(0.0),
            exists: p.path.is_file(),
            slot: p.slot,
        })
        .collect()
}

/// Opens recovered documents as tabs, with their unsaved changes.
#[tauri::command(async)]
pub fn restore_recovered(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    slots: Vec<String>,
) -> Vec<OpenResult> {
    slots
        .iter()
        .map(|slot| {
            let restored = state.documents.restore(slot, state.platform, &state.store);
            windows::adopt(&app, window.label(), restored)
        })
        .collect()
}

/// Deletes recovered changes the user chose not to keep, and with them the working copies
/// of documents that had not been saved under a name.
#[tauri::command]
pub fn discard_recovered(state: State<'_, AppState>, slots: Vec<String>) {
    state.documents.recovery().discard_pending(&slots);
    state.documents.sweep_working_copies();
}

/// The user agreed to close the window (saved or chose not to save). If it is the last
/// one, Lectrix quits: this run's recovery copies go now, not only when the event loop
/// ends, so a quit cut short does not bring them back. With other windows open, only this
/// window's documents close, once it is gone (`windows::destroyed`).
#[tauri::command]
pub fn exit_confirmed(app: AppHandle, window: WebviewWindow, state: State<'_, AppState>) {
    if !app.webview_windows().keys().any(|l| l != window.label()) {
        state.documents.recovery().close();
    }
}

/// Retries an encrypted document with the password the user typed.
#[tauri::command(async)]
pub fn unlock_document(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    token: u32,
    password: String,
) -> Result<OpenResult, AppError> {
    let result = state
        .documents
        .unlock(token, &password, state.platform, &state.store)?;
    Ok(windows::adopt(&app, window.label(), result))
}

#[tauri::command]
pub fn cancel_unlock(state: State<'_, AppState>, token: u32) {
    state.documents.cancel_unlock(token);
}

/// The start screen's Unlock (ADR 0025): asks for a PDF and opens it to remove its
/// password and restrictions.
#[tauri::command(async)]
pub fn unprotect_pick(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UnprotectStep, AppError> {
    let Some(path) = pick_pdfs(&app, Some("Unlock a PDF"), false)?
        .into_iter()
        .next()
    else {
        return Ok(UnprotectStep::Cancelled);
    };
    let token = state.documents.new_token();
    Ok(state.unprotect.inspect(token, path, None))
}

/// Tries the password the user typed for the file Unlock is waiting on.
#[tauri::command(async)]
pub fn unprotect_password(
    state: State<'_, AppState>,
    token: u32,
    password: String,
) -> Result<UnprotectStep, AppError> {
    state.unprotect.with_password(token, password)
}

/// Writes the file Unlock opened without its password and restrictions, and opens it in a
/// tab: over the file itself, or (`copy`) to a file the Save dialog names, the original
/// then only read. A Save dialog given up writes nothing and keeps the file waiting, so
/// the choice can be made again.
#[tauri::command(async)]
pub fn unprotect_save(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    token: u32,
    copy: bool,
) -> Result<UnprotectSaved, AppError> {
    let (path, password) = state.unprotect.ready(token)?;
    let target = if copy {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".into());
        let Some(target) = pick_save_target(
            &app,
            Some("Save an unlocked copy"),
            Some(format!("{stem} (unlocked).pdf")),
            path.parent(),
        )?
        else {
            return Ok(UnprotectSaved::Cancelled);
        };
        target
    } else {
        path.clone()
    };
    state.unprotect.cancel(token);
    let replaced = state.platform.same_file(&path, &target);
    let write = || -> Result<(), AppError> {
        pdf_core::unprotect::remove_protection(&path, password.as_deref(), &target)?;
        crate::applog::info(format!(
            "unlocked {} into {}",
            path.display(),
            target.display()
        ));
        Ok(())
    };
    // A file open in a tab is read again there: its document has no restrictions now.
    let reloaded = match state.documents.clean_tab_of(&target, state.platform) {
        Some(id) => {
            state.image_cache.remove_document(id);
            Some(
                state
                    .documents
                    .rewrite(id, write, None, state.platform, &state.store)
                    .map(Box::new)?,
            )
        }
        None => {
            write()?;
            None
        }
    };
    Ok(UnprotectSaved::Saved {
        name: file_name(&target),
        replaced,
        opened: windows::adopt(&app, window.label(), state.open(&target)),
        reloaded,
    })
}

/// Unlock for document `id`, open in a tab: its file, opened with the password the
/// document was opened with, if it had one. What follows is as for a picked file.
#[tauri::command(async)]
pub fn unprotect_document(state: State<'_, AppState>, id: u32) -> Result<UnprotectStep, AppError> {
    let (path, password) = state.documents.file_of(id)?;
    let token = state.documents.new_token();
    Ok(state.unprotect.inspect(token, path, password))
}

#[tauri::command]
pub fn unprotect_cancel(state: State<'_, AppState>, token: u32) {
    state.unprotect.cancel(token);
}

/// Protect as a tool on a file (section 6.9, ADR 0026): asks for a PDF and opens it, to
/// give it a password, restrictions or both.
#[tauri::command(async)]
pub fn protect_pick(app: AppHandle, state: State<'_, AppState>) -> Result<ProtectStep, AppError> {
    let Some(path) = pick_pdfs(&app, Some("Protect a PDF"), false)?
        .into_iter()
        .next()
    else {
        return Ok(ProtectStep::Cancelled);
    };
    let token = state.documents.new_token();
    Ok(state.protect.inspect(token, path, None))
}

/// Tries the password the user typed for the file Protect is waiting on.
#[tauri::command(async)]
pub fn protect_password(
    state: State<'_, AppState>,
    token: u32,
    password: String,
) -> Result<ProtectStep, AppError> {
    state.protect.with_password(token, password)
}

/// Writes the file Protect opened with `protection` in place of any it had, and opens it
/// in a tab: over the file itself, or (`copy`) to a file the Save dialog names, the
/// original then only read. A Save dialog given up writes nothing and keeps the file
/// waiting, so the choice can be made again.
#[tauri::command(async)]
pub fn protect_save(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    token: u32,
    copy: bool,
    protection: ProtectionInput,
) -> Result<ProtectSaved, AppError> {
    let settings = pdf_core::protection::ProtectionSettings::from(protection);
    settings.validate()?;
    let (path, password) = state.protect.ready(token)?;
    let target = if copy {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".into());
        let Some(target) = pick_save_target(
            &app,
            Some("Save a protected copy"),
            Some(format!("{stem} (protected).pdf")),
            path.parent(),
        )?
        else {
            return Ok(ProtectSaved::Cancelled);
        };
        target
    } else {
        path.clone()
    };
    state.protect.cancel(token);
    let replaced = state.platform.same_file(&path, &target);
    let mut pdfa_removed = None;
    let mut write = || -> Result<(), AppError> {
        pdfa_removed =
            pdf_core::protection::protect_file(&path, password.as_deref(), &settings, &target)?;
        crate::applog::info(format!(
            "protected {} into {}",
            path.display(),
            target.display()
        ));
        Ok(())
    };
    let reopen = settings.reopen_password().map(str::to_owned);
    // A file open in a tab is read again there, with its new password.
    let reloaded = match state.documents.clean_tab_of(&target, state.platform) {
        Some(id) => {
            state.image_cache.remove_document(id);
            Some(
                state
                    .documents
                    .rewrite(id, write, reopen.clone(), state.platform, &state.store)
                    .map(Box::new)?,
            )
        }
        None => {
            write()?;
            None
        }
    };
    let opened = match reloaded {
        Some(_) => state.open(&target),
        None => state.open_with_password(&target, reopen.as_deref()),
    };
    Ok(ProtectSaved::Saved {
        name: file_name(&target),
        replaced,
        opened: windows::adopt(&app, window.label(), opened),
        reloaded,
        pdfa_removed: pdfa_removed.map(Into::into),
    })
}

#[tauri::command]
pub fn protect_cancel(state: State<'_, AppState>, token: u32) {
    state.protect.cancel(token);
}

/// The Compress dialog's settings used last.
fn remembered_compress(state: &AppState) -> Option<CompressSettings> {
    state.store.lock().ok().and_then(|s| s.compress_settings())
}

/// Compress (section 6.11, ADR 0027): asks for a PDF, opens it and reads what its size is
/// made of.
#[tauri::command(async)]
pub fn compress_pick(app: AppHandle, state: State<'_, AppState>) -> Result<CompressStep, AppError> {
    let Some(path) = pick_pdfs(&app, Some("Compress a PDF"), false)?
        .into_iter()
        .next()
    else {
        return Ok(CompressStep::Cancelled);
    };
    let token = state.documents.new_token();
    Ok(state
        .compress
        .inspect(token, path, None, remembered_compress(&state)))
}

/// Tries the password the user typed for the file Compress is waiting on.
#[tauri::command(async)]
pub fn compress_password(
    state: State<'_, AppState>,
    token: u32,
    password: String,
) -> Result<CompressStep, AppError> {
    state
        .compress
        .with_password(token, password, remembered_compress(&state))
}

/// Compress for document `id`, open in a tab: its file as saved, opened with the password
/// the document was opened with.
#[tauri::command(async)]
pub fn compress_document(state: State<'_, AppState>, id: u32) -> Result<CompressStep, AppError> {
    let (path, password) = state.documents.file_of(id)?;
    let token = state.documents.new_token();
    Ok(state
        .compress
        .inspect(token, path, password, remembered_compress(&state)))
}

/// Compresses the file waiting under `token` with `settings` into a file of Lectrix's own,
/// reporting progress on `on_progress`, and remembers the settings. Stopped with
/// `compress_stop`, it fails with the `cancelled` code.
#[tauri::command(async)]
pub fn compress_run(
    state: State<'_, AppState>,
    token: u32,
    settings: CompressSettings,
    on_progress: Channel<CompressProgress>,
) -> Result<CompressOutcome, AppError> {
    if let Ok(mut store) = state.store.lock() {
        store.set_compress_settings(settings);
    }
    state.compress.run(token, settings, move |p| {
        // The webview may have gone away; compressing finishes regardless.
        let _ = on_progress.send(p);
    })
}

#[tauri::command]
pub fn compress_stop(state: State<'_, AppState>, token: u32) {
    state.compress.stop(token);
}

/// Writes the result made for `token` and opens it in a tab: over the file itself, or
/// (`copy`) to a file the Save dialog names, the original then only read. A Save dialog
/// given up writes nothing and keeps the result waiting.
#[tauri::command(async)]
pub fn compress_save(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    token: u32,
    copy: bool,
) -> Result<CompressSaved, AppError> {
    let (path, password) = state.compress.ready(token)?;
    let done = state.compress.result(token)?;
    let target = if copy {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".into());
        let Some(target) = pick_save_target(
            &app,
            Some("Save a compressed copy"),
            Some(format!("{stem} (compressed).pdf")),
            path.parent(),
        )?
        else {
            return Ok(CompressSaved::Cancelled);
        };
        target
    } else {
        path.clone()
    };
    let replaced = state.platform.same_file(&path, &target);
    let write = || -> Result<(), AppError> {
        pdf_core::save::replace_with_copy(&done.file, &target)?;
        crate::applog::info(format!(
            "compressed {} into {}",
            path.display(),
            target.display()
        ));
        Ok(())
    };
    // A file open in a tab is read again there.
    let reloaded = match state.documents.clean_tab_of(&target, state.platform) {
        Some(id) => {
            state.image_cache.remove_document(id);
            Some(
                state
                    .documents
                    .rewrite(id, write, password.clone(), state.platform, &state.store)
                    .map(Box::new)?,
            )
        }
        None => {
            write()?;
            None
        }
    };
    state.compress.cancel(token);
    let opened = match reloaded {
        Some(_) => state.open(&target),
        None => state.open_with_password(&target, password.as_deref()),
    };
    Ok(CompressSaved::Saved {
        name: file_name(&target),
        replaced,
        opened: windows::adopt(&app, window.label(), opened),
        reloaded,
        before: done.report.before,
        after: done.report.after,
        pdfa_removed: done.report.pdfa_removed.map(Into::into),
    })
}

/// Forgets the file waiting under `token`, and deletes its result.
#[tauri::command]
pub fn compress_cancel(state: State<'_, AppState>, token: u32) {
    state.compress.cancel(token);
}

/// Print (section 6.12, ADR 0028): the printers, and the dialog's choices used last.
#[tauri::command(async)]
pub fn print_start(state: State<'_, AppState>) -> Result<PrintStart, AppError> {
    Ok(PrintStart {
        printers: state.print.list()?,
        remembered: state.store.lock().ok().and_then(|s| s.print_preferences()),
    })
}

/// What `printer` takes and how it is set.
#[tauri::command(async)]
pub fn print_printer(
    state: State<'_, AppState>,
    printer: String,
) -> Result<PrinterDetails, AppError> {
    state.print.details(&printer)
}

/// Shows `printer`'s own settings dialog (its driver's, on Windows) over the window; its
/// details after, or null if the dialog was cancelled. The settings hold for the jobs that
/// follow while Lectrix runs.
#[tauri::command(async)]
pub fn print_printer_settings(
    window: WebviewWindow,
    state: State<'_, AppState>,
    printer: String,
) -> Result<Option<PrinterDetails>, AppError> {
    #[cfg(windows)]
    let parent = window.hwnd().ok().map(|h| h.0 as isize);
    #[cfg(not(windows))]
    let parent = {
        let _ = &window;
        None
    };
    state.print.settings_dialog(&printer, parent)
}

/// Where each page `settings` asks for goes on its sheet: the dialog's preview.
#[tauri::command(async)]
pub fn print_layout(
    state: State<'_, AppState>,
    id: u32,
    settings: PrintSettings,
) -> Result<PrintLayout, AppError> {
    let session = state.documents.session(id)?;
    state.print.layout(&session, &settings)
}

/// Prints document `id` as `settings` asks, reporting progress on `on_progress`, and
/// remembers the choices. Stopped with `print_stop`, it fails with the `cancelled` code.
#[tauri::command(async)]
pub fn print_run(
    state: State<'_, AppState>,
    id: u32,
    settings: PrintSettings,
    on_progress: Channel<PrintProgress>,
) -> Result<PrintDone, AppError> {
    let session = state.documents.session(id)?;
    let title = state
        .documents
        .path(id)?
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into());
    if let Ok(mut store) = state.store.lock() {
        store.set_print_preferences(settings.preferences());
    }
    state.print.run(id, session, title, settings, move |p| {
        // The webview may have gone away; printing finishes regardless.
        let _ = on_progress.send(p);
    })
}

#[tauri::command]
pub fn print_stop(state: State<'_, AppState>, id: u32) {
    state.print.stop(id);
}

/// Sets the protection document `id` is saved with next (section 6.9): `null` for its
/// file's own. Not an undo step.
#[tauri::command(async)]
pub fn set_document_protection(
    state: State<'_, AppState>,
    id: u32,
    protection: Option<ProtectionChangeInput>,
) -> Result<crate::ipc::DocumentState, AppError> {
    let session = state.documents.session(id)?;
    Ok(session.set_protection(protection.map(Into::into))?.into())
}

/// Document `id`'s properties (section 6.10): its file, what it declares, its protection.
#[tauri::command(async)]
pub fn get_document_properties(
    state: State<'_, AppState>,
    id: u32,
) -> Result<crate::ipc::DocumentProperties, AppError> {
    let session = state.documents.session(id)?;
    let properties = session.properties()?;
    let protection = session.protection()?.map(Into::into);
    let file = state.documents.saved_file(id)?;
    Ok(crate::ipc::DocumentProperties {
        folder: file
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string()),
        file_size: file
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len()),
        version: properties.version,
        description: properties.description.into(),
        created: properties.created,
        modified: properties.modified,
        creator: properties.creator,
        producer: properties.producer,
        tagged: properties.tagged,
        linearized: properties.linearized,
        signatures: properties.signatures,
        protection,
    })
}

/// The fonts pages `from` to `to` (0-based, `to` not included) of document `id` use, each
/// once (section 6.10): asked for a few pages at a time.
#[tauri::command(async)]
pub fn get_document_fonts(
    state: State<'_, AppState>,
    id: u32,
    from: u32,
    to: u32,
) -> Result<Vec<crate::ipc::UsedFont>, AppError> {
    let session = state.documents.session(id)?;
    let fonts = session.fonts(from as usize..to as usize)?;
    Ok(fonts.into_iter().map(Into::into).collect())
}

/// The protection document `id`'s file has, `null` if none (sections 6.9 and 6.10).
#[tauri::command(async)]
pub fn get_document_protection(
    state: State<'_, AppState>,
    id: u32,
) -> Result<Option<ProtectionInfo>, AppError> {
    let session = state.documents.session(id)?;
    Ok(session.protection()?.map(Into::into))
}

#[tauri::command]
pub fn close_document(state: State<'_, AppState>, id: u32) {
    if state.documents.close(id).is_some() {
        state.image_cache.remove_document(id);
    }
    state.windows.release(id);
}

#[tauri::command(async)]
pub fn get_document_info(state: State<'_, AppState>, id: u32) -> Result<DocumentInfo, AppError> {
    let session = state.documents.session(id)?;
    let path = state.documents.path(id)?;
    // A document not saved under a name yet has no view remembered for a file.
    let untitled = state.documents.is_unsaved(id);
    let view = if untitled {
        None
    } else {
        state
            .store
            .lock()
            .ok()
            .and_then(|s| s.view(&state.platform.file_key(&path)))
    };
    Ok(crate::documents::document_info(
        id,
        session.info()?,
        view,
        untitled,
    ))
}

/// The documents the asking window shows in tabs, for a page that loads after it got
/// them: a window made for a tab moved out of another, a window a tab was moved into
/// (`documents-moved`), or a webview that reloaded (after a renderer crash, or because a
/// test driver navigated it). A moved tab comes with what it kept of its own, once.
#[tauri::command(async)]
pub fn list_open_documents(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<WindowDocument>, AppError> {
    let tabs = state.documents.ids()?;
    let documents = state
        .windows
        .owned_by(window.label())
        .into_iter()
        .filter(|id| tabs.contains(id))
        .filter_map(|id| {
            let document = get_document_info(state.clone(), id).ok()?;
            Some(WindowDocument {
                document,
                handoff: state.windows.take_handoff(id),
            })
        })
        .collect();
    // A window made for a tab has it now, and can ask about its unsaved changes.
    state.windows.set_ready(window.label());
    Ok(documents)
}

// ----- windows (section 8) -----

/// Moves the tab of document `id` out of the asking window: into a new window, or into
/// another one that is open. Returns the label of the window that has it now, or null
/// when it stays (`MoveTarget::Cursor` found nowhere to go). The document itself stays as
/// it is, unsaved changes and undo history included; `handoff` is what the tab keeps
/// outside it, for its new window.
#[tauri::command(async)]
pub fn move_document(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: u32,
    target: MoveTarget,
    handoff: TabHandoff,
) -> Result<Option<String>, AppError> {
    let from = window.label();
    if state.windows.owner(id).as_deref() != Some(from) || !state.documents.ids()?.contains(&id) {
        return Err(AppError::document_closed());
    }
    // An open window to move into, or a new one: under the cursor, or beside this one.
    let (existing, at_cursor) = match target {
        MoveTarget::New => (None, false),
        MoveTarget::Cursor { or_new } => match windows::under_cursor(&app, from) {
            Some(label) => (Some(label), true),
            None if or_new => (None, true),
            None => return Ok(None),
        },
        MoveTarget::Window { label } => {
            if label == from || app.get_webview_window(&label).is_none() {
                return Err(AppError::new(
                    "That window is no longer open.",
                    Some("Move the tab to a new window instead."),
                ));
            }
            (Some(label), false)
        }
    };
    state.windows.set_handoff(id, handoff);
    // A change to its file that this window was told about: the next one is told again.
    state.documents.report_again(id);
    if let Some(label) = existing {
        windows::move_into(&app, id, &label);
        crate::applog::info(format!("document {id} moved to window {label}"));
        return Ok(Some(label));
    }
    let label = windows::next_label(&app);
    // Before the window is made: it asks for its documents as soon as it starts.
    state.windows.own(id, &label);
    if let Err(e) = windows::create_for_tab(&app, &window, at_cursor, &label) {
        crate::applog::error(format!("could not make window {label}: {e}"));
        state.windows.own(id, from);
        state.windows.take_handoff(id);
        return Err(AppError::new(
            "A new window couldn’t be opened.",
            Some("The tab stays where it is. Try again."),
        ));
    }
    crate::applog::info(format!("document {id} moved to the new window {label}"));
    Ok(Some(label))
}

/// Lectrix's other windows, for "Move to window", in the order they were made.
#[tauri::command]
pub fn list_windows(app: AppHandle, window: WebviewWindow) -> Vec<WindowEntry> {
    let mut others: Vec<WindowEntry> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, _)| label != window.label())
        .map(|(label, w)| WindowEntry {
            title: w.title().unwrap_or_default(),
            label,
        })
        .collect();
    // "main", then "window-2", "window-3"… by number.
    others.sort_by_key(|w| {
        w.label
            .rsplit('-')
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0)
    });
    others
}

/// File > Exit, or restarting into an update: asks every other window to close, as its
/// own close button would, so each asks about its unsaved changes first. The asking window
/// closes itself once they are gone (`list_windows` is then empty).
#[tauri::command(async)]
pub fn close_other_windows(app: AppHandle, window: WebviewWindow) {
    for (label, other) in app.webview_windows() {
        if label == window.label() {
            continue;
        }
        // One made a moment ago can't ask yet.
        windows::wait_until_started(&app, &label);
        // Where the user can see what it asks.
        let _ = other.unminimize();
        if let Err(e) = other.close() {
            crate::applog::warn(format!("could not ask window {label} to close: {e}"));
        }
    }
}

#[tauri::command(async)]
pub fn get_page_text(state: State<'_, AppState>, id: u32, page: u32) -> Result<PageText, AppError> {
    let session = state.documents.session(id)?;
    let (text, revision) = session.page_text(page as usize)?;
    // A page whose links can't be read is still read and selected.
    let links = match session.page_links(page as usize) {
        Ok((links, _)) => links,
        Err(e) => {
            crate::applog::warn(format!("document {id}: links of page {page}: {e:?}"));
            Vec::new()
        }
    };
    Ok(PageText::new(page as usize, revision, text, links))
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
    let mut words: Vec<SearchWord> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for word in pages.iter().flat_map(|p| &p.words) {
        match index.get(&word.text) {
            Some(&i) => words[i].count += word.count,
            None => {
                index.insert(word.text.clone(), words.len());
                words.push(SearchWord {
                    text: word.text.clone(),
                    count: word.count,
                });
            }
        }
    }
    Ok(SearchChunk {
        revision,
        words,
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
    let author = author(&state)?;
    let operation = operation.into_operation(&state.documents, &author)?;
    let change = state.documents.session(id)?.apply(operation)?;
    Ok(change.into())
}

// ----- annotations (sections 5 and 6.5) -----

/// The author name new annotations get: from Settings, else the Windows user name.
fn author(state: &AppState) -> Result<String, AppError> {
    let stored = state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .settings()
        .author
        .clone();
    Ok(stored.unwrap_or_else(|| default_author(state.platform)))
}

fn default_author(platform: &dyn Platform) -> String {
    platform
        .user_name()
        .unwrap_or_else(|| "Lectrix user".to_owned())
}

/// Counts the problems "Repair annotations" would fix (section 5.3).
#[tauri::command(async)]
pub fn scan_annotations_for_repair(
    state: State<'_, AppState>,
    id: u32,
) -> Result<RepairSummary, AppError> {
    Ok(state.documents.session(id)?.scan_annotations()?.into())
}

/// Fixes them, as one undo step, and logs every change with the annotation's object
/// number (section 5.3).
#[tauri::command(async)]
pub fn repair_annotations(state: State<'_, AppState>, id: u32) -> Result<DocumentChange, AppError> {
    let session = state.documents.session(id)?;
    let change = session.apply(pdf_core::ops::Operation::RepairAnnotations)?;
    let name = state
        .documents
        .path(id)
        .map(|p| file_name(&p))
        .unwrap_or_default();
    for c in change.repairs.iter().flatten() {
        crate::applog::info(format!("repair {name}: {c}"));
    }
    Ok(change.into())
}

// ----- settings -----

fn settings_of(stored: &StoredSettings, platform: &dyn Platform) -> Settings {
    let default_author = default_author(platform);
    Settings {
        author: stored
            .author
            .clone()
            .unwrap_or_else(|| default_author.clone()),
        default_author,
        appearance: stored.appearance,
        scroll_mode: stored.scroll_mode,
        toolbar_style: stored.toolbar_style,
        toolbar_dock: stored.toolbar_dock,
        toolbar_position: stored.toolbar_position,
        toolbar_visibility: stored.toolbar_visibility,
        quick_bar: stored.quick_bar,
        quick_tools: stored.quick_tools.clone(),
        check_for_updates: stored.check_for_updates,
        smooth_zoom: stored.smooth_zoom,
        smooth_annotation_scroll: stored.smooth_annotation_scroll,
        smooth_page_turn: stored.smooth_page_turn,
        smooth_bookmark_scroll: stored.smooth_bookmark_scroll,
        smooth_search_scroll: stored.smooth_search_scroll,
        search_markers: stored.search_markers,
        tooltip_delay_ms: stored.tooltip_delay_ms,
        open_comment_after_markup: stored.open_comment_after_markup,
        remember_annotation_style: stored.remember_annotation_style,
        remember_annotation_sort: stored.remember_annotation_sort,
        annotation_sort: stored.annotation_sort,
        selection_handles: stored.selection_handles,
        pdfa_read_only: stored.pdfa_read_only,
        highlight_links: stored.highlight_links,
        link_previews: stored.link_previews,
        page_adjustments: stored.page_adjustments,
        color_panel_on_hover: stored.color_panel_on_hover,
        preset_colors: stored.preset_colors.clone(),
        shortcuts: stored.shortcuts.clone(),
    }
}

/// The most commands with their own shortcuts, keys for one command, and characters in a
/// name or a key, that are stored: far more than the app has, so app data stays small
/// whatever the webview sends.
const MAX_SHORTCUT_COMMANDS: usize = 200;
const MAX_SHORTCUT_KEYS: usize = 4;
const MAX_SHORTCUT_CHARS: usize = 40;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    let store = state.store.lock().map_err(|_| AppError::bad_state())?;
    Ok(settings_of(store.settings(), state.platform))
}

/// Stores the settings and applies the appearance to the window at once.
#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    settings: SettingsInput,
) -> Result<Settings, AppError> {
    let author = settings.author.trim();
    let stored = StoredSettings {
        // The default name is not stored, so it follows the Windows account.
        author: (!author.is_empty() && author != default_author(state.platform))
            .then(|| author.chars().take(200).collect()),
        appearance: settings.appearance,
        scroll_mode: settings.scroll_mode,
        toolbar_style: settings.toolbar_style,
        toolbar_dock: settings.toolbar_dock,
        toolbar_position: settings.toolbar_position,
        toolbar_visibility: settings.toolbar_visibility,
        quick_bar: settings.quick_bar,
        quick_tools: settings
            .quick_tools
            .iter()
            .enumerate()
            .filter(|&(i, t)| !settings.quick_tools[..i].contains(t))
            .map(|(_, &t)| t)
            .collect(),
        check_for_updates: settings.check_for_updates,
        smooth_zoom: settings.smooth_zoom,
        smooth_annotation_scroll: settings.smooth_annotation_scroll,
        smooth_page_turn: settings.smooth_page_turn,
        smooth_bookmark_scroll: settings.smooth_bookmark_scroll,
        smooth_search_scroll: settings.smooth_search_scroll,
        search_markers: settings.search_markers,
        tooltip_delay_ms: settings
            .tooltip_delay_ms
            .min(crate::store::MAX_TOOLTIP_DELAY_MS),
        open_comment_after_markup: settings.open_comment_after_markup,
        remember_annotation_style: settings.remember_annotation_style,
        remember_annotation_sort: settings.remember_annotation_sort,
        // Not remembering it forgets it.
        annotation_sort: if settings.remember_annotation_sort {
            settings.annotation_sort
        } else {
            Default::default()
        },
        selection_handles: settings.selection_handles,
        pdfa_read_only: settings.pdfa_read_only,
        highlight_links: settings.highlight_links,
        link_previews: settings.link_previews,
        page_adjustments: settings.page_adjustments.clamped(),
        color_panel_on_hover: settings.color_panel_on_hover,
        // Only colours of presets the panel has, written one way.
        preset_colors: settings
            .preset_colors
            .iter()
            .filter(|&(&place, _)| place < crate::store::PRESET_COLOR_COUNT)
            .filter_map(|(&place, color)| Some((place, crate::store::preset_color(color)?)))
            .collect(),
        shortcuts: settings
            .shortcuts
            .iter()
            .take(MAX_SHORTCUT_COMMANDS)
            .map(|(command, keys)| {
                let short = |s: &String| s.chars().take(MAX_SHORTCUT_CHARS).collect::<String>();
                let keys = keys.iter().take(MAX_SHORTCUT_KEYS).map(short).collect();
                (short(command), keys)
            })
            .collect(),
    };
    store_settings(&app, &window, &state, stored)
}

/// Puts every setting back to its default, the shortcuts included (section 6.6).
#[tauri::command]
pub fn reset_settings(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Settings, AppError> {
    store_settings(&app, &window, &state, StoredSettings::default())
}

/// Stores the settings, applies the appearance to every window, and tells the other
/// windows, which put the settings into effect as the asking one does.
fn store_settings(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    stored: StoredSettings,
) -> Result<Settings, AppError> {
    let mut store = state.store.lock().map_err(|_| AppError::bad_state())?;
    store.set_settings(stored.clone());
    drop(store);
    crate::apply_appearance(app, stored.appearance);
    let settings = settings_of(&stored, state.platform);
    for label in app.webview_windows().into_keys() {
        if label != window.label()
            && let Err(e) = app.emit_to(label.as_str(), crate::SETTINGS_CHANGED, &settings)
        {
            crate::applog::warn(format!(
                "could not tell window {label} of the settings: {e}"
            ));
        }
    }
    Ok(settings)
}

// ----- combining files and inserting pages (section 6.4) -----

/// Shows the Open dialog for files to combine and opens them as sources (not tabs).
#[tauri::command(async)]
pub fn open_merge_sources(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<OpenResult>, AppError> {
    Ok(pick_pdfs(&app, Some("Add files to combine"), true)?
        .iter()
        .map(|p| windows::adopt(&app, window.label(), state.open_source(p)))
        .collect())
}

/// Shows the Open dialog for the files to insert pages from, and opens them as sources.
#[tauri::command(async)]
pub fn open_insert_sources(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<OpenResult>, AppError> {
    Ok(pick_pdfs(&app, Some("Insert pages from"), true)?
        .iter()
        .map(|p| windows::adopt(&app, window.label(), state.open_source(p)))
        .collect())
}

/// Where files dropped on the asking window go: to the grid of pages it shows (`true`: its
/// Combine view, or a document whose pages are being organized; they are opened as
/// sources and reported as `merge-sources-added`) or into tabs.
#[tauri::command]
pub fn set_drop_target(window: WebviewWindow, state: State<'_, AppState>, combine: bool) {
    state.windows.set_drop_to_combine(window.label(), combine);
}

/// Checks the sources before combining: files moved or deleted, and files open in a tab
/// with unsaved changes (combining reads files as saved).
#[tauri::command(async)]
pub fn plan_merge(state: State<'_, AppState>, sources: Vec<u32>) -> Result<MergePlan, AppError> {
    let mut plan = MergePlan {
        unsaved: Vec::new(),
        missing: Vec::new(),
    };
    for id in sources {
        let (path, _) = state.documents.source(id)?;
        if !path.is_file() {
            plan.missing.push(file_name(&path));
            continue;
        }
        for (tab, session) in state.documents.tabs_showing(&path, state.platform) {
            if session.info()?.state.dirty && !plan.unsaved.iter().any(|u| u.tab == tab) {
                plan.unsaved.push(UnsavedSource {
                    tab,
                    name: file_name(&path),
                });
            }
        }
    }
    Ok(plan)
}

/// Shows the Save dialog and combines the pages into the chosen file, reporting progress
/// on `on_progress`, then opens the result in a tab. Returns null if the user cancels the
/// dialog; a cancelled merge fails with the `cancelled` code and writes nothing.
#[tauri::command(async)]
pub fn execute_merge(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    request: MergeRequest,
    on_progress: Channel<MergeProgress>,
) -> Result<Option<MergeOutcome>, AppError> {
    if request.pages.is_empty() {
        return Err(AppError::new(
            "There are no pages to combine.",
            Some("Add files, or put back pages you removed."),
        ));
    }
    let sources = request
        .sources
        .iter()
        .map(|&id| state.documents.source(id))
        .collect::<Result<Vec<_>, _>>()?;
    let Some(target) = pick_save_target(
        &app,
        Some("Save combined file"),
        Some("Combined.pdf".into()),
        sources.first().and_then(|(p, _)| p.parent()),
    )?
    else {
        return Ok(None);
    };
    if let Some((path, _)) = sources
        .iter()
        .find(|(p, _)| state.platform.same_file(p, &target))
    {
        return Err(AppError::new(
            format!("{} is one of the files being combined.", file_name(path)),
            Some("Choose a new name: combining never changes the files it reads."),
        ));
    }
    if !state
        .documents
        .tabs_showing(&target, state.platform)
        .is_empty()
    {
        return Err(AppError::new(
            "That file is open in a tab.",
            Some("Close it there first, or choose a different name."),
        ));
    }

    let cancel = start_page_job(&state)?;
    let job = combine::Job {
        sources,
        picks: request
            .pages
            .iter()
            .map(|p| PagePick {
                source: p.source as usize,
                page: p.page as usize,
                rotate: p.rotation,
            })
            .collect(),
        options: MergeOptions {
            bookmarks: request.bookmarks.into(),
            labels: match request.labels {
                LabelMode::Keep => CoreLabelMode::KeepSources,
                LabelMode::Continuous => CoreLabelMode::Continuous,
                LabelMode::None => CoreLabelMode::None,
            },
        },
        target: target.clone(),
    };
    let started = Instant::now();
    let result = combine::run(job, cancel, report_progress(on_progress));
    if let Ok(mut current) = state.merge_cancel.lock() {
        *current = None;
    }
    let report = result?;
    crate::applog::info(format!(
        "combined {} pages into {} in {:.0} ms",
        report.pages,
        target.display(),
        started.elapsed().as_secs_f64() * 1000.0
    ));
    Ok(Some(MergeOutcome {
        opened: windows::adopt(&app, window.label(), state.open(&target)),
        report: report.into(),
        name: file_name(&target),
    }))
}

/// Claims the one slot for a job that copies pages into a new file (combining,
/// extracting) and returns the flag that stops it. The caller empties the slot when the
/// job ends.
fn start_page_job(state: &AppState) -> Result<Arc<AtomicBool>, AppError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut current = state
        .merge_cancel
        .lock()
        .map_err(|_| AppError::bad_state())?;
    if current.is_some() {
        return Err(AppError::new(
            "Lectrix is already combining files or extracting pages.",
            Some("Wait for it to finish, or stop it."),
        ));
    }
    *current = Some(cancel.clone());
    Ok(cancel)
}

/// Sends a page job's progress to the frontend.
fn report_progress(on_progress: Channel<MergeProgress>) -> impl Fn(Progress) + Send + 'static {
    move |p| {
        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        let message = match p {
            Progress::Copying { done, total } => MergeProgress {
                stage: MergeStage::Copying,
                done: n(done),
                total: n(total),
            },
            Progress::Writing => MergeProgress {
                stage: MergeStage::Writing,
                done: 0,
                total: 0,
            },
        };
        // The webview may have gone away; the job finishes regardless.
        let _ = on_progress.send(message);
    }
}

/// Copies `pages` (0-based, in this order) of document `id` into a new document, as the
/// document is now (unsaved changes included), reporting progress on `on_progress`; then
/// opens the new document in a tab. It is named "Pages from …" after the document, is not
/// saved under that name or any other yet (its file is a working copy of Lectrix's own),
/// and its Save dialog starts in the document's folder. The document itself is not
/// changed. An extraction stopped with `cancel_merge` fails with the `cancelled` code and
/// leaves nothing behind.
#[tauri::command(async)]
pub fn extract_pages(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: u32,
    pages: Vec<u32>,
    on_progress: Channel<MergeProgress>,
) -> Result<MergeOutcome, AppError> {
    if pages.is_empty() {
        return Err(AppError::new(
            "No pages are selected.",
            Some("Select the pages to extract first."),
        ));
    }
    let session = state.documents.session(id)?;
    let current = state.documents.path(id)?;
    let (_, folder) = state.documents.save_suggestion(id)?;
    let target = state
        .documents
        .new_working_copy(&format!("Pages from {}", file_name(&current)))?;

    let cancel = match start_page_job(&state) {
        Ok(cancel) => cancel,
        Err(e) => {
            state.documents.remove_working_copy(&target);
            return Err(e);
        }
    };
    let started = Instant::now();
    let result = combine::extract(
        &session,
        pages.into_iter().map(|p| p as usize).collect(),
        &target,
        cancel,
        report_progress(on_progress),
    );
    if let Ok(mut current) = state.merge_cancel.lock() {
        *current = None;
    }
    let report = match result {
        Ok(report) => report,
        Err(e) => {
            state.documents.remove_working_copy(&target);
            return Err(match e {
                pdf_core::Error::Cancelled => {
                    AppError::new("Extracting was stopped. No document was made.", None)
                        .with_code(crate::ipc::ErrorCode::Cancelled)
                }
                e => e.into(),
            });
        }
    };
    crate::applog::info(format!(
        "extracted {} pages into a new document in {:.0} ms",
        report.pages,
        started.elapsed().as_secs_f64() * 1000.0
    ));
    let opened = state.documents.open_unsaved(&target, folder);
    Ok(MergeOutcome {
        opened: windows::adopt(&app, window.label(), opened),
        report: report.into(),
        name: file_name(&target),
    })
}

/// Stops the merge or extraction in progress, if any.
#[tauri::command]
pub fn cancel_merge(state: State<'_, AppState>) {
    if let Ok(current) = state.merge_cancel.lock()
        && let Some(cancel) = current.as_ref()
    {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// Records that a bookmark was expanded or collapsed in the panel. Not an edit: the state
/// is written into the outline at the next save.
#[tauri::command(async)]
pub fn set_bookmark_open(
    state: State<'_, AppState>,
    id: u32,
    bookmark: u32,
    open: bool,
) -> Result<(), AppError> {
    Ok(state
        .documents
        .session(id)?
        .set_bookmark_open(bookmark, open)?)
}

/// Lets a document that was opened for reading only (a file that claims PDF/A, section
/// 5.4) be changed, for as long as it stays open.
#[tauri::command(async)]
pub fn enable_editing(state: State<'_, AppState>, id: u32) -> Result<(), AppError> {
    state.documents.session(id)?.enable_editing()?;
    crate::applog::info(format!("document {id}: editing enabled"));
    Ok(())
}

#[tauri::command(async)]
pub fn undo(state: State<'_, AppState>, id: u32) -> Result<DocumentChange, AppError> {
    Ok(state.documents.session(id)?.undo()?.into())
}

#[tauri::command(async)]
pub fn redo(state: State<'_, AppState>, id: u32) -> Result<DocumentChange, AppError> {
    Ok(state.documents.session(id)?.redo()?.into())
}

/// Saves the document to its own file (incrementally where possible). Saved from a
/// window other than the one that shows it (the Combine view's "Save and combine"), that
/// window is told, so its tab is no longer marked unsaved.
#[tauri::command(async)]
pub fn save(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: u32,
) -> Result<SaveResult, AppError> {
    let result = state.documents.save(
        id,
        SaveKind::Incremental,
        None,
        state.platform,
        &state.store,
    )?;
    if let Some(owner) = state.windows.owner(id)
        && owner != window.label()
    {
        let saved = DocumentSaved {
            id,
            result: result.clone(),
        };
        if let Err(e) = app.emit_to(owner.as_str(), crate::DOCUMENT_SAVED, saved) {
            crate::applog::warn(format!("could not tell window {owner} of a save: {e}"));
        }
    }
    Ok(result)
}

/// Shows the Save As dialog and saves there. `optimized` rewrites the file with garbage
/// collection and compression ("Save As (optimized)"). Returns null if the user cancels.
/// This is also how a document not saved under a name yet is saved: the dialog starts
/// with its name, in the folder of the document its pages came from.
#[tauri::command(async)]
pub fn save_as(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u32,
    optimized: bool,
) -> Result<Option<SaveResult>, AppError> {
    let current = state.documents.path(id)?;
    let (name, folder) = state.documents.save_suggestion(id)?;
    let Some(target) = pick_save_target(&app, None, name, folder.as_deref())? else {
        return Ok(None);
    };
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

/// Remembers where the user is in a document and how its pages are shown, for the next
/// time the file is opened.
#[tauri::command]
pub fn remember_view(state: State<'_, AppState>, id: u32, view: ViewState) -> Result<(), AppError> {
    // A document not saved under a name yet has no file to remember a view for.
    if state.documents.is_unsaved(id) {
        return Ok(());
    }
    let view = view.normalized();
    let path = state.documents.path(id)?;
    let key = state.platform.file_key(&path);
    state
        .store
        .lock()
        .map_err(|_| AppError::bad_state())?
        .set_view(key, view);
    Ok(())
}

/// Called by the frontend on first mount: startup time and how to start up.
#[tauri::command]
pub fn app_ready(app: AppHandle, window: WebviewWindow, state: State<'_, AppState>) -> StartupInfo {
    let elapsed = MAIN_START.get().map(Instant::elapsed).unwrap_or_default();
    let (panes, updated) = state
        .store
        .lock()
        .map(|mut s| (s.panes(), s.note_version(&app.package_info().version)))
        .unwrap_or_default();
    StartupInfo {
        first_window: window.label() == windows::MAIN,
        main_to_ready_ms: elapsed.as_secs_f64() * 1000.0,
        perf_mode: std::env::var_os("LECTRIX_PERF").is_some(),
        perf_scroll_only: std::env::var("LECTRIX_PERF").as_deref() == Ok("scroll"),
        perf_watch: std::env::var("LECTRIX_PERF").as_deref() == Ok("watch"),
        image_format: match std::env::var("LECTRIX_IMAGE_FORMAT").as_deref() {
            Ok("rgba") => ImageFormat::Rgba,
            _ => ImageFormat::Png,
        },
        panes,
        updated,
        debug_menu: debug_menu(
            std::env::var("LECTRIX_DEBUG").ok().as_deref(),
            cfg!(debug_assertions),
        ),
    }
}

/// Whether to show the Debug menu: LECTRIX_DEBUG (`env`) says so, else development builds.
fn debug_menu(env: Option<&str>, debug_build: bool) -> bool {
    match env {
        Some("1") => true,
        Some("0") => false,
        _ => debug_build,
    }
}

/// Remembers which side panes are open and their widths.
#[tauri::command]
pub fn set_pane_layout(state: State<'_, AppState>, panes: PaneLayout) -> Result<(), AppError> {
    let mut store = state.store.lock().map_err(|_| AppError::bad_state())?;
    store.set_panes(panes);
    Ok(())
}

/// Prints a frontend timing to stdout as `[lectrix-metric] name=value`, for scripted
/// performance measurements (section 2 targets).
#[tauri::command]
pub fn log_metric(name: String, ms: f64) {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(64)
        .collect();
    println!("[lectrix-metric] {name}={ms:.1}");
}

/// The address a link's web address is opened at, if Lectrix opens it: http or https with
/// something after it, of a sane length, and without spaces or control characters. Nothing
/// else is handed to the system, which would run a program or open a file as readily as
/// a web page.
fn web_address(uri: &str) -> Option<&str> {
    let lower = uri.get(..8).unwrap_or(uri).to_ascii_lowercase();
    let scheme = ["https://", "http://"]
        .into_iter()
        .find(|s| lower.starts_with(s))?;
    let plain = uri.chars().all(|c| !c.is_control() && !c.is_whitespace());
    (uri.len() > scheme.len() && uri.len() <= MAX_WEB_ADDRESS && plain).then_some(uri)
}

/// The longest web address opened (browsers take about this much).
const MAX_WEB_ADDRESS: usize = 2048;

/// Hands a web address to the default browser. With LECTRIX_BROWSER_LOG set (automation,
/// tests/e2e), the address is written to that file instead and nothing opens.
fn open_in_browser(url: &str) -> std::io::Result<()> {
    if let Some(log) = std::env::var_os("LECTRIX_BROWSER_LOG") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)?;
        return writeln!(file, "{url}");
    }
    crate::platform::current().open_web_page(url)
}

/// Opens the web address of a link on a page, or of a bookmark, in the default browser
/// (sections 6.1 and 6.2). The address must be one the document carries at `from`, so the
/// webview can have no other page opened, and a web address (`web_address`).
#[tauri::command(async)]
pub fn open_link(
    state: State<'_, AppState>,
    id: u32,
    from: LinkPlace,
    uri: String,
) -> Result<(), AppError> {
    use pdf_core::outline::Target;
    let session = state.documents.session(id)?;
    let carried = match from {
        LinkPlace::Page { page } => session
            .page_links(page as usize)?
            .0
            .iter()
            .any(|l| matches!(&l.target, Target::Uri(u) if *u == uri)),
        LinkPlace::Bookmark { bookmark } => {
            let mut found = false;
            session.info()?.outline.for_each(|b| {
                found |= b.id == bookmark && matches!(&b.target, Target::Uri(u) if *u == uri);
            });
            found
        }
    };
    let Some(url) = web_address(&uri).filter(|_| carried) else {
        return Err(AppError::new(
            "Lectrix doesn’t open this link.",
            Some("It opens web addresses only (http and https)."),
        ));
    };
    open_in_browser(url).map_err(|e| {
        crate::applog::warn(format!("could not open {url}: {e}"));
        AppError::new(
            "The browser couldn’t be opened.",
            Some("Copy the link and open it in your browser."),
        )
    })?;
    crate::applog::info(format!("document {id}: opened the link {url}"));
    Ok(())
}

/// About: opens the source code's page or the author's profile in the default browser.
/// The addresses are fixed in Rust, so the webview can't have any other page opened.
#[tauri::command]
pub fn open_web_page(page: WebPage) -> Result<(), AppError> {
    let url = page.url();
    crate::platform::current().open_web_page(url).map_err(|e| {
        crate::applog::warn(format!("could not open {url}: {e}"));
        let shown = url.trim_start_matches("https://");
        AppError::new(
            "The browser couldn’t be opened.",
            Some(&format!("Go to {shown} in your browser.")),
        )
    })
}

/// The window buttons the title bar draws, and where (section 8). Synchronous, so it runs
/// on the main thread, where GTK's settings are read on Linux.
#[tauri::command]
pub fn window_chrome() -> WindowChrome {
    crate::platform::current().window_chrome()
}

/// The window's menus, for the system's menu bar on macOS (section 8, `menu.rs`). Off the
/// main thread, since building menus waits for it.
#[tauri::command(async)]
pub fn set_menu(app: AppHandle, window: WebviewWindow, menus: Vec<MenuEntry>) {
    crate::menu::set(&app, window.label(), menus);
}

/// Records a frontend error in the app log (the user sees a plain message instead).
#[tauri::command]
pub fn log_error(message: String) {
    let message: String = message.chars().take(2000).collect();
    crate::applog::error(format!("frontend: {message}"));
}

#[cfg(test)]
mod tests {
    use super::debug_menu;
    use crate::ipc::WebPage;

    #[test]
    fn the_debug_menu_shows_in_development_builds_or_when_asked() {
        assert!(debug_menu(None, true));
        assert!(!debug_menu(None, false));
        assert!(debug_menu(Some("1"), false));
        assert!(!debug_menu(Some("0"), true));
        assert!(!debug_menu(Some("yes"), false));
    }

    #[test]
    fn only_web_addresses_are_opened() {
        use super::web_address;
        for ok in [
            "https://example.org/a?b=c#d",
            "http://example.org",
            "HTTPS://Example.org/",
        ] {
            assert_eq!(web_address(ok), Some(ok));
        }
        for refused in [
            "",
            "https://",
            "example.org",
            "file:///C:/Windows/System32/calc.exe",
            "C:\\Windows\\System32\\calc.exe",
            "javascript:alert(1)",
            "mailto:someone@example.org",
            "ftp://example.org/file",
            " https://example.org",
            "https://example.org/a b",
            "https://example.org/\u{7}",
            "httpsx://example.org",
        ] {
            assert_eq!(web_address(refused), None, "{refused:?}");
        }
        let long = format!("https://example.org/{}", "a".repeat(3000));
        assert_eq!(web_address(&long), None);
    }

    #[test]
    fn web_pages_match_the_frontend() {
        let config = include_str!("../../src/lib/config.ts");
        for (name, page) in [
            ("SOURCE_URL", WebPage::Source),
            ("AUTHOR_URL", WebPage::Author),
        ] {
            assert!(
                config.contains(&format!("{name} = '{}'", page.url())),
                "{name} differs between ipc.rs and src/lib/config.ts"
            );
        }
    }
}
