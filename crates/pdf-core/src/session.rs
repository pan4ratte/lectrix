//! Document sessions: one actor thread per open document.
//!
//! MuPDF documents are not `Send` (ADR 0001), so each open document lives on its own thread
//! that exclusively owns it. Callers talk to it through a channel. Display lists are
//! `Send + Sync`, so rendering, text extraction and search run on the caller's (worker)
//! thread.
//!
//! **Revisions.** Every distinct document content has a revision number. Applying an
//! operation creates a new revision; undo and redo return to the revision of that journal
//! position, so images rendered for it can be reused. The document is dirty when its
//! revision differs from the revision last saved.
//!
//! **Saving** writes through [`crate::save::save_atomic`] and then reopens the saved file
//! (ADR 0003): MuPDF's in-memory document still points at the old file's offsets after an
//! incremental save. The undo history therefore starts again after each save.
//!
//! **Recovery copies** (AGENTS.md section 7, crash recovery) are snapshots: the file the
//! document was opened from plus its unsaved changes as an incremental update, written
//! without disturbing the document or its undo history. A restored session reads from the
//! copy but belongs to the original file, and is dirty until saved. Its undo history
//! starts at the restore (ADR 0001: MuPDF 1.27.2 cannot load a saved journal).
//!
//! **Expanded bookmarks** are not edits: expanding or collapsing a bookmark in the panel
//! neither dirties the document nor adds an undo step. The actor keeps
//! the panel's states and writes them into the outline when the document is next saved.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use mupdf::DisplayList;
use mupdf::pdf::PdfDocument;

use crate::annot::read::{AnnotationInfo, read_all as read_annotations};
use crate::annot::repair::{self, RepairChange, RepairScan};
use crate::docinfo::{self, DocumentFlags, PageLayout, PdfA, read_flags, read_page_layout};
use crate::error::{Error, Result};
use crate::ffi::{Journal, open_pdf_shared, save_snapshot};
use crate::geometry::{PageGeometry, read_page_boxes};
use crate::labels::{self, LabelRule};
use crate::links::{self, PageLink};
use crate::merge::{self, MergeReport};
use crate::ops::{NumberedDests, Operation, OutlineEffect};
use crate::outline::{self, Outline};
use crate::pages::DeleteReport;
use crate::render::{self, PixelRect, RgbaImage};
use crate::save::{SaveKind, SaveOutcome, save_atomic, save_atomic_checked};
use crate::text::{self, FoundWord, PageText, SearchHit};

/// Size of a page in view space at zoom 1 (points), after rotation and cropping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
    /// The page's `/Rotate`, normalized (0, 90, 180 or 270): `width` and `height` are after it.
    pub rotation: u16,
}

/// Undo position and save state, returned by every command that can change them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentState {
    pub revision: u64,
    pub dirty: bool,
    /// Name of the step Undo would revert ("Rotate page").
    pub undo_name: Option<String>,
    /// Name of the step Redo would re-apply.
    pub redo_name: Option<String>,
}

/// The document's page labels: the rules as stored, and the label they give each page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLabels {
    /// Sorted by start page; may lack a rule at the first page, or have rules past the
    /// last page, in files other apps wrote.
    pub rules: Vec<LabelRule>,
    /// One label per page.
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub path: PathBuf,
    pub pages: Vec<PageSize>,
    /// `None` when the document has no `/PageLabels`.
    pub labels: Option<PageLabels>,
    pub flags: DocumentFlags,
    /// The document is open for reading only: every operation is refused until
    /// [`Session::enable_editing`] (a file that claims PDF/A, section 5.4).
    pub read_only: bool,
    /// How the document asks to be laid out (its `/PageLayout`), if it does.
    pub page_layout: Option<PageLayout>,
    pub state: DocumentState,
    /// The bookmarks, with the panel's expanded states.
    pub outline: Outline,
    /// Every page's annotations (index = page).
    pub annotations: Vec<Vec<AnnotationInfo>>,
    /// Time to open the file and read every page's geometry.
    pub open_time: Duration,
}

/// What a mutating command changed.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentChange {
    pub state: DocumentState,
    /// Pages whose size (or rotation) changed, with their new size.
    pub changed_pages: Vec<(usize, PageSize)>,
    /// The new labels, if they changed (`Some(None)`: the labels were removed).
    pub labels: Option<Option<PageLabels>>,
    /// The new bookmarks, if they changed (targets move when pages rotate, too).
    pub outline: Option<Outline>,
    /// The id of the object the operation created (the new bookmark).
    pub created: Option<u32>,
    /// The number of pages (it changes when pages are inserted, or that is undone).
    pub page_count: usize,
    /// What inserting pages did (renamed names, links left out).
    pub merge_report: Option<MergeReport>,
    /// Pages whose annotations changed, with all of their annotations now.
    pub annotations: Vec<(usize, Vec<AnnotationInfo>)>,
    /// What repairing annotations changed (for the app log).
    pub repairs: Option<Vec<RepairChange>>,
    /// Where each page was before, when the pages are no longer the same ones in the same
    /// order (moved, deleted, inserted, or any of that undone): for each page now, its
    /// index before, or `None` for a page that was not there.
    pub page_map: Option<Vec<Option<usize>>>,
    /// What deleting pages took out with them.
    pub deleted: Option<DeleteReport>,
}

/// What [`Session::write_recovery`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryWrite {
    /// The document has no unsaved changes: there is nothing to recover.
    Clean,
    /// The copy written earlier for this revision is still current.
    Unchanged,
    /// A copy of `revision` was written.
    Written { revision: u64 },
}

#[derive(Debug, Clone)]
pub struct SaveResult {
    pub state: DocumentState,
    pub outcome: SaveOutcome,
    /// The file the document is now associated with (the Save As target, if any).
    pub path: PathBuf,
    /// The bookmarks, if their ids changed (an optimized save renumbers objects).
    pub outline: Option<Outline>,
    /// Pages whose annotations read differently after the save (renumbered objects).
    pub annotations: Vec<(usize, Vec<AnnotationInfo>)>,
    /// The PDF/A level the file claimed until this save took the claim out of it, because
    /// the changes saved may not comply (section 5.4).
    pub pdfa_removed: Option<PdfA>,
}

