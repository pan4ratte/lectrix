//! IPC payloads. TypeScript types are generated from these by ts-rs into
//! `src/lib/ipc/generated/` (run `cargo test -p lectrix`); never write them by hand.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use pdf_core::session as core;

use crate::annotations::{AnnotationEditInput, NewAnnotationInput, PageAnnotations};
use crate::documents::Documents;

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
    /// The page's `/Rotate` (0, 90, 180 or 270); `width` and `height` are after it.
    pub rotation: u16,
}

impl From<core::PageSize> for PageSize {
    fn from(p: core::PageSize) -> Self {
        PageSize {
            width: p.width,
            height: p.height,
            rotation: p.rotation,
        }
    }
}

/// What the document allows and how it is saved (AGENTS.md section 5.4).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentFlags {
    pub encrypted: bool,
    /// Warn before the first edit: edits show as changes made after signing.
    pub signed: bool,
    /// The file was damaged and repaired on open; the next save is a full save.
    pub repaired: bool,
    /// Pages may be rotated.
    pub can_assemble: bool,
    pub can_annotate: bool,
    /// Text may be copied.
    pub can_copy: bool,
    /// The PDF/A level the file claims, if any: it then opens for reading only, unless
    /// Settings says otherwise, until the user enables editing. A save that may have
    /// broken the standard takes the claim out of the file.
    pub pdfa: Option<PdfaLevel>,
}

/// A PDF/A level as a file's XMP metadata claims it: PDF/A-1b is part 1, conformance B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PdfaLevel {
    pub part: u8,
    /// In upper case; PDF/A-4 may have none.
    pub conformance: Option<char>,
}

impl From<pdf_core::docinfo::DocumentFlags> for DocumentFlags {
    fn from(f: pdf_core::docinfo::DocumentFlags) -> Self {
        DocumentFlags {
            encrypted: f.encrypted,
            signed: f.signed,
            repaired: f.repaired,
            can_assemble: f.can_assemble,
            can_annotate: f.can_annotate,
            can_copy: f.can_copy,
            pdfa: f.pdfa.map(Into::into),
        }
    }
}

impl From<pdf_core::docinfo::PdfA> for PdfaLevel {
    fn from(level: pdf_core::docinfo::PdfA) -> Self {
        PdfaLevel {
            part: level.part,
            conformance: level.conformance,
        }
    }
}

/// Undo position and save state, returned by every command that can change them.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentState {
    /// Content revision: part of every page-image URL, so images refresh on change.
    #[ts(type = "number")]
    pub revision: u64,
    pub dirty: bool,
    pub undo_name: Option<String>,
    pub redo_name: Option<String>,
}

impl From<core::DocumentState> for DocumentState {
    fn from(s: core::DocumentState) -> Self {
        DocumentState {
            revision: s.revision,
            dirty: s.dirty,
            undo_name: s.undo_name,
            redo_name: s.redo_name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ZoomMode {
    FitWidth,
    FitPage,
    Custom,
}

/// How a document asks to be laid out when opened: its catalog's `/PageLayout`, by the
/// names the PDF format gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum PageLayout {
    SinglePage,
    OneColumn,
    TwoColumnLeft,
    TwoColumnRight,
    TwoPageLeft,
    TwoPageRight,
}

impl From<pdf_core::docinfo::PageLayout> for PageLayout {
    fn from(l: pdf_core::docinfo::PageLayout) -> Self {
        use pdf_core::docinfo::PageLayout as Core;
        match l {
            Core::SinglePage => PageLayout::SinglePage,
            Core::OneColumn => PageLayout::OneColumn,
            Core::TwoColumnLeft => PageLayout::TwoColumnLeft,
            Core::TwoColumnRight => PageLayout::TwoColumnRight,
            Core::TwoPageLeft => PageLayout::TwoPageLeft,
            Core::TwoPageRight => PageLayout::TwoPageRight,
        }
    }
}

/// How a document's pages are shown (AGENTS.md section 6.1, visual adjustments): on screen
/// only, never written to the file. Remembered per file with the rest of its view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct PageAdjustments {
    /// Percent, 100 as the pages are.
    pub brightness: u16,
    /// Percent, 100 as the pages are.
    pub contrast: u16,
    /// Dark pages with light text.
    pub invert: bool,
    pub grayscale: bool,
}

impl PageAdjustments {
    /// The least and the most brightness and contrast, percent (mirrored by
    /// `ADJUST_MIN` and `ADJUST_MAX` in src/lib/features/viewer/adjustments.ts).
    pub const MIN: u16 = 50;
    pub const MAX: u16 = 150;

    /// With brightness and contrast inside their range.
    pub fn clamped(self) -> Self {
        Self {
            brightness: self.brightness.clamp(Self::MIN, Self::MAX),
            contrast: self.contrast.clamp(Self::MIN, Self::MAX),
            ..self
        }
    }
}

impl Default for PageAdjustments {
    fn default() -> Self {
        Self {
            brightness: 100,
            contrast: 100,
            invert: false,
            grayscale: false,
        }
    }
}

/// Where the user was in a document, and how its pages were shown; remembered per file in
/// app data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewState {
    /// Physical page index (0-based).
    pub page: u32,
    /// How far down the page the top of the view was, from 0 to 1.
    pub offset: f32,
    /// Zoom factor (1 = 100%).
    pub zoom: f32,
    pub zoom_mode: ZoomMode,
    /// View rotation in degrees (0, 90, 180, 270). Does not change the document.
    pub rotation: u16,
    /// Views stored before there were adjustments have none.
    #[serde(default)]
    pub adjustments: PageAdjustments,
    /// The pages turned as they are shown, on top of the view rotation: a page's index and
    /// its turn in degrees (90, 180 or 270). Never in the document either.
    #[serde(default)]
    pub page_turns: BTreeMap<u32, u16>,
}

