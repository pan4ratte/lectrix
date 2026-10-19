//! Open documents: the registry of sessions, opening (with duplicate detection and
//! passwords), saving, and watching files for changes made by other programs
//! (AGENTS.md sections 6.1 and 7).
//!
//! Besides tabs, the registry holds *sources*: files opened to combine them or to insert
//! their pages (section 6.4). A source has its own session (so its thumbnails render
//! through the page protocol), is never a tab, is not matched against tabs showing the same
//! file, and is not watched.
//!
//! The registry also keeps the tabs' recovery copies (section 7, `recovery.rs`): saving,
//! reloading or closing a document deletes its copy, and a copy left by a crash can be
//! restored as a tab.
//!
//! A tab can also show a document that has *no file of its own yet*: pages extracted into
//! a new document (section 6.7). Its file is a working copy in a folder of Lectrix's own,
//! beside the recovery folder. It has unsaved changes from the start, Save asks where to
//! save it, and the working copy goes once it is saved there, or closed. It is not a recent
//! file, has no remembered view and is not watched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use pdf_core::save::SaveKind;
use pdf_core::session::{DocumentInfo as CoreInfo, OpenOptions, Session};

use crate::ipc::{AppError, DocumentInfo, FileChangedEvent, OpenResult, SaveResult, ViewState};
use crate::platform::Platform;
use crate::recovery::{Recovery, RestoreSource};
use crate::store::Store;

/// Size and modification time: enough to notice that another program rewrote a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl FileStamp {
    pub fn of(path: &Path) -> Option<FileStamp> {
        let meta = std::fs::metadata(path).ok()?;
        Some(FileStamp {
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }
}

struct OpenDoc {
    session: Session,
    path: PathBuf,
    /// The file as Lectrix last read or wrote it.
    stamp: Option<FileStamp>,
    /// The file state the user was last told about, so each change is reported once.
    reported: Option<Option<FileStamp>>,
    /// A save is in progress: the file changing now is Lectrix's own doing.
    saving: bool,
    /// The password that opened the file, kept to copy its pages from it.
    password: Option<String>,
    /// Opened as a source for combining or inserting pages, not as a tab.
    source: bool,
    /// Not saved under a name yet: `path` is a working copy in Lectrix's folder for such
    /// documents.
    unsaved: Option<Unsaved>,
}

/// What is kept for a document that has no file of its own yet.
#[derive(Debug, Clone)]
struct Unsaved {
    /// The folder its Save dialog starts in: that of the document its pages came from.
    folder: Option<PathBuf>,
}

/// The longest name a working copy gets, in characters: it is a file name under the
/// app's data folder, which is a long path already.
const MAX_UNSAVED_NAME: usize = 96;

/// An encrypted file waiting for its password.
struct Pending {
    path: PathBuf,
    source: bool,
    /// A recovery copy being restored, rather than the file itself.
    restore: Option<RestoreSource>,
}

#[derive(Default)]
pub struct Documents {
    docs: Mutex<HashMap<u32, OpenDoc>>,
    next_id: AtomicU32,
    /// Encrypted files waiting for a password, by token.
    pending: Mutex<HashMap<u32, Pending>>,
    next_token: AtomicU32,
    recovery: Recovery,
    /// Numbers the working copies of documents not saved yet.
    next_unsaved: AtomicU32,
}

fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    m.lock().map_err(|_| AppError::bad_state())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into())
}

impl Documents {
    /// A registry that keeps recovery copies with `recovery` (off without a folder).
    pub fn new(recovery: Recovery) -> Documents {
        Documents {
            recovery,
            ..Documents::default()
        }
    }

    pub fn recovery(&self) -> &Recovery {
        &self.recovery
    }

    pub fn session(&self, id: u32) -> Result<Session, AppError> {
        lock(&self.docs)?
            .get(&id)
            .map(|d| d.session.clone())
            .ok_or_else(AppError::document_closed)
    }

    pub fn path(&self, id: u32) -> Result<PathBuf, AppError> {
        lock(&self.docs)?
            .get(&id)
            .map(|d| d.path.clone())
            .ok_or_else(AppError::document_closed)
    }

    // ----- documents without a file of their own yet -----

    /// Where the working copies of documents not saved yet are kept: beside the recovery
    /// folder, in the app's local data. Without one (measurement and test runs), in a
    /// folder of this process in the system's temporary files.
    fn unsaved_dir(&self) -> PathBuf {
        match self.recovery.dir().and_then(Path::parent) {
            Some(data) => data.join("unsaved"),
            None => std::env::temp_dir().join(format!("lectrix-unsaved-{}", std::process::id())),
        }
    }

    fn is_working_copy(&self, path: &Path) -> bool {
        path.starts_with(self.unsaved_dir())
    }

    /// Whether document `id` has no file of its own yet.
    pub fn is_unsaved(&self, id: u32) -> bool {
        self.docs
            .lock()
            .is_ok_and(|docs| docs.get(&id).is_some_and(|d| d.unsaved.is_some()))
    }

    /// The name and the folder the Save dialog starts with for document `id`: its file's,
    /// or for a document not saved yet, its name and the folder of the document it came
    /// from.
    pub fn save_suggestion(&self, id: u32) -> Result<(Option<String>, Option<PathBuf>), AppError> {
        let docs = lock(&self.docs)?;
        let doc = docs.get(&id).ok_or_else(AppError::document_closed)?;
        let name = doc
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let folder = match &doc.unsaved {
            Some(unsaved) => unsaved.folder.clone(),
            None => doc.path.parent().map(Path::to_path_buf),
        };
        Ok((name, folder))
    }

