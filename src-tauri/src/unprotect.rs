//! The start screen's Unlock (ADR 0025): a file picked in a dialog has its password and
//! restrictions removed into a new file the user names in the Save dialog. Whatever
//! password opens the file is enough. Between the steps, the file and its password wait
//! here under a token, so the webview never names a path.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::ipc::{AppError, UnprotectStep};

/// A picked file, and the password that opened it once one has.
struct Picked {
    path: PathBuf,
    password: Option<String>,
}

#[derive(Default)]
pub struct Unprotect {
    pending: Mutex<HashMap<u32, Picked>>,
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into())
}

impl Unprotect {
    /// Opens `path` with `password` and says what comes next, keeping the file under
    /// `token` unless there is nothing more to do.
    pub fn inspect(&self, token: u32, path: PathBuf, password: Option<String>) -> UnprotectStep {
        let name = file_name(&path);
        match pdf_core::unprotect::inspect(&path, password.as_deref()) {
            Ok(protection) if !protection.encrypted => UnprotectStep::NotProtected { name },
            Ok(protection) => {
                self.keep(token, Picked { path, password });
                UnprotectStep::Ready {
                    token,
                    name,
                    signed: protection.signed,
                }
            }
            Err(e @ (pdf_core::Error::PasswordRequired | pdf_core::Error::WrongPassword)) => {
                self.keep(
                    token,
                    Picked {
                        path,
                        password: None,
                    },
                );
                UnprotectStep::NeedsPassword {
                    token,
                    name,
                    retry: matches!(e, pdf_core::Error::WrongPassword),
                }
            }
            Err(e) => {
                crate::applog::warn(format!("could not unlock {}: {e:?}", path.display()));
                UnprotectStep::Failed {
                    name,
                    error: e.into(),
                }
            }
        }
    }

    /// Tries the password the user typed for the file waiting under `token`.
    pub fn with_password(&self, token: u32, password: String) -> Result<UnprotectStep, AppError> {
        let picked = self.take(token)?;
        Ok(self.inspect(token, picked.path, Some(password)))
    }

    /// The file waiting under `token`, and the password that opened it. It stays waiting
    /// until `cancel`.
    pub fn ready(&self, token: u32) -> Result<(PathBuf, Option<String>), AppError> {
        self.pending
            .lock()
            .map_err(|_| AppError::bad_state())?
            .get(&token)
            .map(|p| (p.path.clone(), p.password.clone()))
            .ok_or_else(gone)
    }

    pub fn cancel(&self, token: u32) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&token);
        }
    }

    fn keep(&self, token: u32, picked: Picked) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(token, picked);
        }
    }

    fn take(&self, token: u32) -> Result<Picked, AppError> {
        self.pending
            .lock()
            .map_err(|_| AppError::bad_state())?
            .remove(&token)
            .ok_or_else(gone)
    }
}

fn gone() -> AppError {
    AppError::new(
        "That file is no longer waiting to be unlocked.",
        Some("Choose File > Unlock… again."),
    )
}
