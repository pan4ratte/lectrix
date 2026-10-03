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
//! **Expanded bookmarks** are not edits: expanding or collapsing a bookmark in the panel
//! neither dirties the document nor adds an undo step (Phase 2 review). The actor keeps
//! the panel's states and writes them into the outline when the document is next saved.

use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use mupdf::DisplayList;
use mupdf::pdf::PdfDocument;

use crate::docinfo::{DocumentFlags, read_flags};
use crate::error::{Error, Result};
use crate::ffi::{Journal, open_pdf_shared};
use crate::geometry::{PageGeometry, read_page_boxes};
use crate::labels;
use crate::ops::Operation;
use crate::outline::{self, Outline};
use crate::render::{self, PixelRect, RgbaImage};
use crate::save::{SaveKind, SaveOutcome, save_atomic};
use crate::text::{self, PageText, SearchHit};

/// Size of a page in view space at zoom 1 (points), after rotation and cropping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
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

#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub path: PathBuf,
    pub pages: Vec<PageSize>,
    /// One label per page, or `None` when the document has no `/PageLabels`.
    pub labels: Option<Vec<String>>,
    pub flags: DocumentFlags,
    pub state: DocumentState,
    /// The bookmarks, with the panel's expanded states.
    pub outline: Outline,
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
    pub labels: Option<Option<Vec<String>>>,
    /// The new bookmarks, if they changed (targets move when pages rotate, too).
    pub outline: Option<Outline>,
    /// The id of the object the operation created (the new bookmark).
    pub created: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SaveResult {
    pub state: DocumentState,
    pub outcome: SaveOutcome,
    /// The file the document is now associated with (the Save As target, if any).
    pub path: PathBuf,
}

/// Search results for one page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageHits {
    pub page: usize,
    pub hits: Vec<SearchHit>,
}

type Reply<T> = SyncSender<Result<T>>;