impl ViewState {
    /// The view as it is stored: adjustments within their range, and only turns that are
    /// quarter turns.
    pub fn normalized(mut self) -> Self {
        self.adjustments = self.adjustments.clamped();
        self.page_turns
            .retain(|_, degrees| matches!(*degrees, 90 | 180 | 270));
        self
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentInfo {
    pub id: u32,
    /// File name shown in the tab.
    pub name: String,
    /// Full path, shown as the tab's tooltip.
    pub path: String,
    /// Not saved under a name yet (pages extracted into a new document, section 6.7):
    /// `path` is a working copy in Lectrix's own folder, Save asks where to save, and the
    /// document has unsaved changes until then.
    pub untitled: bool,
    pub pages: Vec<PageSize>,
    /// One label per page, or null when the document has no page labels.
    pub labels: Option<Vec<String>>,
    /// The label rules exactly as stored (empty when there are no labels).
    pub label_rules: Vec<LabelRule>,
    pub flags: DocumentFlags,
    /// Open for reading only (a file that claims PDF/A, section 5.4): Rust refuses every
    /// operation until `enable_editing`.
    pub read_only: bool,
    /// How the document asks to be laid out (its `/PageLayout`), if it does.
    pub page_layout: Option<PageLayout>,
    pub state: DocumentState,
    pub outline: Outline,
    /// The annotations of every page that has any.
    pub annotations: Vec<PageAnnotations>,
    /// Where the user left off last time, if this file was opened before.
    pub view: Option<ViewState>,
    /// Time to open the file and read page geometry, in milliseconds.
    pub open_ms: f64,
}

/// The outcome of opening one file.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum OpenResult {
    Opened {
        // Boxed: far larger than the other outcomes.
        document: Box<DocumentInfo>,
    },
    /// The file is already open in a tab; switch to it.
    AlreadyOpen {
        id: u32,
    },
    /// The file is encrypted. Ask for the password and call `unlock_document`.
    NeedsPassword {
        token: u32,
        name: String,
        /// True after a wrong password.
        retry: bool,
    },
    Failed {
        name: String,
        error: AppError,
    },
}

/// What a tab keeps of its own, outside the document, and takes along when it moves to
/// another window (section 8): its page display (section 6.1), and that the warning about
/// editing a signed document was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TabHandoff {
    /// One of the four scroll modes, not `document`.
    pub scroll_mode: ScrollMode,
    pub cover: bool,
    pub signed_warning_accepted: bool,
}

/// A document the asking window shows in a tab, with what its tab brought from the
/// window it was moved out of, if it was.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindowDocument {
    pub document: DocumentInfo,
    pub handoff: Option<TabHandoff>,
}

/// Where a tab moves to (section 8).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum MoveTarget {
    /// A new window beside the one it is in.
    New,
    /// Where the tab was dropped: the Lectrix window under the cursor, else (`or_new`) a
    /// new window there. Without `or_new` and with no window there, nothing moves: the
    /// tab is its window's only one, so a new window would be the same window elsewhere.
    Cursor { or_new: bool },
    /// Another window that is open.
    Window { label: String },
}

/// Another of Lectrix's windows, for "Move to window": named by its title.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindowEntry {
    pub label: String,
    pub title: String,
}

/// Emitted as `document-saved` to the window that shows a document another window saved
/// (the Combine view's "Save and combine").
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentSaved {
    pub id: u32,
    pub result: SaveResult,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangedPage {
    pub index: u32,
    pub size: PageSize,
}

/// What a mutating command changed (AGENTS.md section 3).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentChange {
    pub state: DocumentState,
    pub changed_pages: Vec<ChangedPage>,
    /// True when `labels` and `label_rules` hold new labels (`labels` may be null:
    /// labels removed).
    pub labels_changed: bool,
    pub labels: Option<Vec<String>>,
    pub label_rules: Vec<LabelRule>,
    /// The new bookmarks, when they changed.
    pub outline: Option<Outline>,
    /// The id of what the operation created (the new bookmark).
    pub created: Option<u32>,
    /// The number of pages: it changes when pages are inserted (or that is undone). Then
    /// `changed_pages` lists every index whose size differs from the page that was there
    /// before, including every new index.
    pub page_count: u32,
    /// What inserting pages did (renamed names, links left out).
    pub merge_report: Option<MergeReport>,
    /// Pages whose annotations changed, with all of their annotations now (a page with
    /// none left has an empty list).
    pub annotations: Vec<PageAnnotations>,
    /// Set when the pages are no longer the same ones in the same order (moved, deleted,
    /// inserted, or any of that undone): for each page now, the index it had before, or
    /// null for a page that was not there. What the frontend keeps by page index (the
    /// current page, the history of jumps) follows its page with this.
    pub page_map: Option<Vec<Option<u32>>>,
    /// What deleting pages took out with them.
    pub deleted: Option<DeleteReport>,
}

/// What deleting pages took out with them, for telling the user.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeleteReport {
    pub pages: u32,
    /// Links on other pages that led to the deleted ones.
    pub links: u32,
    /// Bookmarks that led to them (ones with bookmarks inside stay as headings).
    pub bookmarks: u32,
    /// Form fields that were on them.
    pub fields: u32,
}

impl From<pdf_core::pages::DeleteReport> for DeleteReport {
    fn from(r: pdf_core::pages::DeleteReport) -> Self {
        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        DeleteReport {
            pages: n(r.pages),
            links: n(r.links),
            bookmarks: n(r.bookmarks),
            fields: n(r.fields),
        }
    }
}