/// How [`Session::open_with`] opens a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpenOptions {
    /// Open a file that claims PDF/A for reading only (section 5.4): operations are
    /// refused until [`Session::enable_editing`].
    pub pdfa_read_only: bool,
}

/// Where [`Session::extract`] is. Its callback returns false to stop: nothing is written,
/// and the extraction fails with [`Error::Cancelled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractStep {
    /// Pages are being copied.
    Copying { done: usize, total: usize },
    /// Every page is copied; the file is about to be written.
    Writing,
    /// The file is written and about to take its name.
    Replacing,
}

/// Search results for one page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageHits {
    pub page: usize,
    pub hits: Vec<SearchHit>,
    /// The words of the page that contain the needle (`text::words_containing`).
    pub words: Vec<FoundWord>,
}

type Reply<T> = SyncSender<Result<T>>;

enum Command {
    Info {
        reply: Reply<DocumentInfo>,
    },
    DisplayList {
        page: usize,
        /// Object numbers of annotations to leave out (sorted, without repeats).
        hidden: Vec<u32>,
        cache: bool,
        reply: Reply<(Arc<DisplayList>, u64)>,
    },
    Apply {
        op: Operation,
        reply: Reply<DocumentChange>,
    },
    Undo {
        reply: Reply<DocumentChange>,
    },
    Redo {
        reply: Reply<DocumentChange>,
    },
    SetBookmarkOpen {
        id: u32,
        open: bool,
        reply: Reply<()>,
    },
    EnableEditing {
        reply: Reply<()>,
    },
    ScanAnnotations {
        reply: Reply<RepairScan>,
    },
    Links {
        page: usize,
        reply: Reply<(Vec<PageLink>, u64)>,
    },
    Save {
        kind: SaveKind,
        target: Option<PathBuf>,
        reply: Reply<SaveResult>,
    },
    WriteRecovery {
        target: PathBuf,
        written: Option<u64>,
        reply: Reply<RecoveryWrite>,
    },
    Extract {
        pages: Vec<usize>,
        target: PathBuf,
        step: Box<dyn FnMut(ExtractStep) -> bool + Send>,
        reply: Reply<MergeReport>,
    },
    Reload {
        reply: Reply<DocumentInfo>,
    },
    Close,
}

/// Handle to a document actor. Cheap to clone; the actor stops on [`Session::close`] or
/// when every handle is dropped.
#[derive(Clone)]
pub struct Session {
    tx: Sender<Command>,
}

impl Session {
    /// Opens `path` on a new actor thread and returns once the document is loaded.
    /// Encrypted documents need `password` ([`Error::PasswordRequired`] without one,
    /// [`Error::WrongPassword`] if it does not fit).
    pub fn open(path: &Path, password: Option<&str>) -> Result<(Session, DocumentInfo)> {
        Self::open_with(path, password, OpenOptions::default())
    }

    /// Opens `path` as [`Session::open`] does, with `options`.
    pub fn open_with(
        path: &Path,
        password: Option<&str>,
        options: OpenOptions,
    ) -> Result<(Session, DocumentInfo)> {
        Self::spawn(Origin {
            file: path.to_path_buf(),
            path: path.to_path_buf(),
            password: password.map(str::to_owned),
            restored: false,
            options,
        })
    }

    /// Opens a recovery copy written by [`Session::write_recovery`] as the document of
    /// `path`, the file it is a copy of. The session reads from `copy`, which must stay
    /// until the session is saved, reloaded or closed. It saves to `path`, and it is dirty
    /// until then. Its undo history starts here. It is being edited, so it is never opened
    /// for reading only; what its unsaved changes are is not known, so if it claims PDF/A,
    /// saving it takes the claim out.
    pub fn restore(
        copy: &Path,
        path: &Path,
        password: Option<&str>,
    ) -> Result<(Session, DocumentInfo)> {
        Self::spawn(Origin {
            file: copy.to_path_buf(),
            path: path.to_path_buf(),
            password: password.map(str::to_owned),
            restored: true,
            options: OpenOptions::default(),
        })
    }

    /// Opens `file` as a document that has no file of its own yet: a working copy of
    /// something just made (pages extracted from another document), which the user has
    /// not saved under a name. It has unsaved changes from the start and until it is saved
    /// to another file with [`Session::save`], which it then belongs to; undoing every
    /// change leaves it unsaved all the same. `file` must stay until then, or until the
    /// session is closed. Never opened for reading only.
    pub fn open_unsaved(file: &Path) -> Result<(Session, DocumentInfo)> {
        Self::spawn(Origin {
            file: file.to_path_buf(),
            path: file.to_path_buf(),
            password: None,
            restored: true,
            options: OpenOptions::default(),
        })
    }

