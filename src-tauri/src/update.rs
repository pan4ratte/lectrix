//! Updates from Lectrix's GitHub releases (ADR 0011), the only network access the app
//! makes. When the window is ready, the frontend asks whether a newer release exists, and
//! Help > Check for updates asks again whenever the user wants; the user can update, wait,
//! or skip that version. The updater plugin checks every download against Lectrix's
//! signing key before anything is installed.
//!
//! Windows cannot replace a running program, so there the verified installer waits until
//! Lectrix restarts or closes. On macOS and Linux the new version is installed at once and
//! starts with the next launch.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::AppState;
use crate::applog;
use crate::ipc::{AppError, ErrorCode, UpdateInfo, UpdateProgress, UpdateReady};

/// How often the download reports progress.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

type Stop = Box<dyn FnOnce() + Send>;

/// What the updater keeps between commands.
#[derive(Default)]
pub struct Updates {
    /// The release found by the last check.
    found: Mutex<Option<Update>>,
    /// Stops the download under way.
    stop: Mutex<Option<Stop>>,
    /// Windows: the verified installer, run when Lectrix restarts or closes.
    pending: Mutex<Option<(Update, Vec<u8>)>>,
}

/// Whether this run may look for updates at all. LECTRIX_UPDATES=0 turns checks off and
/// LECTRIX_UPDATES=1 forces them on; otherwise development builds and measurement or test
/// runs (LECTRIX_EPHEMERAL) don't check.
fn checks_allowed(env: Option<&str>, debug_build: bool, ephemeral: bool) -> bool {
    match env {
        Some("0") => false,
        Some("1") => true,
        _ => !debug_build && !ephemeral,
    }
}

fn update_error(message: &str) -> AppError {
    AppError::new(
        message,
        Some("Check your internet connection, then try again from Help > Check for updates."),
    )
}

/// Looks for a newer release.
///
/// At startup (`manual` false), only if Settings leave the check on, and never offering a
/// version the user skipped; failures (no network, GitHub unreachable) are logged and
/// reported as no update, since a background check is not worth interrupting anyone for.
/// From Help > Check for updates (`manual` true), whatever Settings say and offering a
/// skipped version too, since the user asked; failures are reported.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppState>,
    manual: bool,
) -> Result<Option<UpdateInfo>, AppError> {
    let (enabled, skipped) = {
        let store = state.store.lock().map_err(|_| AppError::bad_state())?;
        (
            store.settings().check_for_updates,
            store.skipped_update().map(str::to_owned),
        )
    };
    let allowed = checks_allowed(
        std::env::var("LECTRIX_UPDATES").ok().as_deref(),
        cfg!(debug_assertions),
        std::env::var_os("LECTRIX_EPHEMERAL").is_some(),
    );
    if !allowed && manual {
        return Err(AppError::new(
            "This copy of Lectrix doesn’t check for updates.",
            Some(
                "Development builds and test runs leave update checks out; set LECTRIX_UPDATES=1 to check anyway.",
            ),
        ));
    }
    if !allowed || !(enabled || manual) {
        return Ok(None);
    }
    let checked = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let update = match checked {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(None),
        Err(e) => {
            applog::warn(format!("could not check for updates: {e}"));
            return if manual {
                Err(update_error("Lectrix couldn’t check for updates."))
            } else {
                Ok(None)
            };
        }
    };
    applog::info(format!("Lectrix {} is available", update.version));
    if !manual && skipped.as_deref() == Some(update.version.as_str()) {
        return Ok(None);
    }
    let info = UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
    };
    *state
        .updates
        .found
        .lock()
        .map_err(|_| AppError::bad_state())? = Some(update);
    Ok(Some(info))
}

/// "Don't ask again": this version is not offered again; a later one is.
#[tauri::command]
pub fn skip_update(state: State<'_, AppState>, version: String) -> Result<(), AppError> {
    if let Ok(mut found) = state.updates.found.lock() {
        found.take();
    }
    let mut store = state.store.lock().map_err(|_| AppError::bad_state())?;
    store.set_skipped_update(Some(version));
    Ok(())
}