impl From<core::DocumentChange> for DocumentChange {
    fn from(c: core::DocumentChange) -> Self {
        let (labels_changed, labels) = match c.labels {
            Some(labels) => (true, labels),
            None => (false, None),
        };
        let (labels, label_rules) = split_labels(labels);
        let outline = c.outline.map(Outline::from);
        let created = c.created;
        let page_count = u32::try_from(c.page_count).unwrap_or(u32::MAX);
        let merge_report = c.merge_report.map(MergeReport::from);
        let page_map = c.page_map.map(|map| {
            map.into_iter()
                .map(|before| before.and_then(|i| u32::try_from(i).ok()))
                .collect()
        });
        DocumentChange {
            state: c.state.into(),
            changed_pages: c
                .changed_pages
                .into_iter()
                .map(|(index, size)| ChangedPage {
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                    size: size.into(),
                })
                .collect(),
            labels_changed,
            labels,
            label_rules,
            outline,
            created,
            page_count,
            merge_report,
            annotations: crate::annotations::changed(c.annotations),
            page_map,
            deleted: c.deleted.map(DeleteReport::from),
        }
    }
}

/// What combining files or inserting pages did, for telling the user (section 6.4).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeReport {
    pub pages: u32,
    /// Named destinations renamed because another file already used the name.
    pub renamed_destinations: u32,
    /// Form fields renamed for the same reason.
    pub renamed_fields: u32,
    /// Attached files renamed for the same reason.
    pub renamed_attachments: u32,
    /// Links left out because the page they lead to was not included.
    pub dropped_links: u32,
    /// Bookmarks left out for the same reason.
    pub dropped_bookmarks: u32,
    /// The document's bookmarks are damaged, so the inserted file's were not added.
    pub bookmarks_skipped: bool,
}

impl From<pdf_core::merge::MergeReport> for MergeReport {
    fn from(r: pdf_core::merge::MergeReport) -> Self {
        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        MergeReport {
            pages: n(r.pages),
            renamed_destinations: n(r.renamed_destinations),
            renamed_fields: n(r.renamed_fields),
            renamed_attachments: n(r.renamed_attachments),
            dropped_links: n(r.dropped_links),
            dropped_bookmarks: n(r.dropped_bookmarks),
            bookmarks_skipped: r.bookmarks_skipped,
        }
    }
}

/// What happens to the sources' bookmarks when combining or inserting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BookmarkMode {
    /// Each file's bookmarks under a new bookmark named after the file.
    Nest,
    /// All bookmarks at the top level.
    Flat,
    /// No bookmarks.
    Drop,
}

impl From<BookmarkMode> for pdf_core::merge::BookmarkMode {
    fn from(m: BookmarkMode) -> Self {
        use pdf_core::merge::BookmarkMode as M;
        match m {
            BookmarkMode::Nest => M::NestUnderSource,
            BookmarkMode::Flat => M::Flat,
            BookmarkMode::Drop => M::Drop,
        }
    }
}

/// A web page the About dialog opens in the default browser. The addresses are Rust's
/// own (`WebPage::url`), so the webview can't have any other page opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WebPage {
    /// Lectrix's source code (AGPL-3.0 section 6).
    Source,
    /// The author's GitHub profile.
    Author,
}

impl WebPage {
    pub fn url(self) -> &'static str {
        match self {
            WebPage::Source => "https://github.com/pan4ratte/lectrix",
            WebPage::Author => "https://github.com/pan4ratte",
        }
    }
}

/// Page labels of a combined file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LabelMode {
    /// Every page keeps the label it had in its file.
    Keep,
    /// 1, 2, 3… over the whole result.
    Continuous,
    /// No page labels.
    None,
}

/// Page labels of inserted pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum InsertLabelMode {
    /// The inserted pages keep their own labels (if their file has labels).
    Keep,
    /// The inserted pages continue the numbering around them.
    Follow,
}

/// One page of a combined file: page `page` of source number `source` (an index into
/// `MergeRequest.sources`), turned by `rotation` degrees (a multiple of 90).
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergePage {
    pub source: u32,
    pub page: u32,
    pub rotation: i32,
}

/// "Combine": the sources (ids of documents opened with `open_merge_sources`), the pages
/// of the result in order, and the options.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeRequest {
    pub sources: Vec<u32>,
    pub pages: Vec<MergePage>,
    pub bookmarks: BookmarkMode,
    pub labels: LabelMode,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeStage {
    Copying,
    Writing,
}

/// Sent on `execute_merge`'s channel while combining.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeProgress {
    pub stage: MergeStage,
    /// Pages copied so far, of `total`.
    pub done: u32,
    pub total: u32,
}

/// A combined file, written and opened in a tab; or pages extracted into a new document,
/// opened in a tab and not saved yet.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeOutcome {
    pub opened: OpenResult,
    pub report: MergeReport,
    /// The new file's name (for extracted pages, the name the new document starts with).
    pub name: String,
}

/// A source that is also open in a tab with unsaved changes: combining reads the file as
/// saved, without them.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UnsavedSource {
    /// The tab's document id.
    pub tab: u32,
    pub name: String,
}

/// Checks before combining (`plan_merge`).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergePlan {
    pub unsaved: Vec<UnsavedSource>,
    /// Sources whose files were moved or deleted since they were added.
    pub missing: Vec<String>,
}

/// How a page's label is numbered (AGENTS.md section 6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LabelStyle {
    /// The prefix only.
    None,
    /// 1, 2, 3
    Decimal,
    /// i, ii, iii
    LowerRoman,
    /// I, II, III
    UpperRoman,
    /// a, b, c … z, aa, bb
    LowerLetters,
    /// A, B, C … Z, AA, BB
    UpperLetters,
}

/// A page label rule: from `start_page` until the next rule, pages are labeled `prefix`
/// followed by `first_number`, `first_number + 1`… in `style`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LabelRule {
    /// Physical page index (0-based).
    pub start_page: u32,
    pub style: LabelStyle,
    pub prefix: String,
    /// 1 or higher.
    pub first_number: u32,
}