    fn spawn(origin: Origin) -> Result<(Session, DocumentInfo)> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let name = origin
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        thread::Builder::new()
            .name(format!("doc:{name}"))
            .spawn(move || actor(origin, rx, ready_tx))?;
        let info = ready_rx.recv().map_err(|_| Error::ActorGone)??;
        Ok((Session { tx }, info))
    }

    fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> Result<T> {
        let (reply, rx) = mpsc::sync_channel(1);
        self.tx.send(make(reply)).map_err(|_| Error::ActorGone)?;
        rx.recv().map_err(|_| Error::ActorGone)?
    }

    pub fn info(&self) -> Result<DocumentInfo> {
        self.call(|reply| Command::Info { reply })
    }

    /// The display list of `page` and the revision it shows. Cached on the actor.
    pub fn display_list(&self, page: usize) -> Result<(Arc<DisplayList>, u64)> {
        self.display_list_hiding(page, &[])
    }

    /// The display list of `page` without the annotations whose object numbers are in
    /// `hidden` (the ones the annotation list filters out), and the revision it shows.
    /// Cached on the actor.
    pub fn display_list_hiding(
        &self,
        page: usize,
        hidden: &[u32],
    ) -> Result<(Arc<DisplayList>, u64)> {
        let mut hidden = hidden.to_vec();
        hidden.sort_unstable();
        hidden.dedup();
        self.call(|reply| Command::DisplayList {
            page,
            hidden,
            cache: true,
            reply,
        })
    }

    /// Renders `page` (or one tile of it) to RGBA on the calling thread, without the
    /// annotations in `hidden`.
    pub fn render_rgba(
        &self,
        page: usize,
        scale: f32,
        tile: Option<PixelRect>,
        hidden: &[u32],
    ) -> Result<(RgbaImage, u64)> {
        let (list, revision) = self.display_list_hiding(page, hidden)?;
        Ok((render::render_rgba(&list, scale, tile)?, revision))
    }

    /// Renders `page` at `scale` to PNG: display list on the actor, raster and encode on
    /// the calling thread.
    pub fn render_png(&self, page: usize, scale: f32) -> Result<render::RenderedPng> {
        let t0 = Instant::now();
        let (list, _) = self.display_list(page)?;
        let t1 = Instant::now();
        let pixmap = render::rasterize(&list, scale)?;
        let t2 = Instant::now();
        let png = render::encode_png(&pixmap)?;
        let t3 = Instant::now();
        Ok(render::RenderedPng {
            width: pixmap.width(),
            height: pixmap.height(),
            png,
            timings: render::RenderTimings {
                display_list: t1 - t0,
                raster: t2 - t1,
                encode: t3 - t2,
            },
        })
    }

    /// Text geometry of `page` and the revision it belongs to.
    pub fn page_text(&self, page: usize) -> Result<(PageText, u64)> {
        let (list, revision) = self.display_list(page)?;
        Ok((text::page_text(&list)?, revision))
    }

    /// Searches `pages` for `needle` (a range past the last page stops at the last page).
    /// Pages are visited one by one, so callers search large documents in chunks and can
    /// stop between them. Display lists built for the
    /// search are not cached, so a search does not evict the pages on screen.
    pub fn search(&self, needle: &str, pages: Range<usize>) -> Result<(Vec<PageHits>, u64)> {
        let mut out = Vec::new();
        let mut revision = 0;
        for page in pages {
            let (list, rev) = match self.call(|reply| Command::DisplayList {
                page,
                hidden: Vec::new(),
                cache: false,
                reply,
            }) {
                Ok(found) => found,
                // The range may run past the last page; stop there.
                Err(Error::PageOutOfRange(_)) => break,
                Err(e) => return Err(e),
            };
            revision = rev;
            let (hits, words) = text::search_page_words(&list, needle)?;
            if !hits.is_empty() || !words.is_empty() {
                out.push(PageHits { page, hits, words });
            }
        }
        Ok((out, revision))
    }

    pub fn apply(&self, op: Operation) -> Result<DocumentChange> {
        self.call(|reply| Command::Apply { op, reply })
    }

    pub fn undo(&self) -> Result<DocumentChange> {
        self.call(|reply| Command::Undo { reply })
    }

    pub fn redo(&self) -> Result<DocumentChange> {
        self.call(|reply| Command::Redo { reply })
    }

    /// Records that a bookmark was expanded or collapsed in the panel. Not an edit: the
    /// state is written into the outline at the next save.
    pub fn set_bookmark_open(&self, id: u32, open: bool) -> Result<()> {
        self.call(|reply| Command::SetBookmarkOpen { id, open, reply })
    }

    /// Lets a document that was opened for reading only be changed. Bookmarks expanded
    /// or collapsed meanwhile are written at the next save, as in any document.
    pub fn enable_editing(&self) -> Result<()> {
        self.call(|reply| Command::EnableEditing { reply })
    }

    /// The links on `page` and what each does when clicked (`links.rs`), and the revision
    /// they were read at.
    pub fn page_links(&self, page: usize) -> Result<(Vec<PageLink>, u64)> {
        self.call(|reply| Command::Links { page, reply })
    }

    /// Counts annotation problems that "Repair annotations" would fix (section 5.3).
    pub fn scan_annotations(&self) -> Result<RepairScan> {
        self.call(|reply| Command::ScanAnnotations { reply })
    }

    /// Saves to the document's own file (`target: None`) or to `target` (Save As). After
    /// a Save As, the session belongs to `target`.
    pub fn save(&self, kind: SaveKind, target: Option<PathBuf>) -> Result<SaveResult> {
        self.call(|reply| Command::Save {
            kind,
            target,
            reply,
        })
    }

    /// Writes a recovery copy to `target` if the document has unsaved changes and
    /// `written` (the revision of the last copy) is not the current revision. The document
    /// and its undo history stay as they were. Documents MuPDF repaired on open are copied
    /// in full instead.
    pub fn write_recovery(&self, target: &Path, written: Option<u64>) -> Result<RecoveryWrite> {
        self.call(|reply| Command::WriteRecovery {
            target: target.to_path_buf(),
            written,
            reply,
        })
    }

    /// Copies `pages` (0-based, in this order, each once) into a new file at `target`, as
    /// the document is now, unsaved changes included ([`merge::extract`] says what comes
    /// along). The document itself is not changed, so this works while it is open for
    /// reading only. `step` is told how far it is and can stop it. The document renders
    /// nothing new meanwhile. Refused: a document whose permissions do not allow copying
    /// its content ([`Error::CopyNotPermitted`]), and the document's own file as target.
    pub fn extract(
        &self,
        pages: Vec<usize>,
        target: &Path,
        step: impl FnMut(ExtractStep) -> bool + Send + 'static,
    ) -> Result<MergeReport> {
        self.call(|reply| Command::Extract {
            pages,
            target: target.to_path_buf(),
            step: Box::new(step),
            reply,
        })
    }

    /// Discards the in-memory document and opens the file again (after it changed on
    /// disk).
    pub fn reload(&self) -> Result<DocumentInfo> {
        self.call(|reply| Command::Reload { reply })
    }

    pub fn close(&self) {
        // The actor may already be gone; nothing to do then.
        let _ = self.tx.send(Command::Close);
    }
}

