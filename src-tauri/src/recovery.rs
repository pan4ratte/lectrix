//! Crash recovery (AGENTS.md section 7): every two minutes, each document with unsaved
//! changes gets a recovery copy in the app's local data folder. Saving, closing or
//! reloading a document deletes its copy, and so does quitting normally. Copies that are
//! still there at the next launch were left by a crash, and are offered for restoring.
//!
//! Each document's copy is a *slot* in the recovery folder: `<slot>.json` says which file
//! the copy belongs to, and `<slot>-<n>.pdf` is the copy itself (a snapshot written by
//! `pdf-core`: the file plus its unsaved changes as an incremental update). A new copy
//! gets a new number, and the description is replaced only once the copy is complete, so a
//! crash while writing leaves the previous copy usable. A restored document reads from its
//! copy until it is saved, so that copy stays until then.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pdf_core::session::{RecoveryWrite, Session};
use serde::{Deserialize, Serialize};

use crate::documents::FileStamp;

/// How often recovery copies are written (section 7). LECTRIX_RECOVERY_INTERVAL_MS
/// overrides it, for tests.
pub const INTERVAL: Duration = Duration::from_secs(120);

/// Format of the slot descriptions; slots of other versions are left alone.
const VERSION: u32 = 1;

/// A slot's description (`<slot>.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Meta {
    version: u32,
    /// The document's own file.
    path: PathBuf,
    /// The copy's file name in the recovery folder.
    copy: String,
    /// When the copy was written.
    saved_at: SystemTime,
    /// The document's file when the copy was written, to notice if it changed since.
    stamp: Option<FileStamp>,
}

/// A copy left by an earlier run, as offered for restoring.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub slot: String,
    pub path: PathBuf,
    pub saved_at: SystemTime,
}

/// What restoring needs to know about a pending copy.
#[derive(Debug, Clone)]
pub struct RestoreSource {
    pub slot: String,
    pub copy: PathBuf,
    pub path: PathBuf,
    pub stamp: Option<FileStamp>,
}

/// One open document's slot.
struct Slot {
    name: String,
    /// Bumped whenever the slot is (re)created, so a copy that finishes after the document
    /// was saved or closed is thrown away instead of described.
    epoch: u64,
    /// The revision the current copy holds.
    written: Option<u64>,
    /// The current copy, named in the description.
    copy: Option<PathBuf>,
    /// The copy a restored document reads from: kept until the document is saved,
    /// reloaded or closed.
    backing: Option<PathBuf>,
}

#[derive(Default)]
struct Inner {
    slots: HashMap<u32, Slot>,
    counter: u64,
    /// Set when the app quits: nothing is written after that.
    closed: bool,
}

/// The recovery folder and the slots of the open documents. Without a folder (measurement
/// and test runs, LECTRIX_EPHEMERAL), recovery is off.
#[derive(Default)]
pub struct Recovery {
    dir: Option<PathBuf>,
    inner: Mutex<Inner>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Slot names come back from the webview, so they are checked before they become paths.
fn valid_slot(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-')
}

fn remove(path: &Path) {
    if let Err(e) = fs::remove_file(path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        crate::applog::warn(format!(
            "could not delete recovery file {}: {e}",
            path.display()
        ));
    }
}

impl Recovery {
    pub fn new(dir: Option<PathBuf>) -> Recovery {
        Recovery {
            dir,
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.dir.is_some()
    }

    fn meta_path(dir: &Path, slot: &str) -> PathBuf {
        dir.join(format!("{slot}.json"))
    }

    fn read_meta(dir: &Path, slot: &str) -> Option<Meta> {
        let meta: Meta =
            serde_json::from_slice(&fs::read(Self::meta_path(dir, slot)).ok()?).ok()?;
        (meta.version == VERSION && valid_copy_name(&meta.copy, slot)).then_some(meta)
    }

    /// Copies left by earlier runs, newest first. Files in the folder that belong to no
    /// slot (a copy a crash interrupted, or one a restored document read from) are deleted.
    pub fn pending(&self) -> Vec<Pending> {
        let Some(dir) = &self.dir else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let Ok(inner) = self.inner.lock() else {
            return Vec::new();
        };
        let owned: Vec<&str> = inner.slots.values().map(|s| s.name.as_str()).collect();
        let mut found = Vec::new();
        let mut referenced = Vec::new();
        let mut others = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            match name.strip_suffix(".json") {
                Some(slot) if valid_slot(slot) => {
                    if owned.contains(&slot) {
                        continue;
                    }
                    if let Some(meta) = Self::read_meta(dir, slot)
                        && dir.join(&meta.copy).is_file()
                    {
                        referenced.push(meta.copy.clone());
                        found.push(Pending {
                            slot: slot.to_owned(),
                            path: meta.path,
                            saved_at: meta.saved_at,
                        });
                        continue;
                    }
                    // Unreadable, or its copy is gone: nothing to restore.
                    remove(&entry.path());
                }
                _ => others.push(name),
            }
        }
        for name in others {
            // This run's copies and descriptions being written (`<slot>-<n>.pdf`,
            // `<slot>.json.tmp`) are not leftovers.
            let slot_owned = owned.iter().any(|slot| {
                name.strip_prefix(slot)
                    .is_some_and(|rest| rest.starts_with('-') || rest.starts_with('.'))
            });
            if !slot_owned && !referenced.contains(&name) {
                remove(&dir.join(name));
            }
        }
        found.sort_by_key(|p| std::cmp::Reverse(p.saved_at));
        found
    }