impl From<pdf_core::labels::LabelRule> for LabelRule {
    fn from(r: pdf_core::labels::LabelRule) -> Self {
        use pdf_core::labels::LabelStyle as S;
        LabelRule {
            start_page: u32::try_from(r.start_page).unwrap_or(u32::MAX),
            style: match r.style {
                S::None => LabelStyle::None,
                S::Decimal => LabelStyle::Decimal,
                S::LowerRoman => LabelStyle::LowerRoman,
                S::UpperRoman => LabelStyle::UpperRoman,
                S::LowerLetters => LabelStyle::LowerLetters,
                S::UpperLetters => LabelStyle::UpperLetters,
            },
            prefix: r.prefix,
            first_number: r.first_number,
        }
    }
}

impl From<LabelRule> for pdf_core::labels::LabelRule {
    fn from(r: LabelRule) -> Self {
        use pdf_core::labels::LabelStyle as S;
        pdf_core::labels::LabelRule {
            start_page: r.start_page as usize,
            style: match r.style {
                LabelStyle::None => S::None,
                LabelStyle::Decimal => S::Decimal,
                LabelStyle::LowerRoman => S::LowerRoman,
                LabelStyle::UpperRoman => S::UpperRoman,
                LabelStyle::LowerLetters => S::LowerLetters,
                LabelStyle::UpperLetters => S::UpperLetters,
            },
            prefix: r.prefix,
            first_number: r.first_number,
        }
    }
}

/// The per-page labels and the rules, as sent to the frontend.
pub fn split_labels(
    labels: Option<pdf_core::session::PageLabels>,
) -> (Option<Vec<String>>, Vec<LabelRule>) {
    match labels {
        Some(l) => (
            Some(l.labels),
            l.rules.into_iter().map(LabelRule::from).collect(),
        ),
        None => (None, Vec::new()),
    }
}

/// The document's bookmarks.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Outline {
    pub items: Vec<Bookmark>,
    /// The outline is malformed: shown as far as it can be read, but not editable.
    pub damaged: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Bookmark {
    /// Stable while the document is open; 0 only in damaged outlines.
    pub id: u32,
    pub title: String,
    /// Expanded in the panel.
    pub open: bool,
    pub target: BookmarkTarget,
    pub bold: bool,
    pub italic: bool,
    /// The bookmark's own color as #rrggbb, if it has one.
    pub color: Option<String>,
    pub children: Vec<Bookmark>,
}

/// What a bookmark, or a link on a page, does when clicked.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum BookmarkTarget {
    /// A heading without a destination.
    None,
    /// A place in this document: the point to show at the top-left of the view, in page
    /// points (view space at zoom 1); null keeps the current position on that axis.
    Page {
        page: u32,
        x: Option<f32>,
        y: Option<f32>,
        /// The named destination it goes through, if any.
        named: Option<String>,
    },
    /// The destination does not lead to a page of this document.
    Broken { named: Option<String> },
    /// A web link. Lectrix shows it and offers to copy it; it never opens it.
    Uri { uri: String },
    /// A link to another file.
    File { file: String },
    /// An action Lectrix does not run (JavaScript, named actions...).
    Action { action: String },
}

impl From<pdf_core::outline::Outline> for Outline {
    fn from(o: pdf_core::outline::Outline) -> Self {
        Outline {
            items: o.items.into_iter().map(Bookmark::from).collect(),
            damaged: o.damaged,
        }
    }
}

impl From<pdf_core::outline::Target> for BookmarkTarget {
    fn from(target: pdf_core::outline::Target) -> Self {
        use pdf_core::outline::Target as T;
        match target {
            T::None => BookmarkTarget::None,
            T::Page { page, x, y, named } => BookmarkTarget::Page {
                page: u32::try_from(page).unwrap_or(u32::MAX),
                x,
                y,
                named,
            },
            T::Broken { named } => BookmarkTarget::Broken { named },
            T::Uri(uri) => BookmarkTarget::Uri { uri },
            T::File(file) => BookmarkTarget::File { file },
            T::Action(action) => BookmarkTarget::Action { action },
        }
    }
}

impl From<pdf_core::outline::Bookmark> for Bookmark {
    fn from(b: pdf_core::outline::Bookmark) -> Self {
        let color = b.color.map(|[r, g, b]| {
            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
        });
        Bookmark {
            id: b.id,
            title: b.title,
            open: b.open,
            target: b.target.into(),
            bold: b.bold,
            italic: b.italic,
            color,
            children: b.children.into_iter().map(Bookmark::from).collect(),
        }
    }
}

/// A place to point a bookmark at: the point at the top-left of the view, in page points
/// (view space at zoom 1).
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewDest {
    pub page: u32,
    pub x: f64,
    pub y: f64,
}