/// What a cached display list shows: a page, without the annotations with these object
/// numbers.
type ListKey = (usize, Vec<u32>);

/// Display lists kept per document. Small: a display list for a scanned page holds a
/// reference to its decoded image.
const DISPLAY_LIST_CACHE: usize = 24;

/// Where an actor's document comes from.
struct Origin {
    /// The file MuPDF reads.
    file: PathBuf,
    /// The file the document belongs to (saved to).
    path: PathBuf,
    password: Option<String>,
    /// The document differs from what `path` holds from the start: `file` is a recovery
    /// copy of `path`, or a working copy of a document not saved under a name yet.
    restored: bool,
    options: OpenOptions,
}

struct Actor {
    doc: PdfDocument,
    /// The file the document belongs to: where it is saved.
    path: PathBuf,
    /// The file MuPDF reads: `path`, or a recovery copy of it after a restore.
    file: PathBuf,
    password: Option<String>,
    flags: DocumentFlags,
    /// Open for reading only: operations are refused and nothing but what the file
    /// already holds is written.
    read_only: bool,
    pages: Vec<PageSize>,
    /// Each page's object number: what tells a page from another when their order changes.
    page_ids: Vec<i32>,
    labels: Option<PageLabels>,
    /// The outline as the document has it, kept up to date after each change as far as
    /// the change can have touched it ([`Actor::refresh_outline`]).
    outline: Outline,
    /// The outline was written without being read again (the bookmarks' expanded states,
    /// for a save that then failed), so the next change reads all of it.
    outline_stale: bool,
    /// Every page's annotations as last read (index = page).
    annotations: Vec<Vec<AnnotationInfo>>,
    /// Bookmarks expanded or collapsed in the panel since the last save (id to open).
    open_states: HashMap<u32, bool>,
    /// Revision of the content at each journal position (index 0: as opened or saved).
    history: Vec<u64>,
    /// Whether the step that led to each journal position may have broken a PDF/A claim
    /// (index 0: the changes a restored document came with, which are not known).
    breaks_pdfa: Vec<bool>,
    /// What the step that led to each journal position can have done to the bookmarks
    /// (index 0 stands for no step). A position the session did not make counts as
    /// anything.
    effects: Vec<OutlineEffect>,
    /// The journal position from which on no destination in the document is written with
    /// a page number, once a move or a deletion has gone through them all (`pages.rs`);
    /// moves skip that while it holds. Undoing to before it forgets it.
    by_reference_since: Option<usize>,
    position: usize,
    saved_revision: u64,
    next_revision: u64,
    lists: VecDeque<(ListKey, Arc<DisplayList>)>,
    /// Set when a save succeeded but the saved file could not be reopened: the in-memory
    /// document no longer matches the file, so only a full save is safe.
    needs_full_save: bool,
}