    /// The copy and file of a pending slot, if it is still there.
    pub fn source(&self, slot: &str) -> Option<RestoreSource> {
        let dir = self.dir.as_ref()?;
        if !valid_slot(slot) {
            return None;
        }
        let meta = Self::read_meta(dir, slot)?;
        let copy = dir.join(&meta.copy);
        copy.is_file().then(|| RestoreSource {
            slot: slot.to_owned(),
            copy,
            path: meta.path,
            stamp: meta.stamp,
        })
    }

    /// Makes a restored document the owner of the slot it came from. Its copy stays the
    /// slot's copy (it holds the document as restored) and is kept while the document
    /// reads from it.
    pub fn adopt(&self, id: u32, source: &RestoreSource, revision: u64) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.counter += 1;
        let epoch = inner.counter;
        inner.slots.insert(
            id,
            Slot {
                name: source.slot.clone(),
                epoch,
                written: Some(revision),
                copy: Some(source.copy.clone()),
                backing: Some(source.copy.clone()),
            },
        );
    }

    /// Deletes pending slots the user chose not to restore.
    pub fn discard_pending(&self, slots: &[String]) {
        let Some(dir) = &self.dir else {
            return;
        };
        let Ok(inner) = self.inner.lock() else {
            return;
        };
        for slot in slots {
            if !valid_slot(slot) || inner.slots.values().any(|s| &s.name == slot) {
                continue;
            }
            if let Some(meta) = Self::read_meta(dir, slot) {
                remove(&dir.join(meta.copy));
            }
            remove(&Self::meta_path(dir, slot));
        }
    }

