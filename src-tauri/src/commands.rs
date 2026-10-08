//! IPC commands: thin wrappers that call `pdf-core` through the document registry.
//!
//! The webview never supplies file paths (AGENTS.md section 2): files come from Rust-side
//! dialogs, drag-and-drop, the command line, or the recent-files list kept in Rust.
//! Commands that may block (dialogs, opening, saving, search) run off the main thread
//! (`async`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use pdf_core::merge::{LabelMode as CoreLabelMode, MergeOptions, PagePick};
use pdf_core::save::SaveKind;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::annotations::RepairSummary;
use crate::combine::{self, Progress};
use crate::ipc::{
    AppError, DocumentChange, DocumentInfo, ImageFormat, LabelMode, MergeOutcome, MergePlan,
    MergeProgress, MergeRequest, MergeStage, OpenResult, OperationInput, PageHits, PageText,
    PaneLayout, RecentFile, RecoveredDocument, SaveResult, SearchChunk, Settings, SettingsInput,
    StartupInfo, UnsavedSource, ViewState, WebPage,
};
use crate::platform::Platform;
use crate::store::StoredSettings;
use crate::{AppState, MAIN_START};

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
    state: State<'_, AppState>,
) -> Result<Vec<OpenResult>, AppError> {
    Ok(pick_pdfs(&app, None, true)?
        .iter()
        .map(|p| state.open(p))
        .collect())
}

/// Opens the PDFs given on the command line (file association or `lectrix a.pdf b.pdf`),
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
pub fn restore_recovered(state: State<'_, AppState>, slots: Vec<String>) -> Vec<OpenResult> {
    slots
        .iter()
        .map(|slot| state.documents.restore(slot, state.platform, &state.store))
        .collect()
}

/// Deletes recovered changes the user chose not to keep.
#[tauri::command]
pub fn discard_recovered(state: State<'_, AppState>, slots: Vec<String>) {
    state.documents.recovery().discard_pending(&slots);
}

/// The user agreed to quit (saved or chose not to save): this run's recovery copies go now,
/// not only when the event loop ends, so a quit cut short does not bring them back.
#[tauri::command]
pub fn exit_confirmed(state: State<'_, AppState>) {
    state.documents.recovery().close();
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

/// Every document Rust has open, for a page that loads after they were opened (the
/// webview reloaded after a renderer crash, or a test driver navigated it).
#[tauri::command(async)]
pub fn list_open_documents(state: State<'_, AppState>) -> Result<Vec<DocumentInfo>, AppError> {
    state
        .documents
        .ids()?
        .into_iter()
        .filter_map(|id| get_document_info(state.clone(), id).ok())
        .map(Ok)
        .collect()
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
        toolbar_position: stored.toolbar_position,
        toolbar_visibility: stored.toolbar_visibility,
        quick_tools: stored.quick_tools.clone(),
        check_for_updates: stored.check_for_updates,
        smooth_zoom: stored.smooth_zoom,
        smooth_annotation_scroll: stored.smooth_annotation_scroll,
        tooltip_delay_ms: stored.tooltip_delay_ms,
        open_comment_after_markup: stored.open_comment_after_markup,
        remember_annotation_style: stored.remember_annotation_style,
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    let store = state.store.lock().map_err(|_| AppError::bad_state())?;
    Ok(settings_of(store.settings(), state.platform))
}

/// Stores the settings and applies the appearance to the window at once.
#[tauri::command]
pub fn set_settings(
    app: AppHandle,
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
        toolbar_position: settings.toolbar_position,
        toolbar_visibility: settings.toolbar_visibility,
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
        tooltip_delay_ms: settings
            .tooltip_delay_ms
            .min(crate::store::MAX_TOOLTIP_DELAY_MS),
        open_comment_after_markup: settings.open_comment_after_markup,
        remember_annotation_style: settings.remember_annotation_style,
    };
    let mut store = state.store.lock().map_err(|_| AppError::bad_state())?;
    store.set_settings(stored.clone());
    drop(store);
    crate::apply_appearance(&app, stored.appearance);
    Ok(settings_of(&stored, state.platform))
}

// ----- combining files and inserting pages (section 6.4) -----

/// Shows the Open dialog for files to combine and opens them as sources (not tabs).
#[tauri::command(async)]
pub fn open_merge_sources(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<OpenResult>, AppError> {
    Ok(pick_pdfs(&app, Some("Add files to combine"), true)?
        .iter()
        .map(|p| state.open_source(p))
        .collect())
}

/// Shows the Open dialog for the file to insert pages from, and opens it as a source.
#[tauri::command(async)]
pub fn open_insert_source(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<OpenResult>, AppError> {
    Ok(pick_pdfs(&app, Some("Insert pages from"), false)?
        .first()
        .map(|p| state.open_source(p)))
}

/// Where dropped files go: to the Combine view (`true`, reported as
/// `merge-sources-added`) or into tabs.
#[tauri::command]
pub fn set_drop_target(state: State<'_, AppState>, combine: bool) {
    state.drop_to_combine.store(combine, Ordering::Relaxed);
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

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut current = state
            .merge_cancel
            .lock()
            .map_err(|_| AppError::bad_state())?;
        if current.is_some() {
            return Err(AppError::new(
                "Lectrix is already combining files.",
                Some("Wait for it to finish, or stop it."),
            ));
        }
        *current = Some(cancel.clone());
    }
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
    let result = combine::run(job, cancel, move |p| {
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
        // The webview may have gone away; the merge finishes regardless.
        let _ = on_progress.send(message);
    });
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
        opened: state.open(&target),
        report: report.into(),
        name: file_name(&target),
    }))
}

/// Stops the merge in progress, if any.
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
    let Some(target) = pick_save_target(
        &app,
        None,
        current
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        current.parent(),
    )?
    else {
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

/// Called by the frontend on first mount: startup time and how to start up.
#[tauri::command]
pub fn app_ready(state: State<'_, AppState>) -> StartupInfo {
    let elapsed = MAIN_START.get().map(Instant::elapsed).unwrap_or_default();
    let panes = state.store.lock().map(|s| s.panes()).unwrap_or_default();
    StartupInfo {
        main_to_ready_ms: elapsed.as_secs_f64() * 1000.0,
        perf_mode: std::env::var_os("LECTRIX_PERF").is_some(),
        perf_scroll_only: std::env::var("LECTRIX_PERF").as_deref() == Ok("scroll"),
        perf_watch: std::env::var("LECTRIX_PERF").as_deref() == Ok("watch"),
        image_format: match std::env::var("LECTRIX_IMAGE_FORMAT").as_deref() {
            Ok("rgba") => ImageFormat::Rgba,
            _ => ImageFormat::Png,
        },
        panes,
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

/// Records a frontend error in the app log (the user sees a plain message instead).
#[tauri::command]
pub fn log_error(message: String) {
    let message: String = message.chars().take(2000).collect();
    crate::applog::error(format!("frontend: {message}"));
}

#[cfg(test)]
mod tests {
    use crate::ipc::WebPage;

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