    /// A path for the working copy of a new document named `name` ("Pages from a.pdf"), in
    /// a folder of its own, so that any two have different paths. Nothing is at the path
    /// yet.
    pub fn new_working_copy(&self, name: &str) -> Result<PathBuf, AppError> {
        let n = self.next_unsaved.fetch_add(1, Ordering::Relaxed) + 1;
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let folder = self
            .unsaved_dir()
            .join(format!("{stamp}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&folder).map_err(|e| {
            crate::applog::warn(format!(
                "no folder {} for a new document: {e}",
                folder.display()
            ));
            AppError::new(
                "Lectrix couldn’t make room for the new document.",
                Some("Check that the disk is not full, then try again."),
            )
        })?;
        Ok(folder.join(working_copy_name(name)))
    }

    /// Deletes a working copy and its folder: its document was saved elsewhere or closed,
    /// or none was opened from it (the extraction failed or was stopped). As far as that
    /// can be done now: a file still open stays until the next start, which sweeps the
    /// folder.
    pub fn remove_working_copy(&self, path: &Path) {
        if !self.is_working_copy(path) {
            return;
        }
        let _ = std::fs::remove_file(path);
        if let Some(folder) = path.parent() {
            let _ = std::fs::remove_dir(folder);
        }
        // The folder in the temporary files is this process's alone: it goes with its
        // last working copy (removing it fails while others are there).
        if self.recovery.dir().is_none() {
            let _ = std::fs::remove_dir(self.unsaved_dir());
        }
    }

    /// Opens the working copy at `path` (from [`Documents::new_working_copy`]) as a tab
    /// for a document that has not been saved yet. `folder` is where its Save dialog
    /// starts.
    pub fn open_unsaved(&self, path: &Path, folder: Option<PathBuf>) -> OpenResult {
        let name = file_name(path);
        match Session::open_unsaved(path) {
            Ok((session, info)) => {
                let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
                let doc = OpenDoc {
                    session,
                    path: path.to_path_buf(),
                    stamp: FileStamp::of(path),
                    reported: None,
                    saving: false,
                    password: None,
                    source: false,
                    unsaved: Some(Unsaved { folder }),
                };
                match self.docs.lock() {
                    Ok(mut docs) => {
                        docs.insert(id, doc);
                    }
                    Err(_) => {
                        self.remove_working_copy(path);
                        return OpenResult::Failed {
                            name,
                            error: AppError::bad_state(),
                        };
                    }
                }
                crate::applog::info(format!(
                    "opened new document {id}: {} pages, not saved yet",
                    info.pages.len()
                ));
                OpenResult::Opened {
                    document: Box::new(document_info(id, info, None, true)),
                }
            }
            Err(e) => {
                crate::applog::warn(format!("could not open {}: {e:?}", path.display()));
                self.remove_working_copy(path);
                OpenResult::Failed {
                    name,
                    error: e.into(),
                }
            }
        }
    }

    /// Deletes the working copies nothing needs any more: at the start, those an earlier
    /// run left (it crashed, or quit with such a document open) and that no recovery copy
    /// waiting to be restored belongs to; after recovered changes are discarded, theirs.
    pub fn sweep_working_copies(&self) {
        // (Only the app's own folder: the one in the temporary files is this process's.)
        if self.recovery.dir().is_none() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(self.unsaved_dir()) else {
            return;
        };
        let mut kept: Vec<PathBuf> = self
            .recovery
            .pending()
            .into_iter()
            .map(|p| p.path)
            .collect();
        if let Ok(docs) = self.docs.lock() {
            kept.extend(docs.values().map(|d| d.path.clone()));
        }
        for entry in entries.flatten() {
            let folder = entry.path();
            if kept.iter().any(|path| path.starts_with(&folder)) {
                continue;
            }
            let removed = if folder.is_dir() {
                std::fs::remove_dir_all(&folder)
            } else {
                std::fs::remove_file(&folder)
            };
            if let Err(e) = removed {
                crate::applog::warn(format!("could not delete {}: {e}", folder.display()));
            }
        }
    }

    /// Deletes the working copies of the documents still open: Lectrix is quitting, after
    /// the user saved them or chose not to.
    pub fn discard_unsaved(&self) {
        let paths: Vec<PathBuf> = match self.docs.lock() {
            Ok(docs) => docs
                .values()
                .filter(|d| d.unsaved.is_some())
                .map(|d| d.path.clone())
                .collect(),
            Err(_) => return,
        };
        for path in paths {
            self.remove_working_copy(&path);
        }
    }

    /// The file of document `id`, open in a tab, and the password that opened it: for
    /// Unlock (section 6.8). A document not saved under a name yet has no file.
    pub fn file_of(&self, id: u32) -> Result<(PathBuf, Option<String>), AppError> {
        let docs = lock(&self.docs)?;
        let doc = docs.get(&id).ok_or_else(AppError::document_closed)?;
        if doc.unsaved.is_some() {
            return Err(AppError::new(
                "This document has not been saved yet, so it has no password or restrictions.",
                None,
            ));
        }
        Ok((doc.path.clone(), doc.password.clone()))
    }

    /// The tab whose file `path` is, with no unsaved changes: Unlock writes over its file
    /// and reads it again ([`Documents::rewrite`]). One with unsaved changes is left to
    /// the watcher, which warns that its file changed (section 7).
    pub fn clean_tab_of(&self, path: &Path, platform: &dyn Platform) -> Option<u32> {
        let id = self.find_open(path, platform)?;
        let session = self.session(id).ok()?;
        (!session.info().ok()?.state.dirty).then_some(id)
    }