impl From<ViewDest> for pdf_core::outline::ViewDest {
    fn from(d: ViewDest) -> Self {
        pdf_core::outline::ViewDest {
            page: d.page as usize,
            x: d.x,
            y: d.y,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SaveResult {
    pub state: DocumentState,
    /// The document's file name and path after saving (they change with Save As).
    pub name: String,
    pub path: String,
    /// An incremental save was not possible (the file had been repaired), so the whole
    /// file was rewritten.
    pub fell_back_to_full: bool,
    /// The bookmarks, when their ids changed (Save As (optimized) renumbers objects).
    pub outline: Option<Outline>,
    /// Pages whose annotations read differently after saving (renumbered objects).
    pub annotations: Vec<PageAnnotations>,
    /// The PDF/A level the file claimed until this save took the claim out of it: what
    /// was saved (annotations, inserted pages) may not comply (section 5.4).
    pub pdfa_removed: Option<PdfaLevel>,
}

/// A change to a document, sent from the frontend.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum OperationInput {
    /// Turn pages by `degrees` (multiple of 90, positive is clockwise).
    RotatePages {
        pages: Vec<u32>,
        degrees: i32,
    },
    /// Move pages (they keep their order) into the gap `to` of the current order: 0 is
    /// before the first page, the page count after the last. Page labels stay with the
    /// positions.
    MovePages {
        pages: Vec<u32>,
        to: u32,
    },
    /// Delete pages, with the links, bookmarks and form fields that lead to them or were
    /// on them. At least one page stays.
    DeletePages {
        pages: Vec<u32>,
    },
    /// Add a bookmark as child number `index` of `parent` (null: top level).
    AddBookmark {
        parent: Option<u32>,
        index: u32,
        title: String,
        dest: ViewDest,
    },
    RenameBookmark {
        id: u32,
        title: String,
    },
    /// Move a bookmark to child number `index` of `parent`, counted after it has left its
    /// old place.
    MoveBookmark {
        id: u32,
        parent: Option<u32>,
        index: u32,
    },
    /// Delete a bookmark with its children.
    DeleteBookmark {
        id: u32,
    },
    /// Point a bookmark at a new place ("Set destination to current view").
    SetBookmarkDestination {
        id: u32,
        dest: ViewDest,
    },
    /// Replace the page labels (a rule at the first page is added if missing); an empty
    /// list removes them.
    SetPageLabels {
        rules: Vec<LabelRule>,
    },
    /// Insert pages of a file opened with `open_insert_sources` (its document id) before
    /// page `at` (the page count appends). `pages` (0-based) empty inserts all of them.
    InsertPages {
        source: u32,
        pages: Vec<u32>,
        at: u32,
        bookmarks: BookmarkMode,
        labels: InsertLabelMode,
    },
    /// Create annotations (several: text selected across pages) as one undo step; their
    /// author is the name from Settings.
    AddAnnotation {
        annotations: Vec<NewAnnotationInput>,
    },
    /// Change annotation `id` on `page`.
    UpdateAnnotation {
        page: u32,
        id: u32,
        edit: AnnotationEditInput,
    },
    /// Delete annotation `id` on `page`, with its popup and replies.
    DeleteAnnotation {
        page: u32,
        id: u32,
    },
    /// Reply to annotation `parent` on `page`; the author is the name from Settings.
    AddReply {
        page: u32,
        parent: u32,
        text: String,
    },
}

impl OperationInput {
    /// The operation, with document ids resolved to files and new annotations signed by
    /// `author`.
    pub fn into_operation(
        self,
        documents: &Documents,
        author: &str,
    ) -> Result<pdf_core::ops::Operation, AppError> {
        use pdf_core::ops::Operation as Op;
        Ok(match self {
            OperationInput::InsertPages {
                source,
                pages,
                at,
                bookmarks,
                labels,
            } => {
                let (path, password) = documents.source(source)?;
                Op::InsertPages {
                    source: pdf_core::ops::InsertSource {
                        path,
                        password,
                        pages: pages.into_iter().map(|p| p as usize).collect(),
                    },
                    at: at as usize,
                    options: pdf_core::merge::InsertOptions {
                        bookmarks: bookmarks.into(),
                        labels: match labels {
                            InsertLabelMode::Keep => pdf_core::merge::InsertLabels::KeepSource,
                            InsertLabelMode::Follow => {
                                pdf_core::merge::InsertLabels::FollowDocument
                            }
                        },
                    },
                }
            }
            OperationInput::AddAnnotation { annotations } => {
                let mut all = annotations
                    .into_iter()
                    .map(|a| a.into_core(author))
                    .collect::<Result<Vec<_>, _>>()?;
                match all.len() {
                    1 => Op::AddAnnotation {
                        annotation: all.remove(0),
                    },
                    _ => Op::AddAnnotations { annotations: all },
                }
            }
            OperationInput::AddReply { page, parent, text } => Op::AddReply {
                page: page as usize,
                parent,
                text,
                author: author.to_owned(),
            },
            other => other.into_edit()?,
        })
    }

    fn into_edit(self) -> Result<pdf_core::ops::Operation, AppError> {
        use pdf_core::ops::Operation as Op;
        Ok(match self {
            OperationInput::RotatePages { pages, degrees } => Op::RotatePages {
                pages: pages.into_iter().map(|p| p as usize).collect(),
                degrees,
            },
            OperationInput::MovePages { pages, to } => Op::MovePages {
                pages: pages.into_iter().map(|p| p as usize).collect(),
                to: to as usize,
            },
            OperationInput::DeletePages { pages } => Op::DeletePages {
                pages: pages.into_iter().map(|p| p as usize).collect(),
            },
            OperationInput::AddBookmark {
                parent,
                index,
                title,
                dest,
            } => Op::AddBookmark {
                parent,
                index: index as usize,
                title,
                dest: dest.into(),
            },
            OperationInput::RenameBookmark { id, title } => Op::RenameBookmark { id, title },
            OperationInput::MoveBookmark { id, parent, index } => Op::MoveBookmark {
                id,
                parent,
                index: index as usize,
            },
            OperationInput::DeleteBookmark { id } => Op::DeleteBookmark { id },
            OperationInput::SetBookmarkDestination { id, dest } => Op::SetBookmarkDestination {
                id,
                dest: dest.into(),
            },
            OperationInput::SetPageLabels { rules } => Op::SetPageLabels {
                rules: rules.into_iter().map(Into::into).collect(),
            },
            OperationInput::UpdateAnnotation { page, id, edit } => Op::UpdateAnnotation {
                page: page as usize,
                id,
                edit: edit.into_core()?,
            },
            OperationInput::DeleteAnnotation { page, id } => Op::DeleteAnnotation {
                page: page as usize,
                id,
            },
            // Resolved by `into_operation`.
            OperationInput::InsertPages { .. }
            | OperationInput::AddAnnotation { .. }
            | OperationInput::AddReply { .. } => {
                return Err(AppError::bad_state());
            }
        })
    }
}

/// One line of text with one box per character (`boxes` holds 4 numbers per character:
/// x0, y0, x1, y1 in points, view space at zoom 1).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TextLine {
    pub text: String,
    pub boxes: Vec<f32>,
    pub bbox: [f32; 4],
    pub block: u32,
    pub vertical: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageText {
    pub page: u32,
    #[ts(type = "number")]
    pub revision: u64,
    pub lines: Vec<TextLine>,
    /// The page's links, which the viewer follows when clicked (section 6.1). In the order
    /// they are drawn: a later one lies over an earlier one.
    pub links: Vec<PageLink>,
}

/// Where a web address that the user asked to open is in the document (`open_link`): Rust
/// opens only an address the document carries there.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum LinkPlace {
    /// A link on this page (0-based).
    Page { page: u32 },
    /// The bookmark with this id.
    Bookmark { bookmark: u32 },
}

/// A link on a page: where it is, and what it does when clicked.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageLink {
    /// `[x0, y0, x1, y1]` in page points (view space at zoom 1), as text boxes are.
    pub rect: [f32; 4],
    pub target: BookmarkTarget,
}

