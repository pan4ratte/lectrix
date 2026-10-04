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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use pdf_core::save::SaveKind;
use pdf_core::session::{DocumentInfo as CoreInfo, Session};

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
    /// The file as Folio last read or wrote it.
    stamp: Option<FileStamp>,
    /// The file state the user was last told about, so each change is reported once.
    reported: Option<Option<FileStamp>>,
    /// A save is in progress: the file changing now is Folio's own doing.
    saving: bool,
    /// The password that opened the file, kept to copy its pages from it.
    password: Option<String>,
    /// Opened as a source for combining or inserting pages, not as a tab.
    source: bool,
}

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
        match Session::open(path, password) {
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
                    document: document_info(id, info, view),
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
        let token = self.next_token.fetch_add(1, Ordering::Relaxed) + 1;
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
                    Some("Close it, then start Folio again to restore them."),
                ),
            };
        }
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
                let view = store.lock().ok().and_then(|mut s| {
                    s.add_recent(&source.path, |a, b| platform.same_file(a, b));
                    s.view(&platform.file_key(&source.path))
                });
                crate::applog::info(format!(
                    "restored document {id} from recovery slot {}",
                    source.slot
                ));
                OpenResult::Opened {
                    document: document_info(id, info, view),
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
        Some(doc.session)
    }

    /// Saves document `id` in place or (with `target`) to a new file.
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
            doc.saving = true;
            doc.session.clone()
        };
        let result = session.save(kind, target);
        let mut docs = lock(&self.docs)?;
        let doc = docs.get_mut(&id).ok_or_else(AppError::document_closed)?;
        doc.saving = false;
        let saved = result?;
        let save_as = !platform.same_file(&doc.path, &saved.path);
        doc.path = saved.path.clone();
        doc.stamp = FileStamp::of(&doc.path);
        doc.reported = None;
        drop(docs);
        self.recovery.discard(id);
        if save_as && let Ok(mut store) = store.lock() {
            store.add_recent(&saved.path, |a, b| platform.same_file(a, b));
        }
        if saved.outcome.fell_back_to_full {
            crate::applog::info(format!(
                "document {id}: the file had been repaired, so it was saved in full"
            ));
        }
        Ok(SaveResult {
            state: saved.state.into(),
            name: file_name(&saved.path),
            path: saved.path.display().to_string(),
            fell_back_to_full: saved.outcome.fell_back_to_full,
            outline: saved.outline.map(Into::into),
            annotations: crate::annotations::changed(saved.annotations),
        })
    }

    /// Reads the file again after another program changed it.
    pub fn reload(
        &self,
        id: u32,
        platform: &dyn Platform,
        store: &Mutex<Store>,
    ) -> Result<DocumentInfo, AppError> {
        let session = self.session(id)?;
        let info = session.reload()?;
        self.recovery.discard(id);
        let path = {
            let mut docs = lock(&self.docs)?;
            let doc = docs.get_mut(&id).ok_or_else(AppError::document_closed)?;
            doc.stamp = FileStamp::of(&doc.path);
            doc.reported = None;
            doc.path.clone()
        };
        let view = store
            .lock()
            .ok()
            .and_then(|s| s.view(&platform.file_key(&path)));
        Ok(document_info(id, info, view))
    }

    /// Checks every open file once and returns the changes not yet reported.
    pub fn poll_changes(&self) -> Vec<FileChangedEvent> {
        let snapshot: Vec<(u32, PathBuf)> = match self.docs.lock() {
            Ok(docs) => docs
                .iter()
                .filter(|(_, d)| !d.saving && !d.source)
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

pub fn document_info(id: u32, info: CoreInfo, view: Option<ViewState>) -> DocumentInfo {
    let (labels, label_rules) = crate::ipc::split_labels(info.labels);
    DocumentInfo {
        id,
        name: file_name(&info.path),
        path: info.path.display().to_string(),
        pages: info.pages.into_iter().map(Into::into).collect(),
        labels,
        label_rules,
        flags: info.flags.into(),
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
        match docs.open(&again, None, platform, &store) {
            OpenResult::AlreadyOpen { id } => assert_eq!(id, document.id),
            other => panic!("expected AlreadyOpen, got {other:?}"),
        }
        assert_eq!(store.lock().unwrap().recent().len(), 1);
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

        // Folio's own save is not an outside change.
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
}
