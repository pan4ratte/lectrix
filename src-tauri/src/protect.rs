//! Protect as a tool on a file (section 6.9, ADR 0026): a file picked in a dialog is
//! written with a password to open it, restrictions, or both, in place or as a copy the
//! Save dialog names. A protected file needs a password that opens it first; any will do,
//! as for Unlock (ADR 0025). Between the steps, the file and that password wait here under
//! a token, so the webview never names a path.

use std::path::PathBuf;

use crate::ipc::{AppError, ProtectStep};
use crate::unprotect::{Picked, Waiting, file_name};

pub struct Protect {
    files: Waiting,
}

impl Default for Protect {
    fn default() -> Self {
        Protect {
            files: Waiting::new(gone),
        }
    }
}

impl Protect {
    /// Opens `path` with `password` and says what comes next, keeping the file under
    /// `token` unless there is nothing more to do.
    pub fn inspect(&self, token: u32, path: PathBuf, password: Option<String>) -> ProtectStep {
        let name = file_name(&path);
        match pdf_core::protection::inspect(&path, password.as_deref()) {
            Ok(file) => {
                self.files.keep(token, Picked { path, password });
                ProtectStep::Ready {
                    token,
                    name,
                    protection: file.protection.map(Into::into),
                    signed: file.signed,
                    pdfa: file.pdfa.map(Into::into),
                }
            }
            Err(e @ (pdf_core::Error::PasswordRequired | pdf_core::Error::WrongPassword)) => {
                self.files.keep(
                    token,
                    Picked {
                        path,
                        password: None,
                    },
                );
                ProtectStep::NeedsPassword {
                    token,
                    name,
                    retry: matches!(e, pdf_core::Error::WrongPassword),
                }
            }
            Err(e) => {
                crate::applog::warn(format!("could not protect {}: {e:?}", path.display()));
                ProtectStep::Failed {
                    name,
                    error: e.into(),
                }
            }
        }
    }

    /// Tries the password the user typed for the file waiting under `token`.
    pub fn with_password(&self, token: u32, password: String) -> Result<ProtectStep, AppError> {
        let picked = self.files.take(token)?;
        Ok(self.inspect(token, picked.path, Some(password)))
    }

    /// The file waiting under `token`, and the password that opened it. It stays waiting
    /// until `cancel`.
    pub fn ready(&self, token: u32) -> Result<(PathBuf, Option<String>), AppError> {
        self.files.ready(token)
    }

    pub fn cancel(&self, token: u32) {
        self.files.cancel(token);
    }
}

fn gone() -> AppError {
    AppError::new(
        "That file is no longer waiting to be protected.",
        Some("Choose File > Protect… again."),
    )
}