impl PageText {
    pub fn new(
        page: usize,
        revision: u64,
        text: pdf_core::text::PageText,
        links: Vec<pdf_core::links::PageLink>,
    ) -> Self {
        PageText {
            page: u32::try_from(page).unwrap_or(u32::MAX),
            revision,
            links: links
                .into_iter()
                .map(|l| PageLink {
                    rect: l.rect,
                    target: l.target.into(),
                })
                .collect(),
            lines: text
                .lines
                .into_iter()
                .map(|l| TextLine {
                    text: l.text,
                    boxes: l.boxes.into_iter().flatten().collect(),
                    bbox: l.bbox,
                    block: l.block,
                    vertical: l.vertical,
                })
                .collect(),
        }
    }
}

/// Search hits on one page. Each hit is a list of quads (8 numbers each: upper-left,
/// upper-right, lower-left, lower-right corners, view space at zoom 1).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageHits {
    pub page: u32,
    pub hits: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchChunk {
    #[ts(type = "number")]
    pub revision: u64,
    pub pages: Vec<PageHits>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecentFile {
    /// Position in the list; pass it to `open_recent`.
    pub index: u32,
    pub name: String,
    pub folder: String,
    /// False when the file is no longer where it was.
    pub exists: bool,
    /// When it was last opened, in milliseconds since the Unix epoch.
    #[ts(type = "number")]
    pub opened_at: u64,
    /// Its size in bytes; `None` when it is not found.
    #[ts(type = "number | null")]
    pub size: Option<u64>,
}

/// Unsaved changes a crash left behind, offered for restoring (section 7).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveredDocument {
    /// Identifies the recovery copy; pass it to `restore_recovered` or `discard_recovered`.
    pub slot: String,
    pub name: String,
    pub folder: String,
    /// When the copy was written, in milliseconds since the Unix epoch.
    pub saved_at: f64,
    /// False when the document's file is no longer where it was.
    pub exists: bool,
}

/// How page images travel to the webview (ADR 0004).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageFormat {
    /// Raw RGBA drawn with putImageData (AGENTS.md section 3; kept for measurements).
    Rgba,
    /// PNG decoded by the webview: the default (ADR 0004).
    Png,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartupInfo {
    /// The window Lectrix started with, not one made for a tab moved out of another: it
    /// offers crash recovery, opens the startup files and says what an update brought.
    pub first_window: bool,
    /// Milliseconds from the start of `main` to the frontend's first mount.
    pub main_to_ready_ms: f64,
    /// Set by the LECTRIX_PERF environment variable: the frontend runs its scripted
    /// performance measurements (tests/perf/measure.ps1).
    pub perf_mode: bool,
    /// LECTRIX_PERF=scroll: only the scrolling measurements, without the image format
    /// comparison (which pushes large images through a canvas first).
    pub perf_scroll_only: bool,
    /// LECTRIX_PERF=watch: no scripted scrolling; records frames and blank pages while
    /// real input scrolls the document (tests/perf/wheel-linux.py).
    pub perf_watch: bool,
    /// Page image format: PNG (ADR 0004); LECTRIX_IMAGE_FORMAT=rgba switches to raw RGBA.
    pub image_format: ImageFormat,
    /// The side panes as the user left them.
    pub panes: PaneLayout,
    /// The first start since Lectrix was updated: the frontend offers What's new.
    pub updated: bool,
    /// Show the Debug menu, which rehearses notifications and notices: in development
    /// builds, or with LECTRIX_DEBUG=1 (LECTRIX_DEBUG=0 hides it).
    pub debug_menu: bool,
}

/// The side panes (section 8): which are open, how wide they are, which panels each holds
/// in which order, the panel each shows, and the size of page thumbnails; also how the start
/// screen shows recent files. Remembered in app data. Widths are CSS pixels;
/// the frontend keeps them within its limits, and puts each panel in exactly one pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct PaneLayout {
    pub left_open: bool,
    pub left_width: u32,
    pub right_open: bool,
    pub right_width: u32,
    pub left_panels: Vec<PanelId>,
    pub right_panels: Vec<PanelId>,
    /// The panel each pane shows; none means its first one.
    pub left_active: Option<PanelId>,
    pub right_active: Option<PanelId>,
    /// The width of page thumbnails in the Pages panel, CSS pixels (at most the panel's).
    pub thumbnail_width: u32,
    /// Page thumbnails fill the Pages panel's width, and follow it as it changes.
    pub thumbnails_fit: bool,
    /// Page thumbnails in a grid of as many columns as fit, rather than one column.
    pub thumbnails_grid: bool,
    /// The start screen shows recent files as a grid of previews, rather than a table.
    pub recent_files_grid: bool,
}