    /// Writes a copy of document `id` if it has unsaved changes not yet copied, or deletes
    /// its copy if it has none.
    pub fn write(&self, id: u32, session: &Session, path: &Path) {
        let Some(dir) = &self.dir else {
            return;
        };
        let (slot, epoch, written, target) = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            if inner.closed {
                return;
            }
            inner.counter += 1;
            let n = inner.counter;
            let slot = inner.slots.entry(id).or_insert_with(|| Slot {
                name: format!("{}-{}-{id}", now_secs(), std::process::id()),
                epoch: n,
                written: None,
                copy: None,
                backing: None,
            });
            let target = dir.join(format!("{}-{n}.pdf", slot.name));
            (slot.name.clone(), slot.epoch, slot.written, target)
        };
        if let Err(e) = fs::create_dir_all(dir) {
            crate::applog::warn(format!("no recovery folder {}: {e}", dir.display()));
            return;
        }
        // The copy is written without holding the lock: it can take a moment for a large
        // file, and saving or closing must not wait for it.
        let result = session.write_recovery(&target, written);
        let Ok(mut inner) = self.inner.lock() else {
            remove(&target);
            return;
        };
        let current = inner
            .slots
            .get(&id)
            .is_some_and(|s| s.epoch == epoch && !inner.closed);
        match result {
            Ok(RecoveryWrite::Written { revision }) if current => {
                let meta = Meta {
                    version: VERSION,
                    path: path.to_path_buf(),
                    copy: target
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    saved_at: SystemTime::now(),
                    stamp: FileStamp::of(path),
                };
                if let Err(e) = write_meta(dir, &slot, &meta) {
                    crate::applog::warn(format!("could not describe recovery copy: {e}"));
                    remove(&target);
                    return;
                }
                if let Some(slot) = inner.slots.get_mut(&id) {
                    let previous = slot.copy.replace(target);
                    slot.written = Some(revision);
                    if let Some(previous) = previous
                        && slot.backing.as_ref() != Some(&previous)
                    {
                        remove(&previous);
                    }
                }
                crate::applog::info(format!(
                    "document {id}: recovery copy of revision {revision}"
                ));
            }
            // Saved, closed or quit while the copy was being written.
            Ok(RecoveryWrite::Written { .. }) => remove(&target),
            Ok(RecoveryWrite::Unchanged) => {}
            Ok(RecoveryWrite::Clean) => {
                if current {
                    discard_slot(dir, &mut inner, id);
                }
            }
            Err(e) => {
                remove(&target);
                crate::applog::warn(format!("document {id}: no recovery copy: {e}"));
            }
        }
    }

    /// Deletes document `id`'s copy: it was saved, reloaded or closed.
    pub fn discard(&self, id: u32) {
        let Some(dir) = &self.dir else {
            return;
        };
        if let Ok(mut inner) = self.inner.lock() {
            discard_slot(dir, &mut inner, id);
        }
    }

    /// Deletes every copy this run wrote, and writes no more: the app is quitting normally,
    /// after the user saved or chose not to save. Copies left by earlier runs stay.
    pub fn close(&self) {
        let Some(dir) = &self.dir else {
            return;
        };
        if let Ok(mut inner) = self.inner.lock() {
            inner.closed = true;
            let ids: Vec<u32> = inner.slots.keys().copied().collect();
            for id in ids {
                discard_slot(dir, &mut inner, id);
            }
        }
    }

    /// Writes copies again after `close`: quitting did not happen after all (an update's
    /// installer could not be started).
    pub fn reopen(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.closed = false;
        }
    }
}