    /// Runs `write`, which writes over the file of document `id`, then reads the document
    /// again from it, with `password` from now on (`None`: the file has none). Meanwhile
    /// the watcher leaves the file alone, so the change is not reported as one made by
    /// another program.
    pub fn rewrite(
        &self,
        id: u32,
        write: impl FnOnce() -> Result<(), AppError>,
        password: Option<String>,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> Result<DocumentInfo, AppError> {
        lock(&self.docs)?
            .get_mut(&id)
            .ok_or_else(AppError::document_closed)?
            .saving = true;
        let written = write();
        if let Some(doc) = lock(&self.docs)?.get_mut(&id) {
            doc.saving = false;
        }
        written?;
        self.reload_as(id, Some(password), platform, store)
    }

    fn find_open(&self, path: &Path, platform: &dyn Platform) -> Option<u32> {
        let docs = self.docs.lock().ok()?;
        docs.iter()
            .find(|(_, d)| !d.source && platform.same_file(&d.path, path))
            .map(|(id, _)| *id)
    }

    /// Opens a file the user picked (dialog, drag-and-drop, command line, recent list).
    pub fn open(
        &self,
        path: &Path,
        password: Option<&str>,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> OpenResult {
        self.open_as(path, password, false, platform, store)
    }

    /// Opens a file to combine it or insert its pages from it (a source, not a tab).
    pub fn open_source(
        &self,
        path: &Path,
        password: Option<&str>,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> OpenResult {
        self.open_as(path, password, true, platform, store)
    }

    fn open_as(
        &self,
        path: &Path,
        password: Option<&str>,
        source: bool,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> OpenResult {
        let name = file_name(path);
        if !source && let Some(id) = self.find_open(path, platform) {
            return OpenResult::AlreadyOpen { id };
        }
        // A file that claims PDF/A opens for reading only if Settings says so (section
        // 5.4). A source is only read from.
        let options = OpenOptions {
            pdfa_read_only: !source
                && store
                    .lock()
                    .map(|s| s.settings().pdfa_read_only)
                    .unwrap_or(true),
        };
        match Session::open_with(path, password, options) {
            Ok((session, info)) => {
                let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
                let doc = OpenDoc {
                    session,
                    path: path.to_path_buf(),
                    stamp: FileStamp::of(path),
                    reported: None,
                    saving: false,
                    password: password.map(str::to_owned),
                    source,
                    unsaved: None,
                };
                match self.docs.lock() {
                    Ok(mut docs) => {
                        docs.insert(id, doc);
                    }
                    Err(_) => {
                        return OpenResult::Failed {
                            name,
                            error: AppError::bad_state(),
                        };
                    }
                }
                // Sources are not documents the user opened to read: no recent-files entry,
                // no remembered view.
                let view = if source {
                    None
                } else {
                    store.lock().ok().and_then(|mut s| {
                        s.add_recent(path, |a, b| platform.same_file(a, b));
                        s.view(&platform.file_key(path))
                    })
                };
                crate::applog::info(format!(
                    "opened {} {id}: {} pages in {:.0} ms",
                    if source { "source" } else { "document" },
                    info.pages.len(),
                    info.open_time.as_secs_f64() * 1000.0
                ));
                OpenResult::Opened {
                    document: Box::new(document_info(id, info, view, false)),
                }
            }
            Err(e @ (pdf_core::Error::PasswordRequired | pdf_core::Error::WrongPassword)) => {
                self.ask_password(path, source, None, name, &e)
            }
            Err(e) => {
                crate::applog::warn(format!("could not open {}: {e:?}", path.display()));
                OpenResult::Failed {
                    name,
                    error: e.into(),
                }
            }
        }
    }

    fn ask_password(
        &self,
        path: &Path,
        source: bool,
        restore: Option<RestoreSource>,
        name: String,
        error: &pdf_core::Error,
    ) -> OpenResult {
        let token = self.new_token();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(
                token,
                Pending {
                    path: path.to_path_buf(),
                    source,
                    restore,
                },
            );
        }
        OpenResult::NeedsPassword {
            token,
            name,
            retry: matches!(error, pdf_core::Error::WrongPassword),
        }
    }

    /// Opens the recovery copy an earlier run left in `slot` as a tab for its file, with
    /// the unsaved changes it holds (section 7).
    pub fn restore(&self, slot: &str, platform: &dyn Platform, store: &Mutex<Store>) -> OpenResult {
        match self.recovery.source(slot) {
            Some(source) => self.restore_from(source, None, platform, store),
            None => OpenResult::Failed {
                name: "A recovered document".into(),
                error: AppError::new(
                    "Its recovered changes are no longer there.",
                    Some("They may have been restored or discarded already."),
                ),
            },
        }
    }

    fn restore_from(
        &self,
        source: RestoreSource,
        password: Option<&str>,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> OpenResult {
        let name = file_name(&source.path);
        if self.find_open(&source.path, platform).is_some() {
            return OpenResult::Failed {
                name,
                error: AppError::new(
                    "The file is open already, so its recovered changes were kept for later.",
                    Some("Close it, then start Lectrix again to restore them."),
                ),
            };
        }
        // A document that had not been saved under a name when Lectrix crashed is one
        // again: its file is a working copy, and Save asks where to save.
        let unsaved = self.is_working_copy(&source.path);
        match Session::restore(&source.copy, &source.path, password) {
            Ok((session, info)) => {
                let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
                let revision = info.state.revision;
                let doc = OpenDoc {
                    session,
                    path: source.path.clone(),
                    // The file as it was when the copy was written: if another program
                    // changed it since, the watcher reports that at its next check.
                    stamp: source.stamp,
                    reported: None,
                    saving: false,
                    password: password.map(str::to_owned),
                    source: false,
                    unsaved: unsaved.then_some(Unsaved { folder: None }),
                };
                match self.docs.lock() {
                    Ok(mut docs) => {
                        docs.insert(id, doc);
                    }
                    Err(_) => {
                        return OpenResult::Failed {
                            name,
                            error: AppError::bad_state(),
                        };
                    }
                }
                self.recovery.adopt(id, &source, revision);
                let view = if unsaved {
                    None
                } else {
                    store.lock().ok().and_then(|mut s| {
                        s.add_recent(&source.path, |a, b| platform.same_file(a, b));
                        s.view(&platform.file_key(&source.path))
                    })
                };
                crate::applog::info(format!(
                    "restored document {id} from recovery slot {}",
                    source.slot
                ));
                OpenResult::Opened {
                    document: Box::new(document_info(id, info, view, unsaved)),
                }
            }
            Err(e @ (pdf_core::Error::PasswordRequired | pdf_core::Error::WrongPassword)) => {
                let path = source.path.clone();
                self.ask_password(&path, false, Some(source), name, &e)
            }
            Err(e) => {
                crate::applog::warn(format!(
                    "could not restore recovery slot {}: {e:?}",
                    source.slot
                ));
                OpenResult::Failed {
                    name,
                    error: e.into(),
                }
            }
        }
    }

    /// Writes recovery copies of the tabs with unsaved changes, and deletes those of tabs
    /// that no longer have any.
    pub fn write_recovery_copies(&self) {
        if !self.recovery.enabled() {
            return;
        }
        let tabs: Vec<(u32, Session, PathBuf)> = match self.docs.lock() {
            Ok(docs) => docs
                .iter()
                .filter(|(_, d)| !d.source && !d.saving)
                .map(|(id, d)| (*id, d.session.clone(), d.path.clone()))
                .collect(),
            Err(_) => return,
        };
        for (id, session, path) in tabs {
            self.recovery.write(id, &session, &path);
        }
    }

    /// A token for a file waiting for its password. Unlock's files (`unprotect.rs`) take
    /// theirs from here too, so the frontend's prompts never share one.
    pub fn new_token(&self) -> u32 {
        self.next_token.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Retries an encrypted file with a password.
    pub fn unlock(
        &self,
        token: u32,
        password: &str,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> Result<OpenResult, AppError> {
        let pending = lock(&self.pending)?.remove(&token).ok_or_else(|| {
            AppError::new(
                "That file is no longer waiting to open.",
                Some("Open it again."),
            )
        })?;
        Ok(match pending.restore {
            Some(source) => self.restore_from(source, Some(password), platform, store),
            None => self.open_as(
                &pending.path,
                Some(password),
                pending.source,
                platform,
                store,
            ),
        })
    }

    pub fn cancel_unlock(&self, token: u32) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&token);
        }
    }

    /// Ids of the documents open in tabs (not sources), in the order they were opened.
    pub fn ids(&self) -> Result<Vec<u32>, AppError> {
        let mut ids: Vec<u32> = lock(&self.docs)?
            .iter()
            .filter(|(_, d)| !d.source)
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        Ok(ids)
    }

    /// The file and password of an open document, to copy pages from it.
    pub fn source(&self, id: u32) -> Result<(PathBuf, Option<String>), AppError> {
        lock(&self.docs)?
            .get(&id)
            .map(|d| (d.path.clone(), d.password.clone()))
            .ok_or_else(AppError::document_closed)
    }

    /// Tabs showing the same file as `path` (not sources), with their sessions.
    pub fn tabs_showing(&self, path: &Path, platform: &dyn Platform) -> Vec<(u32, Session)> {
        let Ok(docs) = self.docs.lock() else {
            return Vec::new();
        };
        docs.iter()
            .filter(|(_, d)| !d.source && platform.same_file(&d.path, path))
            .map(|(id, d)| (*id, d.session.clone()))
            .collect()
    }

    pub fn close(&self, id: u32) -> Option<Session> {
        let doc = self.docs.lock().ok()?.remove(&id)?;
        doc.session.close();
        self.recovery.discard(id);
        if doc.unsaved.is_some() {
            self.remove_working_copy(&doc.path);
        }
        Some(doc.session)
    }

    /// Saves document `id` in place or (with `target`) to a new file. A document not saved
    /// under a name yet is saved only to a new file, which it then belongs to; its working
    /// copy goes.
    pub fn save(
        &self,
        id: u32,
        kind: SaveKind,
        target: Option<PathBuf>,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> Result<SaveResult, AppError> {
        let session = {
            let mut docs = lock(&self.docs)?;
            if let Some(target) = &target {
                let other = docs.iter().any(|(other, d)| {
                    *other != id && !d.source && platform.same_file(&d.path, target)
                });
                if other {
                    return Err(AppError::new(
                        "That file is open in another tab.",
                        Some("Close it there first, or choose a different name."),
                    ));
                }
            }
            let doc = docs.get_mut(&id).ok_or_else(AppError::document_closed)?;
            let onto_itself = match &target {
                Some(target) => platform.same_file(&doc.path, target),
                None => true,
            };
            if doc.unsaved.is_some() && onto_itself {
                return Err(AppError::new(
                    "This document has not been saved yet.",
                    Some("Use Save as, and choose where to save it."),
                ));
            }
            doc.saving = true;
            doc.session.clone()
        };
        let result = session.save(kind, target);
        let mut docs = lock(&self.docs)?;
        let doc = docs.get_mut(&id).ok_or_else(AppError::document_closed)?;
        doc.saving = false;
        let saved = result?;
        // Its protection changed: it opens with another password now.
        if let Some(password) = &saved.password {
            doc.password = password.clone();
        }
        let save_as = !platform.same_file(&doc.path, &saved.path);
        let working_copy = doc
            .unsaved
            .take()
            .map(|_| std::mem::replace(&mut doc.path, saved.path.clone()));
        doc.path = saved.path.clone();
        doc.stamp = FileStamp::of(&doc.path);
        doc.reported = None;
        drop(docs);
        self.recovery.discard(id);
        if let Some(working_copy) = working_copy {
            self.remove_working_copy(&working_copy);
        }
        if save_as && let Ok(mut store) = store.lock() {
            store.add_recent(&saved.path, |a, b| platform.same_file(a, b));
        }
        if saved.outcome.fell_back_to_full {
            crate::applog::info(format!(
                "document {id}: the file had been repaired, so it was saved in full"
            ));
        }
        if let Some(level) = saved.pdfa_removed {
            crate::applog::info(format!(
                "document {id}: saved with changes that may not comply with {level}, so its claim was removed"
            ));
        }
        Ok(SaveResult {
            state: saved.state.into(),
            name: file_name(&saved.path),
            path: saved.path.display().to_string(),
            fell_back_to_full: saved.outcome.fell_back_to_full,
            outline: saved.outline.map(Into::into),
            annotations: crate::annotations::changed(saved.annotations),
            pdfa_removed: saved.pdfa_removed.map(Into::into),
            flags: saved.flags.into(),
        })
    }

    /// Reads the file again after another program changed it.
    pub fn reload(
        &self,
        id: u32,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> Result<DocumentInfo, AppError> {
        self.reload_as(id, None, platform, store)
    }

    /// [`Documents::reload`], with the password to open the file with from now on, when
    /// its protection changed.
    fn reload_as(
        &self,
        id: u32,
        password: Option<Option<String>>,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> Result<DocumentInfo, AppError> {
        if self.is_unsaved(id) {
            return Err(AppError::new(
                "This document has not been saved yet, so there is no file to read again.",
                Some("Save it first."),
            ));
        }
        let session = self.session(id)?;
        let info = match &password {
            Some(password) => session.reload_with(password.clone())?,
            None => session.reload()?,
        };
        self.recovery.discard(id);
        let path = {
            let mut docs = lock(&self.docs)?;
            let doc = docs.get_mut(&id).ok_or_else(AppError::document_closed)?;
            if let Some(password) = password {
                doc.password = password;
            }
            doc.stamp = FileStamp::of(&doc.path);
            doc.reported = None;
            doc.path.clone()
        };
        let view = store
            .lock()
            .ok()
            .and_then(|s| s.view(&platform.file_key(&path)));
        Ok(document_info(id, info, view, false))
    }

    /// Reports a change of document `id`'s file again at the next check: its tab moved to
    /// another window, which has not been told.
    pub fn report_again(&self, id: u32) {
        if let Ok(mut docs) = self.docs.lock()
            && let Some(doc) = docs.get_mut(&id)
        {
            doc.reported = None;
        }
    }

    /// Checks every open file once and returns the changes not yet reported.
    pub fn poll_changes(&self) -> Vec<FileChangedEvent> {
        let snapshot: Vec<(u32, PathBuf)> = match self.docs.lock() {
            Ok(docs) => docs
                .iter()
                .filter(|(_, d)| !d.saving && !d.source && d.unsaved.is_none())
                .map(|(id, d)| (*id, d.path.clone()))
                .collect(),
            Err(_) => return Vec::new(),
        };
        // Stat outside the lock: a slow network drive must not stall rendering.
        let stamps: Vec<(u32, Option<FileStamp>)> = snapshot
            .into_iter()
            .map(|(id, path)| (id, FileStamp::of(&path)))
            .collect();
        let mut events = Vec::new();
        let Ok(mut docs) = self.docs.lock() else {
            return events;
        };
        for (id, now) in stamps {
            let Some(doc) = docs.get_mut(&id) else {
                continue;
            };
            if doc.saving || now == doc.stamp || doc.reported == Some(now) {
                continue;
            }
            doc.reported = Some(now);
            events.push(FileChangedEvent {
                id,
                exists: now.is_some(),
            });
        }
        events
    }
}

/// How often open files are checked for outside changes.
pub const WATCH_INTERVAL: Duration = Duration::from_millis(1500);

/// A working copy's file name for a document named `name`: cut to a length that fits
/// under the app's data folder, with its `.pdf` kept.
fn working_copy_name(name: &str) -> String {
    let stem = name.strip_suffix(".pdf").unwrap_or(name);
    let stem: String = stem.chars().take(MAX_UNSAVED_NAME).collect();
    format!("{}.pdf", stem.trim_end())
}

/// What the frontend is told about document `id`. `untitled`: it has no file of its own
/// yet.
pub fn document_info(
    id: u32,
    info: CoreInfo,
    view: Option<ViewState>,
    untitled: bool,
) -> DocumentInfo {
    let (labels, label_rules) = crate::ipc::split_labels(info.labels);
    DocumentInfo {
        id,
        name: file_name(&info.path),
        path: info.path.display().to_string(),
        untitled,
        pages: info.pages.into_iter().map(Into::into).collect(),
        labels,
        label_rules,
        flags: info.flags.into(),
        read_only: info.read_only,
        page_layout: info.page_layout.map(Into::into),
        state: info.state.into(),
        outline: info.outline.into(),
        annotations: crate::annotations::pages(info.annotations),
        view,
        open_ms: info.open_time.as_secs_f64() * 1000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_core::ops::Operation;
    use pdf_core::testgen::{SampleSpec, sample_document};

    fn sample(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let doc = sample_document(&SampleSpec::default()).unwrap();
        pdf_core::save::save_atomic(&doc, SaveKind::Full, None, &path).unwrap();
        path
    }

    fn setup(name: &str) -> (PathBuf, Documents, Mutex<Store>) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/test-output/documents")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (dir, Documents::default(), Mutex::new(Store::load(None)))
    }

    #[test]
    fn opening_the_same_file_twice_switches_to_its_tab() {
        let (dir, docs, store) = setup("twice");
        let path = sample(&dir, "a.pdf");
        let platform = crate::platform::current();
        let OpenResult::Opened { document } = docs.open(&path, None, platform, &store) else {
            panic!("expected Opened");
        };
        let again = dir.join(".").join("A.PDF");
        // The same file under another spelling where names ignore case (Windows, and macOS
        // by default); elsewhere (Linux) A.PDF is another file, which doesn't exist.
        if again.exists() {
            match docs.open(&again, None, platform, &store) {
                OpenResult::AlreadyOpen { id } => assert_eq!(id, document.id),
                other => panic!("expected AlreadyOpen, got {other:?}"),
            }
        }
        let same = dir.join(".").join("a.pdf");
        match docs.open(&same, None, platform, &store) {
            OpenResult::AlreadyOpen { id } => assert_eq!(id, document.id),
            other => panic!("expected AlreadyOpen, got {other:?}"),
        }
        assert_eq!(store.lock().unwrap().recent().len(), 1);
    }

    /// A file that claims PDF/A opens for reading only while Settings says so: Rust then
    /// refuses to change it until editing is enabled.
    #[test]
    fn pdfa_files_open_for_reading_only_unless_settings_say_otherwise() {
        let (dir, docs, store) = setup("pdfa");
        let platform = crate::platform::current();
        let pdfa = |name: &str| {
            let path = dir.join(name);
            let doc = sample_document(&SampleSpec {
                pdfa: Some(pdf_core::docinfo::PdfA {
                    part: 2,
                    conformance: Some('B'),
                }),
                ..SampleSpec::default()
            })
            .unwrap();
            pdf_core::save::save_atomic(&doc, SaveKind::Full, None, &path).unwrap();
            path
        };
        let rotate = || Operation::RotatePages {
            pages: vec![0],
            degrees: 90,
        };

        let OpenResult::Opened { document } =
            docs.open(&pdfa("reading.pdf"), None, platform, &store)
        else {
            panic!("expected Opened");
        };
        assert!(document.read_only);
        assert_eq!(
            document.flags.pdfa,
            Some(crate::ipc::PdfaLevel {
                part: 2,
                conformance: Some('B')
            })
        );
        let session = docs.session(document.id).unwrap();
        let refused: AppError = session.apply(rotate()).unwrap_err().into();
        assert!(refused.message.contains("reading only"), "{refused:?}");
        session.enable_editing().unwrap();
        session.apply(rotate()).unwrap();

        // A file without the claim is not affected.
        let OpenResult::Opened { document } =
            docs.open(&sample(&dir, "plain.pdf"), None, platform, &store)
        else {
            panic!("expected Opened");
        };
        assert!(!document.read_only);

        // With the setting off, such files open ready to edit; as a source, always.
        let mut settings = store.lock().unwrap().settings().clone();
        settings.pdfa_read_only = false;
        store.lock().unwrap().set_settings(settings);
        let OpenResult::Opened { document } =
            docs.open(&pdfa("editing.pdf"), None, platform, &store)
        else {
            panic!("expected Opened");
        };
        assert!(!document.read_only);
        docs.session(document.id).unwrap().apply(rotate()).unwrap();
    }

    #[test]
    fn missing_or_broken_files_fail_with_a_plain_message() {
        let (dir, docs, store) = setup("broken");
        std::fs::write(dir.join("junk.pdf"), b"hello").unwrap();
        let platform = crate::platform::current();
        for name in ["junk.pdf", "missing.pdf"] {
            match docs.open(&dir.join(name), None, platform, &store) {
                OpenResult::Failed { name: n, error } => {
                    assert_eq!(n, name);
                    assert!(!error.message.is_empty());
                }
                other => panic!("expected Failed, got {other:?}"),
            }
        }
    }

    #[test]
    fn watcher_reports_outside_changes_once_but_not_own_saves() {
        let (dir, docs, store) = setup("watch");
        let path = sample(&dir, "w.pdf");
        let platform = crate::platform::current();
        let OpenResult::Opened { document } = docs.open(&path, None, platform, &store) else {
            panic!("expected Opened");
        };
        assert!(docs.poll_changes().is_empty());

        // Lectrix's own save is not an outside change.
        docs.session(document.id)
            .unwrap()
            .apply(Operation::RotatePages {
                pages: vec![0],
                degrees: 90,
            })
            .unwrap();
        docs.save(document.id, SaveKind::Incremental, None, platform, &store)
            .unwrap();
        assert!(docs.poll_changes().is_empty());

        // Another program rewrites the file (by rename, as most programs save).
        let other = sample(&dir, "other.pdf");
        std::fs::rename(&other, &path).unwrap();
        let events = docs.poll_changes();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, document.id);
        assert!(events[0].exists);
        assert!(docs.poll_changes().is_empty(), "reported once");

        // Reloading accepts the new file.
        docs.reload(document.id, platform, &store).unwrap();
        assert!(docs.poll_changes().is_empty());

        std::fs::remove_file(&path).unwrap();
        let events = docs.poll_changes();
        assert_eq!(events.len(), 1);
        assert!(!events[0].exists);
        // Its tab moved to another window, which is told too.
        assert!(docs.poll_changes().is_empty());
        docs.report_again(document.id);
        assert_eq!(docs.poll_changes().len(), 1);
    }

    #[test]
    fn sources_are_separate_from_tabs_and_keep_their_password() {
        let (dir, docs, store) = setup("sources");
        let path = sample(&dir, "s.pdf");
        let platform = crate::platform::current();
        let OpenResult::Opened { document: tab } = docs.open(&path, None, platform, &store) else {
            panic!("expected Opened");
        };
        // The same file opens again as a source, with its own session.
        let OpenResult::Opened { document: source } =
            docs.open_source(&path, Some("pw"), platform, &store)
        else {
            panic!("expected Opened");
        };
        assert_ne!(tab.id, source.id);
        assert_eq!(docs.ids().unwrap(), vec![tab.id]);
        assert_eq!(
            docs.source(source.id).unwrap(),
            (path.clone(), Some("pw".into()))
        );
        assert_eq!(docs.tabs_showing(&path, platform).len(), 1);
        // Sources are not recent files and are not watched.
        assert_eq!(store.lock().unwrap().recent().len(), 1);
        std::fs::remove_file(&path).ok();
        let events = docs.poll_changes();
        assert!(events.iter().all(|e| e.id != source.id));
    }

    #[test]
    fn an_encrypted_source_asks_for_its_password_and_stays_a_source() {
        use mupdf::pdf::{Encryption, PdfWriteOptions};
        let (dir, docs, store) = setup("encrypted-source");
        let path = dir.join("locked.pdf");
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let mut options = PdfWriteOptions::default();
        options
            .set_encryption(Encryption::Aes256)
            .set_user_password("open sesame")
            .set_owner_password("owner");
        doc.save_with_options(path.to_str().unwrap(), options)
            .unwrap();
        let platform = crate::platform::current();
        let OpenResult::NeedsPassword { token, .. } =
            docs.open_source(&path, None, platform, &store)
        else {
            panic!("expected NeedsPassword");
        };
        let OpenResult::Opened { document } =
            docs.unlock(token, "open sesame", platform, &store).unwrap()
        else {
            panic!("expected Opened");
        };
        // Unlocked, it is still a source: not a tab, not a recent file, password kept.
        assert!(docs.ids().unwrap().is_empty());
        assert!(store.lock().unwrap().recent().is_empty());
        assert_eq!(
            docs.source(document.id).unwrap(),
            (path.clone(), Some("open sesame".into()))
        );
    }

    #[test]
    fn save_as_onto_another_open_document_is_refused() {
        let (dir, docs, store) = setup("save-as-clash");
        let a = sample(&dir, "a.pdf");
        let b = sample(&dir, "b.pdf");
        let platform = crate::platform::current();
        let OpenResult::Opened { document } = docs.open(&a, None, platform, &store) else {
            panic!();
        };
        let OpenResult::Opened { .. } = docs.open(&b, None, platform, &store) else {
            panic!();
        };
        assert!(
            docs.save(
                document.id,
                SaveKind::Full,
                Some(b.clone()),
                platform,
                &store
            )
            .is_err()
        );
        let saved = docs
            .save(
                document.id,
                SaveKind::Full,
                Some(dir.join("c.pdf")),
                platform,
                &store,
            )
            .unwrap();
        assert_eq!(saved.name, "c.pdf");
        assert_eq!(docs.path(document.id).unwrap(), dir.join("c.pdf"));
    }

    #[test]
    fn a_restored_tab_notices_if_its_file_changed_since_the_copy() {
        let (dir, _, store) = setup("restore-watch");
        let path = sample(&dir, "r.pdf");
        let platform = crate::platform::current();
        let recovery_dir = dir.join("recovery");
        {
            let docs = Documents::new(Recovery::new(Some(recovery_dir.clone())));
            let OpenResult::Opened { document } = docs.open(&path, None, platform, &store) else {
                panic!("expected Opened");
            };
            docs.session(document.id)
                .unwrap()
                .apply(Operation::RotatePages {
                    pages: vec![0],
                    degrees: 90,
                })
                .unwrap();
            docs.write_recovery_copies();
            // A crash: the registry goes without closing anything.
        }

        let docs = Documents::new(Recovery::new(Some(recovery_dir)));
        let pending = docs.recovery().pending();
        assert_eq!(pending.len(), 1);
        let OpenResult::Opened { document } = docs.restore(&pending[0].slot, platform, &store)
        else {
            panic!("expected Opened");
        };
        assert!(document.state.dirty);
        assert_eq!(document.path, path.display().to_string());
        assert!(docs.poll_changes().is_empty(), "the file is as it was");
        // The same file again: it switches to the restored tab.
        assert!(matches!(
            docs.open(&path, None, platform, &store),
            OpenResult::AlreadyOpen { id } if id == document.id
        ));

        // Another program rewrites the file: the restored tab hears about it.
        let other = sample(&dir, "other.pdf");
        std::fs::rename(&other, &path).unwrap();
        let events = docs.poll_changes();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, document.id);

        // Restoring the same slot twice is refused, and closing deletes the copy.
        assert!(matches!(
            docs.restore(&pending[0].slot, platform, &store),
            OpenResult::Failed { .. }
        ));
        docs.close(document.id);
        assert!(docs.recovery().pending().is_empty());
    }

    /// Makes a document that has no file of its own yet from the first two pages of `from`,
    /// as `extract_pages` does.
    fn extract_two(docs: &Documents, from: u32) -> Box<DocumentInfo> {
        let (_, folder) = docs.save_suggestion(from).unwrap();
        let target = docs.new_working_copy("Pages from a.pdf").unwrap();
        docs.session(from)
            .unwrap()
            .extract(vec![1, 0], &target, |_| true)
            .unwrap();
        match docs.open_unsaved(&target, folder) {
            OpenResult::Opened { document } => document,
            other => panic!("expected Opened, got {other:?}"),
        }
    }

    #[test]
    fn extracted_pages_are_a_document_without_a_file_until_it_is_saved() {
        let (dir, _, store) = setup("unsaved");
        let docs = Documents::new(Recovery::new(Some(dir.join("recovery"))));
        let platform = crate::platform::current();
        let OpenResult::Opened { document: source } =
            docs.open(&sample(&dir, "a.pdf"), None, platform, &store)
        else {
            panic!("expected Opened");
        };

        let made = extract_two(&docs, source.id);
        assert!(made.untitled && made.state.dirty);
        assert_eq!(made.name, "Pages from a.pdf");
        assert_eq!(made.pages.len(), 2);
        let working = PathBuf::from(&made.path);
        assert!(working.starts_with(dir.join("unsaved")) && working.is_file());
        // A tab, but not a recent file, and its working copy is not watched.
        assert_eq!(docs.ids().unwrap(), vec![source.id, made.id]);
        assert_eq!(store.lock().unwrap().recent().len(), 1);
        assert!(docs.poll_changes().is_empty());
        // Its Save dialog starts with its name, in the folder its pages came from.
        assert_eq!(
            docs.save_suggestion(made.id).unwrap(),
            (Some("Pages from a.pdf".into()), Some(dir.clone()))
        );

        // It has no file to save onto, nor one to read again.
        let refused = docs
            .save(made.id, SaveKind::Incremental, None, platform, &store)
            .unwrap_err();
        assert!(
            refused.message.contains("not been saved yet"),
            "{refused:?}"
        );
        assert!(docs.reload(made.id, platform, &store).is_err());

        let target = dir.join("pages.pdf");
        let saved = docs
            .save(
                made.id,
                SaveKind::Full,
                Some(target.clone()),
                platform,
                &store,
            )
            .unwrap();
        assert_eq!(saved.name, "pages.pdf");
        assert!(!saved.state.dirty);
        // Now it is a file like any other: a recent one, saved in place, and its working
        // copy is gone with its folder.
        assert!(!docs.is_unsaved(made.id));
        assert_eq!(docs.path(made.id).unwrap(), target);
        assert_eq!(store.lock().unwrap().recent().len(), 2);
        assert!(!working.exists(), "the working copy stayed");
        assert!(!working.parent().unwrap().exists());
        docs.session(made.id)
            .unwrap()
            .apply(Operation::RotatePages {
                pages: vec![0],
                degrees: 90,
            })
            .unwrap();
        docs.save(made.id, SaveKind::Incremental, None, platform, &store)
            .unwrap();

        // Closed without saving, a new document leaves nothing behind.
        let dropped = extract_two(&docs, source.id);
        let working = PathBuf::from(&dropped.path);
        assert!(working.is_file());
        docs.close(dropped.id);
        docs.sweep_working_copies();
        assert!(!working.exists(), "the working copy stayed");
    }

    #[test]
    fn a_document_without_a_file_comes_back_after_a_crash_as_one() {
        let (dir, _, store) = setup("unsaved-crash");
        let platform = crate::platform::current();
        let recovery_dir = dir.join("recovery");
        let working;
        {
            let docs = Documents::new(Recovery::new(Some(recovery_dir.clone())));
            let OpenResult::Opened { document: source } =
                docs.open(&sample(&dir, "a.pdf"), None, platform, &store)
            else {
                panic!("expected Opened");
            };
            working = PathBuf::from(&extract_two(&docs, source.id).path);
            docs.write_recovery_copies();
            // A crash: the registry goes without closing anything.
        }

        let docs = Documents::new(Recovery::new(Some(recovery_dir.clone())));
        // The start of the next run keeps what a recovery copy belongs to.
        docs.sweep_working_copies();
        assert!(working.is_file());
        let pending = docs.recovery().pending();
        assert_eq!(pending.len(), 1);
        let OpenResult::Opened { document } = docs.restore(&pending[0].slot, platform, &store)
        else {
            panic!("expected Opened");
        };
        assert!(document.untitled && document.state.dirty);
        assert_eq!(document.name, "Pages from a.pdf");
        assert_eq!(document.pages.len(), 2);
        assert!(
            docs.save(document.id, SaveKind::Incremental, None, platform, &store)
                .is_err()
        );
        let target = dir.join("kept.pdf");
        docs.save(
            document.id,
            SaveKind::Full,
            Some(target.clone()),
            platform,
            &store,
        )
        .unwrap();
        assert!(target.is_file() && !working.exists());
        docs.close(document.id);

        // Discarded instead of restored, its working copy goes at the sweep that follows.
        let docs = Documents::new(Recovery::new(Some(recovery_dir.clone())));
        let OpenResult::Opened { document: source } =
            docs.open(&dir.join("a.pdf"), None, platform, &store)
        else {
            panic!("expected Opened");
        };
        let working = PathBuf::from(&extract_two(&docs, source.id).path);
        docs.write_recovery_copies();
        drop(docs);
        let docs = Documents::new(Recovery::new(Some(recovery_dir)));
        let slots: Vec<String> = docs
            .recovery()
            .pending()
            .into_iter()
            .map(|p| p.slot)
            .collect();
        assert_eq!(slots.len(), 1);
        docs.recovery().discard_pending(&slots);
        docs.sweep_working_copies();
        assert!(!working.exists(), "the working copy stayed");
    }

    #[test]
    fn working_copies_get_names_that_fit() {
        assert_eq!(working_copy_name("Pages from a.pdf"), "Pages from a.pdf");
        assert_eq!(working_copy_name("no extension"), "no extension.pdf");
        let long = format!("Pages from {}.pdf", "x".repeat(300));
        let name = working_copy_name(&long);
        assert_eq!(name.chars().count(), MAX_UNSAVED_NAME + 4);
        assert!(name.ends_with(".pdf"));
    }
}