impl Default for PaneLayout {
    fn default() -> Self {
        PaneLayout {
            left_open: true,
            left_width: 240,
            right_open: false,
            right_width: 300,
            left_panels: vec![PanelId::Pages, PanelId::Bookmarks],
            right_panels: vec![PanelId::Annotations],
            left_active: None,
            right_active: None,
            thumbnail_width: 112,
            thumbnails_fit: false,
            thumbnails_grid: false,
            recent_files_grid: false,
        }
    }
}

/// A panel the side panes hold; the user moves them between the two panes. (Page labels
/// had one up to 1.1; they are edited in the page organizing tool now, section 6.7.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PanelId {
    Pages,
    Bookmarks,
    Annotations,
}

/// Light or dark: following the system, or forced (Settings, section 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// How pages are laid out when a document opens (Settings, section 6.1): as the document
/// asks, or one of the four scroll modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ScrollMode {
    /// The document's own `/PageLayout`; single page, continuous when it has none.
    Document,
    SinglePage,
    #[default]
    SinglePageContinuous,
    TwoPage,
    TwoPageContinuous,
}

/// How the annotation toolbar looks (Settings, section 6.6): a panel docked beside the
/// page canvas (the default), or floating over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToolbarStyle {
    Floating,
    #[default]
    Panel,
}

/// Which edge of the page canvas the floating annotation toolbar sits on (Settings,
/// section 6.6). At a side, its tools are stacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToolbarPosition {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

/// Where the docked annotation toolbar is (Settings, section 6.6): in the bar above the
/// pages, or a bar of its own at a side of them, its tools stacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToolbarDock {
    #[default]
    Top,
    Left,
    Right,
}

/// When the annotation toolbar is shown: always, or while the pointer is near its edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToolbarVisibility {
    #[default]
    Always,
    OnHover,
}

/// A button of the bar shown over selected text (section 6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum QuickTool {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    /// A highlight with the inspector open on its note.
    HighlightNote,
    Copy,
    Bookmark,
}

impl QuickTool {
    /// What the bar shows until the user picks.
    pub const DEFAULT: [QuickTool; 5] = [
        QuickTool::Highlight,
        QuickTool::Underline,
        QuickTool::StrikeOut,
        QuickTool::HighlightNote,
        QuickTool::Copy,
    ];
}

/// The Settings dialog's values.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    /// The author name new annotations get.
    pub author: String,
    /// What `author` is when the user has not set one (the Windows user name).
    pub default_author: String,
    pub appearance: Appearance,
    /// How pages are laid out when a document opens.
    pub scroll_mode: ScrollMode,
    pub toolbar_style: ToolbarStyle,
    pub toolbar_dock: ToolbarDock,
    pub toolbar_position: ToolbarPosition,
    pub toolbar_visibility: ToolbarVisibility,
    /// The buttons of the bar over selected text; empty: no bar.
    pub quick_tools: Vec<QuickTool>,
    /// Whether Lectrix looks for a new release once a day (ADR 0011).
    pub check_for_updates: bool,
    /// Whether zooming in or out a step glides instead of jumping (section 6.1).
    pub smooth_zoom: bool,
    /// Whether picking an annotation in the list scrolls to it smoothly (section 6.5).
    pub smooth_annotation_scroll: bool,
    /// Whether the Previous page and Next page buttons glide to the page (section 6.1).
    pub smooth_page_turn: bool,
    /// Whether following a bookmark or a link in the text glides to its place (sections
    /// 6.1 and 6.2).
    pub smooth_bookmark_scroll: bool,
    /// How long the pointer rests on an annotation before its comment shows, in ms.
    pub tooltip_delay_ms: u32,
    /// Whether marking text up selects the new annotation and opens the inspector on its
    /// comment (section 6.5); otherwise nothing is selected.
    pub open_comment_after_markup: bool,
    /// Whether new annotations get the colour and opacity last given to one of their type
    /// (section 6.5); otherwise the defaults, and the toolbar's choice until Lectrix closes.
    pub remember_annotation_style: bool,
    /// Whether selected text shows a handle at its start and at its end, to drag it from
    /// (section 6.1); its sides can be dragged either way.
    pub selection_handles: bool,
    /// Whether a file that claims PDF/A opens for reading only, until the user enables
    /// editing (section 5.4); otherwise it opens ready to edit, as any file.
    pub pdfa_read_only: bool,
    /// Whether a link on a page is tinted while the pointer is on it (section 6.1).
    pub highlight_links: bool,
    /// Whether a tooltip on the link under the pointer says where it leads (section 6.1).
    pub link_previews: bool,
    /// Whether a colour button opens its panel when the pointer rests on it (section
    /// 6.5); otherwise only when it is clicked.
    pub color_panel_on_hover: bool,
    /// The colour panel's presets that differ from the defaults (section 6.5): a preset's
    /// place in the panel, from 0, and its colour ("#rrggbb"). The frontend knows the
    /// defaults (`PRESET_COLORS` in `tools.ts`).
    pub preset_colors: BTreeMap<u8, String>,
    /// Keyboard shortcuts that differ from the defaults (section 6.6): a command's name
    /// and its keys ("Ctrl+Shift+S"); no keys, no shortcut. The frontend knows the
    /// commands and their defaults (`src/lib/shortcuts.ts`).
    pub shortcuts: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsInput {
    /// Empty: use the default (the Windows user name).
    pub author: String,
    pub appearance: Appearance,
    pub scroll_mode: ScrollMode,
    pub toolbar_style: ToolbarStyle,
    pub toolbar_dock: ToolbarDock,
    pub toolbar_position: ToolbarPosition,
    pub toolbar_visibility: ToolbarVisibility,
    pub quick_tools: Vec<QuickTool>,
    pub check_for_updates: bool,
    pub smooth_zoom: bool,
    pub smooth_annotation_scroll: bool,
    pub smooth_page_turn: bool,
    pub smooth_bookmark_scroll: bool,
    pub tooltip_delay_ms: u32,
    pub open_comment_after_markup: bool,
    pub remember_annotation_style: bool,
    pub selection_handles: bool,
    pub pdfa_read_only: bool,
    pub highlight_links: bool,
    pub link_previews: bool,
    pub color_panel_on_hover: bool,
    pub preset_colors: BTreeMap<u8, String>,
    pub shortcuts: BTreeMap<String, Vec<String>>,
}