enum Command {
    Info {
        reply: Reply<DocumentInfo>,
    },
    DisplayList {
        page: usize,
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
    Save {
        kind: SaveKind,
        target: Option<PathBuf>,
        reply: Reply<SaveResult>,
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
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let path_buf = path.to_path_buf();
        let password = password.map(str::to_owned);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        thread::Builder::new()
            .name(format!("doc:{name}"))
            .spawn(move || actor(path_buf, password, rx, ready_tx))?;
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
        self.call(|reply| Command::DisplayList {
            page,
            cache: true,
            reply,
        })
    }

    /// Renders `page` (or one tile of it) to RGBA on the calling thread.
    pub fn render_rgba(
        &self,
        page: usize,
        scale: f32,
        tile: Option<PixelRect>,
    ) -> Result<(RgbaImage, u64)> {
        let (list, revision) = self.display_list(page)?;
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
                cache: false,
                reply,
            }) {
                Ok(found) => found,
                // The range may run past the last page; stop there.
                Err(Error::PageOutOfRange(_)) => break,
                Err(e) => return Err(e),
            };
            revision = rev;
            let hits = text::search_page(&list, needle)?;
            if !hits.is_empty() {
                out.push(PageHits { page, hits });
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

    /// Saves to the document's own file (`target: None`) or to `target` (Save As). After
    /// a Save As, the session belongs to `target`.
    pub fn save(&self, kind: SaveKind, target: Option<PathBuf>) -> Result<SaveResult> {
        self.call(|reply| Command::Save {
            kind,
            target,
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

/// Display lists kept per document. Small: a display list for a scanned page holds a
/// reference to its decoded image.
const DISPLAY_LIST_CACHE: usize = 24;

struct Actor {
    doc: PdfDocument,
    path: PathBuf,
    password: Option<String>,
    flags: DocumentFlags,
    pages: Vec<PageSize>,
    labels: Option<Vec<String>>,
    /// The outline as the document has it.
    outline: Outline,
    /// Bookmarks expanded or collapsed in the panel since the last save (id to open).
    open_states: HashMap<u32, bool>,
    /// Revision of the content at each journal position (index 0: as opened or saved).
    history: Vec<u64>,
    position: usize,
    saved_revision: u64,
    next_revision: u64,
    lists: VecDeque<(usize, Arc<DisplayList>)>,
    /// Set when a save succeeded but the saved file could not be reopened: the in-memory
    /// document no longer matches the file, so only a full save is safe.
    needs_full_save: bool,
}

fn actor(
    path: PathBuf,
    password: Option<String>,
    rx: Receiver<Command>,
    ready: SyncSender<Result<DocumentInfo>>,
) {
    let start = Instant::now();
    let mut actor = match Actor::open(path, password) {
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
            Command::DisplayList { page, cache, reply } => {
                let _ = reply.send(actor.display_list(page, cache));
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
            Command::Save {
                kind,
                target,
                reply,
            } => {
                let _ = reply.send(actor.save(kind, target));
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
    Journal::new(&mut doc).enable()?;
    Ok(doc)
}

impl Actor {
    fn open(path: PathBuf, password: Option<String>) -> Result<Actor> {
        let doc = load(&path, password.as_deref())?;
        let flags = read_flags(&doc)?;
        let pages = page_sizes(&doc)?;
        let labels = page_labels(&doc, pages.len())?;
        let outline = outline::read_bookmarks(&doc)?;
        Ok(Actor {
            doc,
            path,
            password,
            flags,
            pages,
            labels,
            outline,
            open_states: HashMap::new(),
            history: vec![0],
            position: 0,
            saved_revision: 0,
            next_revision: 1,
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
            state: self.state(),
            outline: self.outline_view(),
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
    /// add a revision after the signature), or one whose permissions do not allow outline
    /// changes.
    fn write_open_states(&mut self) -> Result<()> {
        if self.open_states.is_empty()
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

    fn display_list(&mut self, page: usize, cache: bool) -> Result<(Arc<DisplayList>, u64)> {
        let revision = self.revision();
        if let Some(i) = self.lists.iter().position(|(p, _)| *p == page) {
            // Move to the back (most recently used).
            if let Some(entry) = self.lists.remove(i) {
                let list = entry.1.clone();
                self.lists.push_back(entry);
                return Ok((list, revision));
            }
        }
        let list = Arc::new(render::display_list(&self.doc, page)?);
        if cache {
            if self.lists.len() >= DISPLAY_LIST_CACHE {
                self.lists.pop_front();
            }
            self.lists.push_back((page, list.clone()));
        }
        Ok((list, revision))
    }

    fn apply(&mut self, op: &Operation) -> Result<DocumentChange> {
        op.validate(self.pages.len())?;
        if !self.flags.can_assemble && op.needs_assemble() {
            return Err(Error::NotPermitted);
        }
        let before = Journal::new(&mut self.doc).state()?.current;
        self.doc.begin_operation(&op.name())?;
        let created = match op.apply(&mut self.doc) {
            Ok(created) => {
                self.doc.end_operation()?;
                created
            }
            Err(e) => {
                self.doc.abandon_operation()?;
                return Err(e);
            }
        };
        let after = Journal::new(&mut self.doc).state()?.current;
        if after != before {
            let revision = self.next_revision;
            self.next_revision += 1;
            self.history.truncate(before + 1);
            self.history.resize(after + 1, revision);
            self.position = after;
        }
        let mut change = self.changed()?;
        change.created = created;
        Ok(change)
    }

    /// Undo (`forward: false`) or redo.
    fn step(&mut self, forward: bool) -> Result<DocumentChange> {
        let mut journal = Journal::new(&mut self.doc);
        let state = journal.state()?;
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
        self.position = current;
        self.changed()
    }

    /// Re-reads page geometry and labels after a change and reports the differences.
    fn changed(&mut self) -> Result<DocumentChange> {
        self.lists.clear();
        let pages = page_sizes(&self.doc)?;
        let changed_pages = pages
            .iter()
            .enumerate()
            .filter(|(i, p)| self.pages.get(*i) != Some(p))
            .map(|(i, p)| (i, *p))
            .collect();
        self.pages = pages;
        let labels = page_labels(&self.doc, self.pages.len())?;
        let labels = if labels != self.labels {
            self.labels = labels.clone();
            Some(labels)
        } else {
            None
        };
        let outline = outline::read_bookmarks(&self.doc)?;
        let outline = if outline != self.outline {
            self.outline = outline;
            self.prune_open_states();
            Some(self.outline_view())
        } else {
            None
        };
        Ok(DocumentChange {
            state: self.state(),
            changed_pages,
            labels,
            outline,
            created: None,
        })
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
        self.write_open_states()?;
        let outcome = save_atomic(&self.doc, kind, Some(&self.path), &target)?;
        let revision = self.revision();
        self.saved_revision = revision;
        // The saved file is now the document's file. Reopen it so later incremental saves
        // append to the right bytes (ADR 0003).
        match load(&target, self.password.as_deref()) {
            Ok(doc) => {
                self.doc = doc;
                self.needs_full_save = false;
                self.flags = read_flags(&self.doc)?;
                self.lists.clear();
                self.history = vec![revision];
                self.position = 0;
                self.outline = outline::read_bookmarks(&self.doc)?;
                self.prune_open_states();
            }
            // The file is saved; keep working on the old in-memory document.
            Err(_) => self.needs_full_save = true,
        }
        self.path = target;
        Ok(SaveResult {
            state: self.state(),
            outcome,
            path: self.path.clone(),
        })
    }

    fn reload(&mut self) -> Result<()> {
        let doc = load(&self.path, self.password.as_deref())?;
        self.doc = doc;
        self.flags = read_flags(&self.doc)?;
        self.pages = page_sizes(&self.doc)?;
        self.labels = page_labels(&self.doc, self.pages.len())?;
        self.outline = outline::read_bookmarks(&self.doc)?;
        self.open_states.clear();
        self.lists.clear();
        self.needs_full_save = false;
        let revision = self.next_revision;
        self.next_revision += 1;
        self.history = vec![revision];
        self.position = 0;
        self.saved_revision = revision;
        Ok(())
    }
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
        });
    }
    Ok(pages)
}

/// Every page's label, or `None` if the document has no labels.
fn page_labels(doc: &PdfDocument, page_count: usize) -> Result<Option<Vec<String>>> {
    let rules = labels::read_rules(doc)?;
    if rules.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        (0..page_count)
            .map(|p| labels::label_for_page(&rules, p))
            .collect(),
    ))
}