/// Downloads the release found by the check and verifies its signature. On macOS and Linux
/// it is then installed; on Windows it waits for `restart_to_update` or for Lectrix to
/// close.
#[tauri::command]
pub async fn download_update(
    state: State<'_, AppState>,
    on_progress: Channel<UpdateProgress>,
) -> Result<UpdateReady, AppError> {
    let update = state
        .updates
        .found
        .lock()
        .map_err(|_| AppError::bad_state())?
        .clone()
        .ok_or_else(|| AppError::new("There is no update to download.", None))?;
    applog::info(format!(
        "downloading Lectrix {} from {}",
        update.version, update.download_url
    ));

    let download = update.clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut downloaded = 0u64;
        let mut reported: Option<Instant> = None;
        download
            .download(
                |chunk, total| {
                    downloaded += chunk as u64;
                    let done = total.is_some_and(|t| downloaded >= t);
                    if done || reported.is_none_or(|r| r.elapsed() >= PROGRESS_INTERVAL) {
                        reported = Some(Instant::now());
                        let _ = on_progress.send(UpdateProgress {
                            downloaded: u32::try_from(downloaded).unwrap_or(u32::MAX),
                            total: total.map(|t| u32::try_from(t).unwrap_or(u32::MAX)),
                        });
                    }
                },
                || {},
            )
            .await
    });
    let abort = task.inner().abort_handle();
    if let Ok(mut stop) = state.updates.stop.lock() {
        *stop = Some(Box::new(move || abort.abort()));
    }
    let result = task.await;
    if let Ok(mut stop) = state.updates.stop.lock() {
        stop.take();
    }
    let bytes = match result {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => {
            applog::warn(format!("could not download the update: {e}"));
            return Err(match e {
                tauri_plugin_updater::Error::Minisign(_)
                | tauri_plugin_updater::Error::Base64(_)
                | tauri_plugin_updater::Error::SignatureUtf8(_) => AppError::new(
                    "The update didn’t pass Lectrix’s signature check, so it was not installed.",
                    Some("Download Lectrix again from its GitHub page."),
                ),
                _ => update_error("The update couldn’t be downloaded."),
            });
        }
        Err(_) => {
            applog::info("update download stopped");
            return Err(
                AppError::new("The download was stopped.", None).with_code(ErrorCode::Cancelled)
            );
        }
    };
    applog::info(format!(
        "Lectrix {} downloaded and verified",
        update.version
    ));

    if cfg!(windows) {
        *state
            .updates
            .pending
            .lock()
            .map_err(|_| AppError::bad_state())? = Some((update, bytes));
        return Ok(UpdateReady { installed: false });
    }
    let version = update.version.clone();
    tauri::async_runtime::spawn_blocking(move || update.install(&bytes))
        .await
        .map_err(|_| AppError::bad_state())?
        .map_err(|e| {
            applog::error(format!("could not install Lectrix {version}: {e}"));
            AppError::new(
                "The update couldn’t be installed.",
                Some("Download Lectrix again from its GitHub page."),
            )
        })?;
    applog::info(format!("Lectrix {version} installed"));
    Ok(UpdateReady { installed: true })
}

#[tauri::command]
pub fn cancel_update_download(state: State<'_, AppState>) {
    let stop = state.updates.stop.lock().ok().and_then(|mut s| s.take());
    if let Some(stop) = stop {
        stop();
    }
}

/// Restarts into the new version. The frontend has already asked about unsaved changes
/// (`exit_confirmed`). On Windows this starts the installer, which opens Lectrix again
/// when it is done, and the app exits.
#[tauri::command]
pub fn restart_to_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let pending = state.updates.pending.lock().ok().and_then(|mut p| p.take());
    if let Some((update, bytes)) = pending {
        applog::info(format!(
            "installing Lectrix {} and restarting",
            update.version
        ));
        // Exits the app once the installer has started.
        if let Err(e) = update.install(&bytes) {
            applog::error(format!("could not start the installer: {e}"));
            if let Ok(mut p) = state.updates.pending.lock() {
                *p = Some((update, bytes));
            }
            // Lectrix stays open, so its unsaved changes need recovery copies again.
            state.documents.recovery().reopen();
            return Err(AppError::new(
                "The installer couldn’t be started.",
                Some("Close Lectrix to install the update, or try again."),
            ));
        }
    }
    app.restart()
}

/// Windows: installs an update that was downloaded but not yet installed, as Lectrix
/// closes. The installer does not open Lectrix again afterwards.
pub fn install_on_exit(state: &AppState) {
    let pending = state.updates.pending.lock().ok().and_then(|mut p| p.take());
    if let Some((update, bytes)) = pending {
        applog::info(format!("installing Lectrix {} on exit", update.version));
        if let Err(e) = update.restart_after_install(false).install(&bytes) {
            applog::error(format!("could not start the installer: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_follow_the_environment() {
        // Release builds check, unless measuring or testing.
        assert!(checks_allowed(None, false, false));
        assert!(!checks_allowed(None, false, true));
        assert!(!checks_allowed(None, true, false));
        // LECTRIX_UPDATES overrides both ways.
        assert!(!checks_allowed(Some("0"), false, false));
        assert!(checks_allowed(Some("1"), true, true));
        assert!(checks_allowed(Some("yes"), false, false));
    }
}