/// A newer release of Lectrix, found when it started (ADR 0011).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// The latest versions' changes, as CHANGELOG.md has them (latest.json's notes), for
    /// What's new before updating. Releases before 1.1.0 carry only "Lectrix <version>".
    pub notes: Option<String>,
}

/// How far downloading an update has got.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateProgress {
    pub downloaded: u32,
    /// None when the server does not say how big the file is.
    pub total: Option<u32>,
}

/// A downloaded update, checked against Lectrix's signing key.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateReady {
    /// True when the new version is already installed and starts with the next launch
    /// (macOS, Linux). False on Windows, where the installer runs when Lectrix restarts or
    /// closes, because it cannot replace the running app.
    pub installed: bool,
}

/// Emitted as `file-changed` when an open document's file changes on disk.

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileChangedEvent {
    pub id: u32,
    /// False when the file was moved or deleted.
    pub exists: bool,
}

/// Emitted as `touchpad-pinch` where the webview would apply a touchpad pinch to the whole
/// app instead of handing it to the page (WebKitGTK; `Platform::route_touchpad_pinch`).
/// The frontend zooms the document with it, as with Ctrl+wheel.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
// Only Linux emits it; the type is exported for the frontend everywhere.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct TouchpadPinch {
    /// A new pinch: `scale` counts from 1 again.
    pub begin: bool,
    /// How far the fingers have spread since the pinch began (2 = twice as far apart).
    pub scale: f64,
    /// Where the fingers are, in the webview's CSS pixels.
    pub x: f64,
    pub y: f64,
}

/// Errors shown to the user: plain language plus a suggested next step (section 8).
#[derive(Debug, Clone, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[error("{message}")]
pub struct AppError {
    pub message: String,
    pub suggestion: Option<String>,
    /// A machine-readable reason the UI can act on (for example offering Save As).
    pub code: ErrorCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorCode {
    General,
    /// The target file is locked by another program.
    FileLocked,
    NotPermitted,
    NothingToUndo,
    NothingToRedo,
    DocumentClosed,
    /// The user stopped a long operation; nothing to show.
    Cancelled,
}

impl AppError {
    pub fn new(message: impl Into<String>, suggestion: Option<&str>) -> Self {
        AppError {
            message: message.into(),
            suggestion: suggestion.map(str::to_owned),
            code: ErrorCode::General,
        }
    }

    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.code = code;
        self
    }

    pub fn document_closed() -> Self {
        AppError::new(
            "This document is no longer open.",
            Some("Open it again from File > Open."),
        )
        .with_code(ErrorCode::DocumentClosed)
    }

    pub fn bad_state() -> Self {
        AppError::new("The app is in a bad state.", Some("Restart Lectrix."))
    }
}

impl From<pdf_core::Error> for AppError {
    fn from(e: pdf_core::Error) -> Self {
        use pdf_core::Error as E;
        // Details go to the log; the user sees a plain sentence.
        crate::applog::warn(format!("{e:?}"));
        match e {
            E::NotPdf => AppError::new(
                "This file isn't a PDF, or it is too damaged to open.",
                Some("Check that you picked the right file."),
            ),
            E::Io(_) => AppError::new(
                "The file couldn't be read or written.",
                Some("Check that it still exists and that you have permission to change it."),
            ),
            E::TargetLocked(path) => AppError::new(
                format!(
                    "{} is open in another program, so it can't be replaced.",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ),
                Some(
                    "Close it in the other program (Acrobat locks files it has open), or use Save As.",
                ),
            )
            .with_code(ErrorCode::FileLocked),
            E::NotPermitted => AppError::new(
                "This document's security settings don't allow that change.",
                Some("Ask the document's author for an unrestricted copy."),
            )
            .with_code(ErrorCode::NotPermitted),
            E::ReadOnly => AppError::new(
                "This document is open for reading only, because it is a PDF/A file.",
                Some("Choose Enable editing in the bar above the pages to change it."),
            )
            .with_code(ErrorCode::NotPermitted),
            E::NothingToUndo => {
                AppError::new("There is nothing to undo.", None).with_code(ErrorCode::NothingToUndo)
            }
            E::NothingToRedo => {
                AppError::new("There is nothing to redo.", None).with_code(ErrorCode::NothingToRedo)
            }
            E::PasswordRequired | E::WrongPassword => AppError::new(
                "This document needs a password.",
                Some("Open it again and enter the password."),
            ),
            E::ActorGone => AppError::document_closed(),
            E::Cancelled => AppError::new("Combining was stopped. No file was written.", None)
                .with_code(ErrorCode::Cancelled),
            E::CopyNotPermitted(name) => AppError::new(
                format!("{name} doesn’t allow copying its pages into another document."),
                Some("Its security settings forbid it. Ask its author for an unrestricted copy."),
            )
            .with_code(ErrorCode::NotPermitted),
            E::DamagedOutline => AppError::new(
                "This document’s bookmarks are damaged, so Lectrix can show them but not change them.",
                Some(
                    "Save a copy with File > Save as (optimized) in another app that can repair it, or leave the bookmarks as they are.",
                ),
            ),
            E::DamagedPageTree => AppError::new(
                "This document’s page structure is damaged, so its pages can’t be moved or deleted here.",
                Some(
                    "Save a copy with File > Save as (optimized), open the copy, and try again there.",
                ),
            ),
            _ => AppError::new(
                "Something went wrong while working with this document.",
                Some("Try again. If it keeps happening, the app log has details."),
            ),
        }
    }
}