/// A copy belongs to its slot: `<slot>-<n>.pdf`, nothing that leaves the folder.
fn valid_copy_name(copy: &str, slot: &str) -> bool {
    copy.strip_prefix(slot)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_suffix(".pdf"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn discard_slot(dir: &Path, inner: &mut Inner, id: u32) {
    if let Some(slot) = inner.slots.remove(&id) {
        remove(&Recovery::meta_path(dir, &slot.name));
        for file in [slot.copy, slot.backing].into_iter().flatten() {
            remove(&file);
        }
    }
}

/// Replaces `<slot>.json` in one rename, so it always names a complete copy.
fn write_meta(dir: &Path, slot: &str, meta: &Meta) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(meta).map_err(std::io::Error::other)?;
    let temp = dir.join(format!("{slot}.json.tmp"));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, Recovery::meta_path(dir, slot)).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_core::ops::Operation;
    use pdf_core::save::SaveKind;
    use pdf_core::testgen::{SampleSpec, sample_document};

    fn setup(name: &str) -> (PathBuf, PathBuf) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/test-output/recovery")
            .join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let doc = root.join("doc.pdf");
        let sample = sample_document(&SampleSpec::default()).unwrap();
        pdf_core::save::save_atomic(&sample, SaveKind::Full, None, &doc).unwrap();
        (root.join("recovery"), doc)
    }

    fn rotate(session: &Session, page: usize) {
        session
            .apply(Operation::RotatePages {
                pages: vec![page],
                degrees: 90,
            })
            .unwrap();
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|e| {
                e.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn copies_follow_unsaved_changes_and_are_offered_after_a_crash() {
        let (dir, doc) = setup("crash");
        let (session, _) = Session::open(&doc, None).unwrap();
        let recovery = Recovery::new(Some(dir.clone()));

        recovery.write(1, &session, &doc);
        assert!(files(&dir).is_empty(), "nothing to recover yet");

        rotate(&session, 0);
        recovery.write(1, &session, &doc);
        let first = files(&dir);
        assert_eq!(first.len(), 2, "{first:?}");
        recovery.write(1, &session, &doc);
        assert_eq!(files(&dir), first, "unchanged: no new copy");

        rotate(&session, 1);
        recovery.write(1, &session, &doc);
        let second = files(&dir);
        assert_eq!(second.len(), 2, "the previous copy is gone: {second:?}");
        assert_ne!(second, first);

        // This run's own slots are not offered; a later run (after a crash) sees them.
        assert!(recovery.pending().is_empty());
        let next_run = Recovery::new(Some(dir.clone()));
        let pending = next_run.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].path, doc);
        let source = next_run.source(&pending[0].slot).unwrap();
        assert_eq!(source.stamp, FileStamp::of(&doc));

        let (restored, info) = Session::restore(&source.copy, &source.path, None).unwrap();
        assert!(info.state.dirty);
        assert_eq!((info.pages[0].width, info.pages[1].width), (792.0, 792.0));
        next_run.adopt(7, &source, info.state.revision);
        assert!(
            next_run.pending().is_empty(),
            "adopted slots are not offered"
        );

        // The restored document's copy stays while it reads from it, even after a newer one.
        rotate(&restored, 2);
        next_run.write(7, &restored, &doc);
        assert!(source.copy.exists());
        assert_eq!(files(&dir).len(), 3);
        restored.save(SaveKind::Incremental, None).unwrap();
        next_run.discard(7);
        assert!(files(&dir).is_empty());
    }

    #[test]
    fn saving_undoing_closing_and_quitting_delete_copies() {
        let (dir, doc) = setup("lifecycle");
        let (session, _) = Session::open(&doc, None).unwrap();
        let recovery = Recovery::new(Some(dir.clone()));

        rotate(&session, 0);
        recovery.write(1, &session, &doc);
        session.undo().unwrap();
        recovery.write(1, &session, &doc);
        assert!(files(&dir).is_empty(), "undone to the saved state");

        rotate(&session, 0);
        recovery.write(1, &session, &doc);
        recovery.discard(1);
        assert!(files(&dir).is_empty(), "saved or closed");

        recovery.write(1, &session, &doc);
        assert_eq!(files(&dir).len(), 2);
        recovery.close();
        assert!(files(&dir).is_empty(), "quit normally");
        recovery.write(1, &session, &doc);
        assert!(files(&dir).is_empty(), "nothing is written after quitting");
    }

    #[test]
    fn copies_of_earlier_runs_survive_quitting_until_discarded() {
        let (dir, doc) = setup("discard");
        let (session, _) = Session::open(&doc, None).unwrap();
        rotate(&session, 0);
        Recovery::new(Some(dir.clone())).write(1, &session, &doc);

        let next_run = Recovery::new(Some(dir.clone()));
        let pending = next_run.pending();
        assert_eq!(pending.len(), 1);
        next_run.close();
        assert_eq!(
            next_run.pending(),
            pending,
            "not now: offered again next time"
        );
        next_run.discard_pending(std::slice::from_ref(&pending[0].slot));
        assert!(files(&dir).is_empty());
    }

    #[test]
    fn leftovers_and_bad_names_are_cleaned_up_or_refused() {
        let (dir, _) = setup("leftovers");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("123-4-5-9.pdf"), b"interrupted copy").unwrap();
        fs::write(dir.join("123-4-6.json"), b"{ not json").unwrap();
        let meta = Meta {
            version: VERSION,
            path: PathBuf::from("C:/x.pdf"),
            copy: "../../outside.pdf".into(),
            saved_at: SystemTime::now(),
            stamp: None,
        };
        write_meta(&dir, "123-4-7", &meta).unwrap();
        let recovery = Recovery::new(Some(dir.clone()));
        assert!(recovery.pending().is_empty());
        assert!(files(&dir).is_empty());
        for slot in ["../x", "A-1", "", "1/2"] {
            assert!(recovery.source(slot).is_none(), "{slot}");
        }
    }

    #[test]
    fn without_a_folder_recovery_is_off() {
        let (_, doc) = setup("off");
        let (session, _) = Session::open(&doc, None).unwrap();
        rotate(&session, 0);
        let recovery = Recovery::default();
        assert!(!recovery.enabled());
        recovery.write(1, &session, &doc);
        assert!(recovery.pending().is_empty());
    }
}