fn actor(origin: Origin, rx: Receiver<Command>, ready: SyncSender<Result<DocumentInfo>>) {
    let start = Instant::now();
    let mut actor = match Actor::open(origin) {
        Ok(actor) => actor,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let info = actor.info(start.elapsed());
    if ready.send(Ok(info)).is_err() {
        return;
    }
    while let Ok(command) = rx.recv() {
        match command {
            Command::Info { reply } => {
                let _ = reply.send(Ok(actor.info(Duration::ZERO)));
            }
            Command::DisplayList {
                page,
                hidden,
                cache,
                reply,
            } => {
                let _ = reply.send(actor.display_list(page, hidden, cache));
            }
            Command::Apply { op, reply } => {
                let _ = reply.send(actor.apply(&op));
            }
            Command::Undo { reply } => {
                let _ = reply.send(actor.step(false));
            }
            Command::Redo { reply } => {
                let _ = reply.send(actor.step(true));
            }
            Command::SetBookmarkOpen { id, open, reply } => {
                actor.set_bookmark_open(id, open);
                let _ = reply.send(Ok(()));
            }
            Command::EnableEditing { reply } => {
                actor.read_only = false;
                let _ = reply.send(Ok(()));
            }
            Command::ScanAnnotations { reply } => {
                let _ = reply.send(repair::scan(&actor.doc));
            }
            Command::Links { page, reply } => {
                let links = links::page_links(&actor.doc, page);
                let _ = reply.send(links.map(|links| (links, actor.revision())));
            }
            Command::Save {
                kind,
                target,
                reply,
            } => {
                let _ = reply.send(actor.save(kind, target));
            }
            Command::WriteRecovery {
                target,
                written,
                reply,
            } => {
                let _ = reply.send(actor.write_recovery(&target, written));
            }
            Command::Extract {
                pages,
                target,
                mut step,
                reply,
            } => {
                let _ = reply.send(actor.extract(&pages, &target, &mut *step));
            }
            Command::Reload { reply } => {
                let start = Instant::now();
                let _ = reply.send(actor.reload().map(|()| actor.info(start.elapsed())));
            }
            Command::Close => break,
        }
    }
}

/// Opens a document through the share-delete stream and unlocks it if needed.
fn load(path: &Path, password: Option<&str>) -> Result<PdfDocument> {
    let mut doc = open_pdf_shared(path)?;
    if doc.needs_password()? {
        let Some(password) = password else {
            return Err(Error::PasswordRequired);
        };
        if !doc.authenticate(password)? {
            return Err(Error::WrongPassword);
        }
    }
    repair_if_without_pages(&mut doc)?;
    Journal::new(&mut doc).enable()?;
    Ok(doc)
}

/// A file can have a cross-reference table that MuPDF reads without an error but that
/// leads nowhere: its last section names itself as the previous one (Lectrix wrote such
/// files up to version 1.0.0, when saving again after a save that failed), so that section is read alone
/// and the catalog is not found. MuPDF repairs a table it cannot read; this one it reads,
/// and the document has no pages. Then the table is rebuilt from the objects in the file,
/// which brings back everything in it; the document counts as repaired, so its next save
/// is a full one (section 5.4). A file that still has no pages cannot be shown.
pub fn repair_if_without_pages(doc: &mut PdfDocument) -> Result<()> {
    let has_pages = |doc: &PdfDocument| matches!(doc.page_count(), Ok(n) if n > 0);
    if has_pages(doc) {
        return Ok(());
    }
    if !crate::ffi::was_repaired(doc)? {
        // If the repair fails too, the file has no pages either way.
        let _ = crate::ffi::repair(doc);
    }
    if has_pages(doc) {
        Ok(())
    } else {
        Err(Error::NotPdf)
    }
}

impl Actor {
    fn open(origin: Origin) -> Result<Actor> {
        let Origin {
            file,
            path,
            password,
            restored,
            options,
        } = origin;
        let doc = load(&file, password.as_deref())?;
        let flags = read_flags(&doc)?;
        let read_only = options.pdfa_read_only && flags.pdfa.is_some() && !restored;
        let pages = page_sizes(&doc)?;
        let page_ids = page_ids(&doc)?;
        let labels = page_labels(&doc, pages.len())?;
        let outline = outline::read_bookmarks(&doc)?;
        let annotations = read_annotations(&doc)?;
        // A restored document differs from its file until it is saved. The file's revision
        // (0) is at no journal position, so undoing back to the restore leaves it dirty.
        let (history, next_revision) = if restored { (vec![1], 2) } else { (vec![0], 1) };
        Ok(Actor {
            doc,
            path,
            file,
            password,
            flags,
            read_only,
            pages,
            page_ids,
            labels,
            outline,
            outline_stale: false,
            annotations,
            open_states: HashMap::new(),
            history,
            breaks_pdfa: vec![restored],
            effects: vec![OutlineEffect::Any],
            by_reference_since: None,
            position: 0,
            saved_revision: 0,
            next_revision,
            lists: VecDeque::new(),
            needs_full_save: false,
        })
    }

    fn revision(&self) -> u64 {
        // `position` is always a valid index: it moves only within `history`.
        self.history.get(self.position).copied().unwrap_or(0)
    }

    fn state(&mut self) -> DocumentState {
        let journal = Journal::new(&mut self.doc).state().ok();
        let revision = self.revision();
        DocumentState {
            revision,
            dirty: revision != self.saved_revision,
            undo_name: journal.as_ref().and_then(|j| j.undo_name.clone()),
            redo_name: journal.and_then(|j| j.redo_name),
        }
    }

    fn info(&mut self, open_time: Duration) -> DocumentInfo {
        DocumentInfo {
            path: self.path.clone(),
            pages: self.pages.clone(),
            labels: self.labels.clone(),
            flags: self.flags,
            read_only: self.read_only,
            // A value that can't be read counts as none: it only picks the first view.
            page_layout: read_page_layout(&self.doc).ok().flatten(),
            state: self.state(),
            outline: self.outline_view(),
            annotations: self.annotations.clone(),
            open_time,
        }
    }

    /// The outline with the panel's expanded states.
    fn outline_view(&self) -> Outline {
        self.outline.clone().with_open_states(&self.open_states)
    }

    fn set_bookmark_open(&mut self, id: u32, open: bool) {
        let mut stored = None;
        self.outline.for_each(|b| {
            if b.id == id {
                stored = Some(b.open);
            }
        });
        match stored {
            Some(s) if s == open => {
                self.open_states.remove(&id);
            }
            Some(_) => {
                self.open_states.insert(id, open);
            }
            None => {}
        }
    }

    /// Forgets panel states of bookmarks that are gone or already stored that way.
    fn prune_open_states(&mut self) {
        let mut stored = HashMap::new();
        self.outline.for_each(|b| {
            stored.insert(b.id, b.open);
        });
        self.open_states
            .retain(|id, open| stored.get(id).is_some_and(|s| s != open));
    }

    /// Writes the panel's expanded states into the outline, as an unnamed journal change so
    /// they are not an undo step of their own. Skipped where writing them would be an edit
    /// the user did not ask for: a signed document with no other changes (the save would
    /// add a revision after the signature), one whose permissions do not allow outline
    /// changes, or one open for reading only (they wait until editing is enabled).
    fn write_open_states(&mut self) -> Result<()> {
        if self.open_states.is_empty()
            || self.read_only
            || !self.flags.can_assemble
            || (self.flags.signed && self.revision() == self.saved_revision)
        {
            return Ok(());
        }
        let states: Vec<(u32, bool)> = self.open_states.iter().map(|(&k, &v)| (k, v)).collect();
        Journal::new(&mut self.doc).begin_implicit()?;
        match outline::edit::set_open(&mut self.doc, &states) {
            Ok(_) => self.doc.end_operation()?,
            Err(e) => {
                self.doc.abandon_operation()?;
                return Err(e);
            }
        }
        Ok(())
    }

    fn display_list(
        &mut self,
        page: usize,
        hidden: Vec<u32>,
        cache: bool,
    ) -> Result<(Arc<DisplayList>, u64)> {
        let revision = self.revision();
        let key = (page, hidden);
        if let Some(i) = self.lists.iter().position(|(k, _)| *k == key) {
            // Move to the back (most recently used).
            if let Some(entry) = self.lists.remove(i) {
                let list = entry.1.clone();
                self.lists.push_back(entry);
                return Ok((list, revision));
            }
        }
        let list = Arc::new(render::display_list_hiding(&self.doc, page, &key.1)?);
        if cache {
            if self.lists.len() >= DISPLAY_LIST_CACHE {
                self.lists.pop_front();
            }
            self.lists.push_back((key, list.clone()));
        }
        Ok((list, revision))
    }

    fn apply(&mut self, op: &Operation) -> Result<DocumentChange> {
        op.validate(self.pages.len())?;
        if self.read_only {
            return Err(Error::ReadOnly);
        }
        if (!self.flags.can_assemble && op.needs_assemble())
            || (!self.flags.can_annotate && op.needs_annotate())
        {
            return Err(Error::NotPermitted);
        }
        let before = Journal::new(&mut self.doc).state()?.current;
        let name = op.step_name(&self.doc);
        self.doc.begin_operation(&name)?;
        let applied = match op.apply(&mut self.doc, self.by_reference_since.is_some()) {
            Ok(applied) => {
                self.doc.end_operation()?;
                applied
            }
            Err(e) => {
                self.doc.abandon_operation()?;
                return Err(e);
            }
        };
        let after = Journal::new(&mut self.doc).state()?.current;
        // After steps the session did not make, the outline it holds may not be the
        // document's.
        let mut effect = if before < self.effects.len() {
            op.outline_effect()
        } else {
            OutlineEffect::Any
        };
        if after != before {
            let revision = self.next_revision;
            self.next_revision += 1;
            self.history.truncate(before + 1);
            self.history.resize(after + 1, revision);
            self.breaks_pdfa.truncate(before + 1);
            self.breaks_pdfa.resize(after + 1, op.may_break_pdfa());
            // One operation is one step; any others between are not known.
            if after != before + 1 {
                effect = OutlineEffect::Any;
            }
            self.effects.truncate(before + 1);
            self.effects.resize(after, OutlineEffect::Any);
            self.effects.push(effect);
            self.position = after;
            match applied.numbered_dests {
                NumberedDests::AsBefore => {}
                NumberedDests::NoneBefore => {
                    self.by_reference_since.get_or_insert(before);
                }
                NumberedDests::NoneNow => {
                    self.by_reference_since.get_or_insert(after);
                }
                NumberedDests::Unknown => self.by_reference_since = None,
            }
        }
        let mut change = self.changed(effect)?;
        change.created = applied.created;
        change.merge_report = applied.report;
        change.repairs = applied.repairs;
        change.deleted = applied.deleted;
        Ok(change)
    }

    /// Undo (`forward: false`) or redo.
    fn step(&mut self, forward: bool) -> Result<DocumentChange> {
        let mut journal = Journal::new(&mut self.doc);
        let state = journal.state()?;
        let from = state.current;
        if forward {
            if state.current >= state.steps {
                return Err(Error::NothingToRedo);
            }
            journal.redo()?;
        } else {
            if state.current == 0 {
                return Err(Error::NothingToUndo);
            }
            journal.undo()?;
        }
        let current = journal.state()?.current;
        if current >= self.history.len() {
            // The journal knows steps we have not seen (should not happen): give them a
            // fresh revision so nothing stale is reused.
            let revision = self.next_revision;
            self.next_revision += 1;
            self.history.resize(current + 1, revision);
        }
        // What an unseen step did is not known.
        if current >= self.breaks_pdfa.len() {
            self.breaks_pdfa.resize(current + 1, true);
        }
        self.position = current;
        if self.by_reference_since.is_some_and(|since| current < since) {
            self.by_reference_since = None;
        }
        // The steps undone or redone are the ones that led to the positions between.
        let effect = (from.min(current) + 1..=from.max(current))
            .map(|p| self.effects.get(p).copied().unwrap_or(OutlineEffect::Any))
            .fold(OutlineEffect::None, OutlineEffect::and);
        self.changed(effect)
    }

    /// Re-reads page geometry, labels and annotations after a change, and the bookmarks
    /// as far as `effect` says the change can have touched them, and reports the
    /// differences.
    fn changed(&mut self, effect: OutlineEffect) -> Result<DocumentChange> {
        self.lists.clear();
        let pages = page_sizes(&self.doc)?;
        let changed_pages: Vec<(usize, PageSize)> = pages
            .iter()
            .enumerate()
            .filter(|(i, p)| self.pages.get(*i) != Some(p))
            .map(|(i, p)| (i, *p))
            .collect();
        self.pages = pages;
        let page_ids = page_ids(&self.doc)?;
        let page_map = (page_ids != self.page_ids).then(|| {
            let before: HashMap<i32, usize> = self
                .page_ids
                .iter()
                .enumerate()
                .map(|(i, &id)| (id, i))
                .collect();
            page_ids.iter().map(|id| before.get(id).copied()).collect()
        });
        self.page_ids = page_ids;
        let labels = page_labels(&self.doc, self.pages.len())?;
        let labels = if labels != self.labels {
            self.labels = labels.clone();
            Some(labels)
        } else {
            None
        };
        let outline = self.refresh_outline(effect, page_map.as_deref(), &changed_pages)?;
        let annotations = self.reread_annotations()?;
        Ok(DocumentChange {
            state: self.state(),
            changed_pages,
            labels,
            outline,
            created: None,
            page_count: self.pages.len(),
            merge_report: None,
            annotations,
            repairs: None,
            page_map,
            deleted: None,
        })
    }

    /// Brings the bookmarks up to date after a change, and returns them if they differ.
    /// Reading the whole outline takes a tenth of a second in a file with thousands of
    /// bookmarks, so that is done only after a step that can have changed anything in it.
    /// After pages changed places, the bookmarks take their pages' new numbers from
    /// `page_map` (the destinations in the file name the pages themselves, `pages.rs`).
    /// After pages were turned, the bookmarks that lead to `changed_pages` are read again.
    fn refresh_outline(
        &mut self,
        effect: OutlineEffect,
        page_map: Option<&[Option<usize>]>,
        changed_pages: &[(usize, PageSize)],
    ) -> Result<Option<Outline>> {
        // A damaged outline is shown as far as it can be read, and never edited: it does
        // not follow its pages, and its items may not be objects of their own.
        if self.outline.damaged || self.outline_stale {
            return self.reread_outline();
        }
        let changed = match (effect, page_map) {
            (OutlineEffect::None, _) | (OutlineEffect::PageOrder, None) => false,
            (OutlineEffect::PageOrder, Some(map)) => {
                let mut now = Vec::new();
                for (to, from) in map.iter().enumerate() {
                    if let Some(from) = *from {
                        if now.len() <= from {
                            now.resize(from + 1, None);
                        }
                        now[from] = Some(to);
                    }
                }
                let mut outline = self.outline.clone();
                match outline.follow_pages(&now) {
                    Some(true) => {
                        self.outline = outline;
                        true
                    }
                    Some(false) => false,
                    // A page that is gone: not what moving pages does.
                    None => return self.reread_outline(),
                }
            }
            (OutlineEffect::PageGeometry, None) => {
                let pages: HashSet<usize> = changed_pages.iter().map(|(page, _)| *page).collect();
                !pages.is_empty() && self.outline.refresh_on_pages(&self.doc, &pages)?
            }
            (OutlineEffect::PageGeometry, Some(_)) | (OutlineEffect::Any, _) => {
                return self.reread_outline();
            }
        };
        Ok(changed.then(|| self.outline_view()))
    }

    /// Reads the whole outline again, and returns the bookmarks if they differ.
    fn reread_outline(&mut self) -> Result<Option<Outline>> {
        let outline = outline::read_bookmarks(&self.doc)?;
        self.outline_stale = false;
        if outline == self.outline {
            return Ok(None);
        }
        self.outline = outline;
        self.prune_open_states();
        Ok(Some(self.outline_view()))
    }

    /// Reads every page's annotations again and returns the pages whose list differs from
    /// the last read (including pages that are new or now empty).
    fn reread_annotations(&mut self) -> Result<Vec<(usize, Vec<AnnotationInfo>)>> {
        let now = read_annotations(&self.doc)?;
        let changed = now
            .iter()
            .enumerate()
            .filter(|(i, a)| self.annotations.get(*i) != Some(*a))
            .map(|(i, a)| (i, a.clone()))
            .collect();
        self.annotations = now;
        Ok(changed)
    }

    fn save(&mut self, kind: SaveKind, target: Option<PathBuf>) -> Result<SaveResult> {
        let target = target.unwrap_or_else(|| self.path.clone());
        // A signed document is always saved incrementally (section 5.4), so the original
        // signed bytes stay intact.
        let kind = if self.needs_full_save && kind == SaveKind::Incremental {
            SaveKind::Full
        } else if self.flags.signed {
            SaveKind::Incremental
        } else {
            kind
        };
        // Until the saved file is read back, the outline held is not the one written.
        self.outline_stale |= !self.open_states.is_empty();
        self.write_open_states()?;
        let claim = self.remove_broken_pdfa_claim()?;
        let saved = if kind == SaveKind::Optimized {
            self.save_optimized(&target)
        } else {
            save_atomic(&self.doc, kind, Some(&self.file), &target)
        };
        let outcome = match saved {
            Ok(outcome) => outcome,
            Err(e) => {
                // Nothing was written: the document in memory claims again what its file
                // does (best effort; the next save would take the claim out again).
                if let Some(claim) = &claim {
                    let _ = self
                        .rewrite_metadata(|doc| docinfo::restore_metadata(doc, &claim.metadata));
                }
                return Err(e);
            }
        };
        let pdfa_removed = claim.map(|claim| claim.level);
        let revision = self.revision();
        self.saved_revision = revision;
        let mut outline = None;
        let mut annotations = Vec::new();
        // The saved file is now the document's file. Reopen it so later incremental saves
        // append to the right bytes (ADR 0003).
        match load(&target, self.password.as_deref()) {
            Ok(doc) => {
                self.doc = doc;
                self.file = target.clone();
                self.needs_full_save = false;
                self.flags = read_flags(&self.doc)?;
                self.lists.clear();
                self.history = vec![revision];
                self.breaks_pdfa = vec![false];
                self.effects = vec![OutlineEffect::Any];
                // The file holds what the document did: what was known of it still is.
                self.by_reference_since = self.by_reference_since.map(|_| 0);
                self.position = 0;
                // An optimized save numbers the objects anew.
                self.page_ids = page_ids(&self.doc)?;
                outline = self.reread_outline()?;
                annotations = self.reread_annotations()?;
            }
            // The file is saved; keep working on the old in-memory document.
            Err(_) => {
                self.needs_full_save = true;
                if pdfa_removed.is_some() {
                    self.flags.pdfa = None;
                }
            }
        }
        // A file that no longer claims PDF/A has nothing to open for reading only.
        self.read_only &= self.flags.pdfa.is_some();
        self.path = target;
        Ok(SaveResult {
            state: self.state(),
            outcome,
            path: self.path.clone(),
            outline,
            annotations,
            pdfa_removed,
        })
    }

    fn extract(
        &self,
        pages: &[usize],
        target: &Path,
        step: &mut dyn FnMut(ExtractStep) -> bool,
    ) -> Result<MergeReport> {
        if !self.flags.can_copy {
            let name = self.path.file_name().map(|n| n.to_string_lossy());
            return Err(Error::CopyNotPermitted(
                name.unwrap_or_default().into_owned(),
            ));
        }
        let same = |a: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(target)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if same(&self.path) || same(&self.file) {
            return Err(Error::InvalidArgument(
                "pages are extracted into a new file, not into the document's own".into(),
            ));
        }
        let (doc, report) = merge::extract(&self.doc, pages, &mut |done, total| {
            step(ExtractStep::Copying { done, total })
        })?;
        if !step(ExtractStep::Writing) {
            return Err(Error::Cancelled);
        }
        // Copied streams keep their compression and nothing is left unreferenced, so a
        // plain full save is enough.
        save_atomic_checked(&doc, SaveKind::Full, None, target, &mut || {
            if step(ExtractStep::Replacing) {
                Ok(())
            } else {
                Err(Error::Cancelled)
            }
        })?;
        Ok(report)
    }

    /// Takes the PDF/A claim out of the metadata if the changes about to be saved may have
    /// broken it (section 5.4): a file must not say it complies when Lectrix cannot vouch
    /// for what it wrote. Changes that were undone do not count. Like the bookmarks'
    /// expanded states, this is written as an unnamed journal change. Returns what was
    /// removed, with the metadata as it was.
    fn remove_broken_pdfa_claim(&mut self) -> Result<Option<RemovedClaim>> {
        let broken = self
            .breaks_pdfa
            .iter()
            .take(self.position + 1)
            .any(|&breaks| breaks);
        if self.flags.pdfa.is_none() || !broken {
            return Ok(None);
        }
        let Some(metadata) = docinfo::metadata_bytes(&self.doc) else {
            return Ok(None);
        };
        let level = self.rewrite_metadata(docinfo::remove_pdfa_claim)?;
        Ok(level.map(|level| RemovedClaim { level, metadata }))
    }

    /// Runs `write`, which changes the metadata, as an unnamed journal change.
    fn rewrite_metadata<T>(
        &mut self,
        write: impl FnOnce(&mut PdfDocument) -> Result<T>,
    ) -> Result<T> {
        Journal::new(&mut self.doc).begin_implicit()?;
        match write(&mut self.doc) {
            Ok(value) => {
                self.doc.end_operation()?;
                Ok(value)
            }
            Err(e) => {
                self.doc.abandon_operation()?;
                Err(e)
            }
        }
    }

    /// "Save As (optimized)": garbage collection renumbers and merges the objects of the
    /// document it is given, in memory, whether or not the file it writes is kept. If the
    /// replace then failed, this session went on with a document that no longer matched its
    /// file, and its next incremental save lost changes. So the optimizing is done on a
    /// second document: this one written whole (which changes nothing in it) to a file
    /// beside the target, and opened from there.
    fn save_optimized(&self, target: &Path) -> Result<SaveOutcome> {
        let staging = crate::save::temp_path_for(target)?;
        let result = save_atomic(&self.doc, SaveKind::Full, None, &staging).and_then(|_| {
            let copy = load(&staging, self.password.as_deref())?;
            save_atomic(&copy, SaveKind::Optimized, None, target)
        });
        // Best effort: it may not exist if writing failed early.
        let _ = std::fs::remove_file(&staging);
        result
    }

    /// Writes a recovery copy (see [`Session::write_recovery`]).
    fn write_recovery(&mut self, target: &Path, written: Option<u64>) -> Result<RecoveryWrite> {
        let revision = self.revision();
        if revision == self.saved_revision {
            return Ok(RecoveryWrite::Clean);
        }
        if written == Some(revision) {
            return Ok(RecoveryWrite::Unchanged);
        }
        if self.needs_full_save || !self.doc.can_be_saved_incrementally() {
            // A repaired file (or one the in-memory document no longer matches) can only be
            // written whole. A full save leaves the document and its journal as they were:
            // MuPDF records no changes while saving.
            save_atomic(&self.doc, SaveKind::Full, None, target)?;
        } else {
            save_snapshot(&mut self.doc, target)?;
        }
        Ok(RecoveryWrite::Written { revision })
    }

    fn reload(&mut self) -> Result<()> {
        let doc = load(&self.path, self.password.as_deref())?;
        self.doc = doc;
        self.file = self.path.clone();
        self.flags = read_flags(&self.doc)?;
        self.read_only &= self.flags.pdfa.is_some();
        self.pages = page_sizes(&self.doc)?;
        self.page_ids = page_ids(&self.doc)?;
        self.labels = page_labels(&self.doc, self.pages.len())?;
        self.outline = outline::read_bookmarks(&self.doc)?;
        self.outline_stale = false;
        self.annotations = read_annotations(&self.doc)?;
        self.open_states.clear();
        self.lists.clear();
        self.needs_full_save = false;

        let revision = self.next_revision;
        self.next_revision += 1;
        self.history = vec![revision];
        self.breaks_pdfa = vec![false];
        self.effects = vec![OutlineEffect::Any];
        self.by_reference_since = None;
        self.position = 0;
        self.saved_revision = revision;
        Ok(())
    }
}

/// A PDF/A claim taken out of a document for a save.
struct RemovedClaim {
    level: PdfA,
    /// The metadata as it was, to put back if the save fails.
    metadata: Vec<u8>,
}

/// Page sizes from the page dictionaries (no content parsing).
fn page_sizes(doc: &PdfDocument) -> Result<Vec<PageSize>> {
    let count = doc.page_count()?;
    let mut pages = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for i in 0..count {
        let obj = doc.find_page(i)?;
        let g = PageGeometry::new(&read_page_boxes(&obj)?);
        pages.push(PageSize {
            width: g.width as f32,
            height: g.height as f32,
            rotation: u16::try_from(g.rotation).unwrap_or(0),
        });
    }
    Ok(pages)
}

/// Every page's object number (0 for a page that is not an indirect object, in a damaged
/// file).
fn page_ids(doc: &PdfDocument) -> Result<Vec<i32>> {
    let count = doc.page_count()?;
    let mut ids = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for i in 0..count {
        let obj = doc.find_page(i)?;
        ids.push(if obj.is_indirect()? {
            obj.as_indirect()?
        } else {
            0
        });
    }
    Ok(ids)
}

/// The stored label rules and every page's label, or `None` if the document has no labels.
fn page_labels(doc: &PdfDocument, page_count: usize) -> Result<Option<PageLabels>> {
    let rules = labels::read_rules(doc)?;
    if rules.is_empty() {
        return Ok(None);
    }
    let labels = labels::labels_for_pages(&rules, page_count);
    Ok(Some(PageLabels { rules, labels }))
}
