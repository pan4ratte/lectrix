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
    /// What printing the document allows (section 6.12).
    pub printing: PrintPermission,
    /// The PDF/A level the file claims, if any: it then opens for reading only, unless
    /// Settings says otherwise, until the user enables editing. A save that may have
    /// broken the standard takes the claim out of the file.
    pub pdfa: Option<PdfaLevel>,
}

/// What printing a document allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PrintPermission {
    NotAllowed,
    /// At low resolution only (150 dpi).
    LowResolution,
    HighResolution,
}

impl From<pdf_core::protection::Printing> for PrintPermission {
    fn from(p: pdf_core::protection::Printing) -> Self {
        use pdf_core::protection::Printing;
        match p {
            Printing::NotAllowed => PrintPermission::NotAllowed,
            Printing::LowResolution => PrintPermission::LowResolution,
            Printing::HighResolution => PrintPermission::HighResolution,
        }
    }
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
            printing: f.printing.into(),
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
    /// The protection the next save writes in place of the file's, if it was changed
    /// (section 6.9).
    pub protection: Option<PendingProtection>,
}

impl From<core::DocumentState> for DocumentState {
    fn from(s: core::DocumentState) -> Self {
        DocumentState {
            revision: s.revision,
            dirty: s.dirty,
            undo_name: s.undo_name,
            redo_name: s.redo_name,
            protection: s.protection.map(Into::into),
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
    /// How far the pages are toned sepia, percent: 0 as the pages are.
    pub sepia: u16,
}

impl PageAdjustments {
    /// The least and the most brightness and contrast, percent (mirrored by
    /// `ADJUST_MIN` and `ADJUST_MAX` in src/lib/features/viewer/adjustments.ts).
    pub const MIN: u16 = 50;
    pub const MAX: u16 = 150;
    /// The most sepia, percent (`SEPIA_MAX` there).
    pub const SEPIA_MAX: u16 = 100;

    /// With brightness, contrast and sepia inside their ranges.
    pub fn clamped(self) -> Self {
        Self {
            brightness: self.brightness.clamp(Self::MIN, Self::MAX),
            contrast: self.contrast.clamp(Self::MIN, Self::MAX),
            sepia: self.sepia.min(Self::SEPIA_MAX),
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
            sepia: 0,
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
    /// The visual adjustments this document has of its own. Without them it is shown with
    /// the ones Settings give every document (`Settings::page_adjustments`).
    #[serde(default)]
    pub own_adjustments: Option<PageAdjustments>,
    /// Up to 1.1 every view had adjustments under this name, the pages as they are
    /// included. Read from such a view, and made the document's own if anything was
    /// adjusted (`normalized`); never written, and not the frontend's to send.
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub adjustments: Option<PageAdjustments>,
    /// The pages turned as they are shown, on top of the view rotation: a page's index and
    /// its turn in degrees (90, 180 or 270). Never in the document either.
    #[serde(default)]
    pub page_turns: BTreeMap<u32, u16>,
}

impl ViewState {
    /// The view as it is stored and handed out: the document's own adjustments within
    /// their range (those of a view stored up to 1.1 among them, if anything was adjusted
    /// there), and only turns that are quarter turns.
    pub fn normalized(mut self) -> Self {
        let before = self.adjustments.take().map(PageAdjustments::clamped);
        self.own_adjustments = self
            .own_adjustments
            .map(PageAdjustments::clamped)
            .or(before.filter(|old| *old != PageAdjustments::default()));
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

/// A step of the start screen's Unlock (ADR 0025): removing a file's password and
/// restrictions into a new file.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum UnprotectStep {
    /// No file was picked.
    Cancelled,
    /// The file has no password and no restrictions: there is nothing to remove.
    NotProtected {
        name: String,
    },
    /// Ask for the password and call `unprotect_password`. Its token is shared with the
    /// passwords asked for opening files, so the prompts never mix up.
    NeedsPassword {
        token: u32,
        name: String,
        /// True after a wrong password.
        retry: bool,
    },
    /// The file is open: ask whether to save it or a copy, then call `unprotect_save`, or
    /// `unprotect_cancel`.
    Ready {
        token: u32,
        name: String,
        /// Its signatures will not match the file written whole: say so when asking.
        signed: bool,
    },
    Failed {
        name: String,
        error: AppError,
    },
}

/// The outcome of `unprotect_save`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum UnprotectSaved {
    /// The Save dialog was given up: nothing was written, and the file is still waiting.
    Cancelled,
    /// The file was written, and opened as `opened` says.
    Saved {
        name: String,
        /// The user saved it over the protected file itself.
        replaced: bool,
        opened: OpenResult,
        /// The file was open in a tab, with no unsaved changes: its document as read again.
        reloaded: Option<Box<DocumentInfo>>,
    },
}

/// What a file allows printing (section 6.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Printing {
    NotAllowed,
    LowResolution,
    HighResolution,
}

/// What a file allows changing, as the Protect dialog names it (section 6.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Changes {
    None,
    /// Inserting, deleting and rotating pages.
    PageAssembly,
    FormsAndSigning,
    CommentsFormsAndSigning,
    AnyExceptExtracting,
}

/// What a file allows to whoever opens it without its permissions password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Restrictions {
    pub printing: Printing,
    pub changes: Changes,
    pub copying: bool,
}

impl From<pdf_core::protection::Restrictions> for Restrictions {
    fn from(r: pdf_core::protection::Restrictions) -> Self {
        use pdf_core::protection::{Changes as C, Printing as P};
        Restrictions {
            printing: match r.printing {
                P::NotAllowed => Printing::NotAllowed,
                P::LowResolution => Printing::LowResolution,
                P::HighResolution => Printing::HighResolution,
            },
            changes: match r.changes {
                C::None => Changes::None,
                C::PageAssembly => Changes::PageAssembly,
                C::FormsAndSigning => Changes::FormsAndSigning,
                C::CommentsFormsAndSigning => Changes::CommentsFormsAndSigning,
                C::AnyExceptExtracting => Changes::AnyExceptExtracting,
            },
            copying: r.copying,
        }
    }
}

impl From<Restrictions> for pdf_core::protection::Restrictions {
    fn from(r: Restrictions) -> Self {
        use pdf_core::protection::{Changes as C, Printing as P};
        pdf_core::protection::Restrictions {
            printing: match r.printing {
                Printing::NotAllowed => P::NotAllowed,
                Printing::LowResolution => P::LowResolution,
                Printing::HighResolution => P::HighResolution,
            },
            changes: match r.changes {
                Changes::None => C::None,
                Changes::PageAssembly => C::PageAssembly,
                Changes::FormsAndSigning => C::FormsAndSigning,
                Changes::CommentsFormsAndSigning => C::CommentsFormsAndSigning,
                Changes::AnyExceptExtracting => C::AnyExceptExtracting,
            },
            copying: r.copying,
        }
    }
}

/// Each permission a file sets, as its `/P` has it.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Allowed {
    pub print: bool,
    pub print_high_resolution: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub assemble: bool,
}

/// The cipher a file is encrypted with.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Cipher {
    Aes,
    Rc4,
    Unknown,
}

/// The protection a file has (sections 6.9 and 6.10).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProtectionInfo {
    /// It needs a password to open.
    pub open_password: bool,
    /// It restricts something to whoever opens it without its permissions password.
    pub restricted: bool,
    pub allowed: Allowed,
    /// The Protect dialog's choices nearest to what it sets.
    pub restrictions: Restrictions,
    pub cipher: Cipher,
    pub key_bits: u32,
    /// It was opened with its permissions password: nothing it restricts applies.
    pub unrestricted: bool,
}

impl From<pdf_core::protection::ProtectionInfo> for ProtectionInfo {
    fn from(p: pdf_core::protection::ProtectionInfo) -> Self {
        let a = p.allowed;
        ProtectionInfo {
            open_password: p.open_password,
            restricted: p.restricted,
            allowed: Allowed {
                print: a.print,
                print_high_resolution: a.print_high_resolution,
                modify: a.modify,
                copy: a.copy,
                annotate: a.annotate,
                fill_forms: a.fill_forms,
                assemble: a.assemble,
            },
            restrictions: p.restrictions.into(),
            cipher: match p.cipher {
                pdf_core::protection::Cipher::Aes => Cipher::Aes,
                pdf_core::protection::Cipher::Rc4 => Cipher::Rc4,
                pdf_core::protection::Cipher::Unknown => Cipher::Unknown,
            },
            key_bits: p.key_bits,
            unrestricted: p.unrestricted,
        }
    }
}

/// A document's protection as its next save writes it, when it was changed (section 6.9).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PendingProtection {
    /// Written without encryption.
    Remove,
    Set {
        open_password: bool,
        /// What the file allows without its permissions password, if it restricts anything.
        restrictions: Option<Restrictions>,
    },
}

impl From<core::PendingProtection> for PendingProtection {
    fn from(p: core::PendingProtection) -> Self {
        match p {
            core::PendingProtection::Remove => PendingProtection::Remove,
            core::PendingProtection::Set {
                open_password,
                restrictions,
            } => PendingProtection::Set {
                open_password,
                restrictions: restrictions.map(Into::into),
            },
        }
    }
}

/// A protection chosen in the Protect dialog (section 6.9): a password to open the file,
/// a permissions password with what the file allows without it, or both.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProtectionInput {
    pub open_password: Option<String>,
    pub permissions_password: Option<String>,
    /// Used only with a permissions password.
    pub restrictions: Restrictions,
}

impl From<ProtectionInput> for pdf_core::protection::ProtectionSettings {
    fn from(p: ProtectionInput) -> Self {
        pdf_core::protection::ProtectionSettings {
            open_password: p.open_password,
            restrict: p
                .permissions_password
                .map(|password| pdf_core::protection::Restrict {
                    password,
                    restrictions: p.restrictions.into(),
                }),
        }
    }
}

/// A change of a document's protection, written by its next save (section 6.9).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ProtectionChangeInput {
    /// The file is written without encryption.
    Remove,
    Set {
        protection: ProtectionInput,
    },
}

impl From<ProtectionChangeInput> for core::ProtectionChange {
    fn from(c: ProtectionChangeInput) -> Self {
        match c {
            ProtectionChangeInput::Remove => core::ProtectionChange::Remove,
            ProtectionChangeInput::Set { protection } => {
                core::ProtectionChange::Set(protection.into())
            }
        }
    }
}

/// What the Description tab of a document's properties changes (section 6.10). An empty
/// value is one the document does not have.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentDescription {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
}

impl From<pdf_core::properties::Description> for DocumentDescription {
    fn from(d: pdf_core::properties::Description) -> Self {
        DocumentDescription {
            title: d.title,
            author: d.author,
            subject: d.subject,
            keywords: d.keywords,
        }
    }
}

impl From<DocumentDescription> for pdf_core::properties::Description {
    fn from(d: DocumentDescription) -> Self {
        pdf_core::properties::Description {
            title: d.title,
            author: d.author,
            subject: d.subject,
            keywords: d.keywords,
        }
    }
}

/// A document's properties (section 6.10): its file, what it declares about itself, and
/// its protection.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentProperties {
    /// The folder the file is in; null for a document that is no file yet.
    pub folder: Option<String>,
    /// The file's size in bytes, as it is on disk.
    #[ts(type = "number | null")]
    pub file_size: Option<u64>,
    /// "1.7".
    pub version: String,
    pub description: DocumentDescription,
    /// Milliseconds since the Unix epoch.
    #[ts(type = "number | null")]
    pub created: Option<i64>,
    #[ts(type = "number | null")]
    pub modified: Option<i64>,
    /// The application it was made with (`/Creator`).
    pub creator: Option<String>,
    /// What wrote the PDF (`/Producer`).
    pub producer: Option<String>,
    pub tagged: bool,
    /// Linearized ("fast web view").
    pub linearized: bool,
    /// How many signature fields hold a signature.
    pub signatures: u32,
    /// The protection its file has.
    pub protection: Option<ProtectionInfo>,
}

/// The kind of a font, as the Fonts tab names it (section 6.10).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FontKind {
    Type1,
    Type1Compact,
    MultipleMaster,
    TrueType,
    OpenType,
    Type3,
    CidType0,
    CidTrueType,
    Unknown,
}

/// A font a document uses (section 6.10).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsedFont {
    /// Its object number (0 for a font written directly into its resources).
    pub id: u32,
    /// Without a subset's prefix.
    pub name: String,
    pub subset: bool,
    pub kind: FontKind,
    pub embedded: bool,
    /// One of the 14 standard fonts every reader has, not embedded.
    pub standard: bool,
    /// Its `/Encoding` name; "Custom" for its own differences, null for the font's own.
    pub encoding: Option<String>,
    /// The first page (0-based) of those asked about that uses it.
    pub first_page: u32,
}

impl From<pdf_core::properties::UsedFont> for UsedFont {
    fn from(f: pdf_core::properties::UsedFont) -> Self {
        use pdf_core::properties::FontKind as K;
        UsedFont {
            id: f.id,
            name: f.name,
            subset: f.subset,
            kind: match f.kind {
                K::Type1 => FontKind::Type1,
                K::Type1Compact => FontKind::Type1Compact,
                K::MultipleMaster => FontKind::MultipleMaster,
                K::TrueType => FontKind::TrueType,
                K::OpenType => FontKind::OpenType,
                K::Type3 => FontKind::Type3,
                K::CidType0 => FontKind::CidType0,
                K::CidTrueType => FontKind::CidTrueType,
                K::Unknown => FontKind::Unknown,
            },
            embedded: f.embedded,
            standard: f.standard,
            encoding: f.encoding,
            first_page: u32::try_from(f.first_page).unwrap_or(u32::MAX),
        }
    }
}

/// A step of Protect as a tool on a file (section 6.9).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ProtectStep {
    /// No file was picked.
    Cancelled,
    /// Ask for the password and call `protect_password`.
    NeedsPassword {
        token: u32,
        name: String,
        /// True after a wrong password.
        retry: bool,
    },
    /// The file is open: show the Protect dialog, then ask whether to save it or a copy,
    /// and call `protect_save`, or `protect_cancel`.
    Ready {
        token: u32,
        name: String,
        /// The protection it has now, to start the dialog from.
        protection: Option<ProtectionInfo>,
        /// Its signatures will not match the file written whole: say so when asking.
        signed: bool,
        /// The PDF/A level it claims, which protection takes away.
        pdfa: Option<PdfaLevel>,
    },
    Failed {
        name: String,
        error: AppError,
    },
}

/// The outcome of `protect_save`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ProtectSaved {
    /// The Save dialog was given up: nothing was written, and the file is still waiting.
    Cancelled,
    /// The file was written, and opened as `opened` says.
    Saved {
        name: String,
        /// The user saved it over the file itself.
        replaced: bool,
        opened: OpenResult,
        /// The file was open in a tab, with no unsaved changes: its document as read again.
        reloaded: Option<Box<DocumentInfo>>,
        /// The PDF/A level the file no longer claims.
        pdfa_removed: Option<PdfaLevel>,
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

/// A window button the app draws in its title bar (section 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// The window buttons the app draws at each end of its title bar, in order from left to
/// right. Empty where the system draws them itself (macOS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindowChrome {
    pub left: Vec<WindowButton>,
    pub right: Vec<WindowButton>,
}

/// An entry of the app's menus, as the frontend lays them out (`src/lib/menus.svelte.ts`),
/// for the system's menu bar on macOS (section 8, `menu.rs`). The menus themselves are
/// submenus at the top level.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum MenuEntry {
    /// Runs the frontend's item `id` when chosen. With `checked`, it has a check mark that
    /// shows when true. `accelerator` is the shortcut it shows, as muda reads one
    /// ("Cmd+Shift+S").
    Item {
        id: String,
        label: String,
        enabled: bool,
        checked: Option<bool>,
        accelerator: Option<String>,
    },
    Separator,
    Submenu {
        label: String,
        enabled: bool,
        items: Vec<MenuEntry>,
    },
    /// An item the system provides and runs itself.
    System {
        item: SystemMenuItem,
    },
}

/// Items of macOS's own (`MenuEntry::System`): text editing in the focused field, and the
/// app and window commands every Mac app has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SystemMenuItem {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    BringAllToFront,
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

/// What reopening the tab a window closed last did (File > Reopen closed tab).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Reopened {
    /// None when the window had no closed tab to reopen.
    pub result: Option<OpenResult>,
    /// How many closed tabs the window can still reopen.
    pub remaining: u32,
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
    /// What the document allows after the save: a change of its protection (section 6.9)
    /// changes it.
    pub flags: DocumentFlags,
}

/// An annotation named by its page and object number.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotationTarget {
    pub page: u32,
    pub id: u32,
}

/// One annotation's change among several made together.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotationUpdateInput {
    pub page: u32,
    pub id: u32,
    pub edit: AnnotationEditInput,
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
    /// Give a bookmark's title a colour (#rrggbb), or take it away (null).
    SetBookmarkColor {
        id: u32,
        color: Option<String>,
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
    /// Change several annotations (selected together) as one undo step.
    UpdateAnnotations {
        updates: Vec<AnnotationUpdateInput>,
    },
    /// Delete several annotations, each with its popup and replies, as one undo step.
    DeleteAnnotations {
        annotations: Vec<AnnotationTarget>,
    },
    /// Reply to annotation `parent` on `page`; the author is the name from Settings.
    AddReply {
        page: u32,
        parent: u32,
        text: String,
    },
    /// Change the document's title, author, subject and keywords (its properties, section
    /// 6.10); an empty one is taken out.
    SetDescription {
        description: DocumentDescription,
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
            OperationInput::SetBookmarkColor { id, color } => Op::SetBookmarkColor {
                id,
                color: match color {
                    Some(hex) => {
                        let c = crate::annotations::parse_hex(&hex)?;
                        Some([c.r, c.g, c.b])
                    }
                    None => None,
                },
            },
            OperationInput::SetPageLabels { rules } => Op::SetPageLabels {
                rules: rules.into_iter().map(Into::into).collect(),
            },
            OperationInput::SetDescription { description } => Op::SetDescription {
                description: description.into(),
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
            OperationInput::UpdateAnnotations { updates } => Op::UpdateAnnotations {
                updates: updates
                    .into_iter()
                    .map(|u| {
                        Ok(pdf_core::ops::AnnotationUpdate {
                            page: u.page as usize,
                            id: u.id,
                            edit: u.edit.into_core()?,
                        })
                    })
                    .collect::<Result<_, AppError>>()?,
            },
            OperationInput::DeleteAnnotations { annotations } => Op::DeleteAnnotations {
                annotations: annotations
                    .into_iter()
                    .map(|a| (a.page as usize, a.id))
                    .collect(),
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
    /// The words of these pages that contain the query, as the text has them (each
    /// spelling once), for the search bar's suggestions (section 6.1).
    pub words: Vec<SearchWord>,
}

/// A word that contains a search's query, and how many times the pages searched have it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchWord {
    pub text: String,
    pub count: u32,
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
    /// Pinned to the top of the list (section 6.1).
    pub pinned: bool,
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
/// screen shows recent files, and how wide the Combine view's pane is. Remembered in app
/// data. Widths are CSS pixels;
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
    /// The width of the Combine view's pane (its files and options), CSS pixels.
    pub combine_width: u32,
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
            combine_width: 224,
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

/// What the annotation list's rows are sorted by (section 6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AnnotationSortKey {
    #[default]
    Page,
    Author,
    Created,
    Modified,
}

/// A to Z (first page, A, oldest first) or Z to A.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

/// How the annotation list is sorted (section 6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotationSort {
    pub sort: AnnotationSortKey,
    pub order: SortOrder,
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
    /// Whether the bar of quick tools shows over selected text (section 6.5).
    pub quick_bar: bool,
    /// The buttons of that bar, kept while it is switched off; empty: no bar.
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
    /// Whether going to the next or previous match of a search glides there (section
    /// 6.1).
    pub smooth_search_scroll: bool,
    /// Whether the matches of a search are marked beside the pages' scrollbar (section
    /// 6.1).
    pub search_markers: bool,
    /// How long the pointer rests on an annotation before its comment shows, in ms.
    pub tooltip_delay_ms: u32,
    /// Whether marking text up selects the new annotation and opens the inspector on its
    /// comment (section 6.5); otherwise nothing is selected.
    pub open_comment_after_markup: bool,
    /// Whether new annotations get the colour and opacity last given to one of their type
    /// (section 6.5); otherwise the defaults, and the toolbar's choice until Lectrix closes.
    pub remember_annotation_style: bool,
    /// Whether the annotation list is sorted as it was last sorted, in every document and
    /// after a restart (section 6.5); otherwise each document starts sorted by page.
    pub remember_annotation_sort: bool,
    /// The sort last picked in the annotation list, while it is remembered.
    pub annotation_sort: AnnotationSort,
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
    /// The visual adjustments every document is shown with, unless it has its own
    /// (section 6.1): the pages as they are until changed in Settings.
    pub page_adjustments: PageAdjustments,
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
    pub quick_bar: bool,
    pub quick_tools: Vec<QuickTool>,
    pub check_for_updates: bool,
    pub smooth_zoom: bool,
    pub smooth_annotation_scroll: bool,
    pub smooth_page_turn: bool,
    pub smooth_bookmark_scroll: bool,
    pub smooth_search_scroll: bool,
    pub search_markers: bool,
    pub tooltip_delay_ms: u32,
    pub open_comment_after_markup: bool,
    pub remember_annotation_style: bool,
    pub remember_annotation_sort: bool,
    pub annotation_sort: AnnotationSort,
    pub selection_handles: bool,
    pub pdfa_read_only: bool,
    pub highlight_links: bool,
    pub link_previews: bool,
    pub page_adjustments: PageAdjustments,
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
                Some("Unlock it to remove its restrictions (File > Unlock…)."),
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
                Some("Its security settings forbid it. Unlock it first (File > Unlock…), then try again."),
            )
            .with_code(ErrorCode::NotPermitted),
            E::DamagedOutline => AppError::new(
                "This document’s bookmarks are damaged, so Lectrix can show them but not change them.",
                Some(
                    "Save a copy with File > Save as (optimized) in another app that can repair it, or leave the bookmarks as they are.",
                ),
            ),
            E::Printer { printer, .. } if printer.is_empty() => AppError::new(
                "Lectrix couldn’t get the list of printers.",
                Some("Try again. If it keeps happening, check your printers in the system’s settings."),
            ),
            E::Printer { printer, .. } => AppError::new(
                format!("Lectrix couldn’t print on {printer}."),
                Some("Check that the printer is on and connected, then try again, or pick another printer."),
            ),
            E::PrintNotAllowed => AppError::new(
                "This document’s security settings don’t allow printing it.",
                Some("Ask its author for a copy that can be printed."),
            )
            .with_code(ErrorCode::NotPermitted),
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

// ----- Print (section 6.12, ADR 0028) -----

/// A printer of the system.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrinterEntry {
    pub name: String,
    /// The system's default printer.
    pub default: bool,
}

/// The printers, and the Print dialog's choices used last.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrintStart {
    pub printers: Vec<PrinterEntry>,
    pub remembered: Option<PrintPreferences>,
}

/// A paper size a printer takes.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrintPaper {
    /// The printer's own id for it.
    pub id: i32,
    pub name: String,
    /// In points, portrait.
    pub width: f32,
    pub height: f32,
}

/// What a printer can do, as it is set now.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrinterDetails {
    pub papers: Vec<PrintPaper>,
    /// The paper it is set to.
    pub paper: Option<i32>,
    pub duplex: bool,
    /// Its settings ask for both sides.
    pub duplex_on: bool,
    pub colour: bool,
    /// It has settings of its own to show (a driver's dialog, on Windows).
    pub settings_dialog: bool,
}

impl PrinterDetails {
    pub fn new(c: pdf_core::platform::PrinterCapabilities, settings_dialog: bool) -> Self {
        PrinterDetails {
            papers: c
                .papers
                .into_iter()
                .map(|p| PrintPaper {
                    id: p.id,
                    name: p.name,
                    width: p.width,
                    height: p.height,
                })
                .collect(),
            paper: c.paper,
            duplex: c.duplex,
            duplex_on: c.duplex_on,
            colour: c.colour,
            settings_dialog,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PrintSizing {
    Fit,
    ActualSize,
    ShrinkOversized,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PrintOrientation {
    Auto,
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PrintDuplex {
    OneSided,
    LongEdge,
    ShortEdge,
}

impl From<PrintDuplex> for pdf_core::print::Duplex {
    fn from(d: PrintDuplex) -> Self {
        match d {
            PrintDuplex::OneSided => pdf_core::print::Duplex::OneSided,
            PrintDuplex::LongEdge => pdf_core::print::Duplex::LongEdge,
            PrintDuplex::ShortEdge => pdf_core::print::Duplex::ShortEdge,
        }
    }
}

/// What the Print dialog asks for.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrintSettings {
    pub printer: String,
    /// The pages, in the order they print (0-based).
    pub pages: Vec<u32>,
    pub sizing: PrintSizing,
    /// Percent, for `custom` sizing.
    pub scale: u16,
    pub orientation: PrintOrientation,
    /// The printer's id of the paper; null for the one it is set to.
    pub paper: Option<i32>,
    pub duplex: PrintDuplex,
    pub annotations: bool,
    pub grey: bool,
    pub copies: u16,
    pub collate: bool,
}

/// Smallest and largest custom scale, percent.
pub const PRINT_SCALE: (u16, u16) = (10, 1000);

impl PrintSettings {
    pub fn sizing(&self) -> pdf_core::print::Sizing {
        use pdf_core::print::Sizing;
        match self.sizing {
            PrintSizing::Fit => Sizing::Fit,
            PrintSizing::ActualSize => Sizing::ActualSize,
            PrintSizing::ShrinkOversized => Sizing::ShrinkOversized,
            PrintSizing::Custom => {
                Sizing::Scale(f32::from(self.scale.clamp(PRINT_SCALE.0, PRINT_SCALE.1)) / 100.0)
            }
        }
    }

    pub fn orientation(&self) -> pdf_core::print::Orientation {
        use pdf_core::print::Orientation;
        match self.orientation {
            PrintOrientation::Auto => Orientation::Auto,
            PrintOrientation::Portrait => Orientation::Portrait,
            PrintOrientation::Landscape => Orientation::Landscape,
        }
    }

    /// What the dialog keeps for next time: not the pages or the copies.
    pub fn preferences(&self) -> PrintPreferences {
        PrintPreferences {
            printer: Some(self.printer.clone()),
            sizing: self.sizing,
            scale: self.scale,
            orientation: self.orientation,
            duplex: self.duplex,
            annotations: self.annotations,
            grey: self.grey,
            collate: self.collate,
        }
    }
}

/// The Print dialog's choices used last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrintPreferences {
    pub printer: Option<String>,
    pub sizing: PrintSizing,
    pub scale: u16,
    pub orientation: PrintOrientation,
    pub duplex: PrintDuplex,
    pub annotations: bool,
    pub grey: bool,
    pub collate: bool,
}

/// A rectangle on a sheet, in points from its top-left corner.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SheetArea {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<pdf_core::print::Area> for SheetArea {
    fn from(a: pdf_core::print::Area) -> Self {
        SheetArea {
            x: a.x,
            y: a.y,
            width: a.width,
            height: a.height,
        }
    }
}

/// Which way a page is turned on its sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PrintTurn {
    Upright,
    /// A quarter to the left: its top along the sheet's left edge.
    Left,
    /// A quarter to the right.
    Right,
}

impl From<pdf_core::print::Turn> for PrintTurn {
    fn from(t: pdf_core::print::Turn) -> Self {
        match t {
            pdf_core::print::Turn::Upright => PrintTurn::Upright,
            pdf_core::print::Turn::Left => PrintTurn::Left,
            pdf_core::print::Turn::Right => PrintTurn::Right,
        }
    }
}

/// Where a page goes on its sheet.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SheetPlacement {
    pub page: u32,
    pub turn: PrintTurn,
    /// The page's box on the sheet.
    pub area: SheetArea,
}

/// The sheets a job prints (one copy), for the preview.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrintLayout {
    /// The paper, in the job's orientation.
    pub width: f32,
    pub height: f32,
    pub printable: SheetArea,
    pub landscape: bool,
    pub sheets: Vec<SheetPlacement>,
    /// The document allows printing at low resolution only.
    pub low_resolution: bool,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrintProgress {
    /// Sheets sent to the printer.
    pub done: u32,
    pub total: u32,
}

/// A job sent to the printer.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrintDone {
    /// The document's pages printed, once each: what the notification counts.
    pub pages: u32,
    /// Sheets sent, with the copies Lectrix made and blank backs.
    pub sheets: u32,
}

// ----- Compress (section 6.11, ADR 0027) -----

/// The dialog's presets; `Custom` once a setting differs from the one chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CompressPreset {
    Lossless,
    HighQuality,
    Balanced,
    Smallest,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ResampleMethod {
    Bicubic,
    Average,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageCompression {
    Keep,
    Jpeg,
    Jpeg2000,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageColorMode {
    Keep,
    Gray,
    BlackAndWhite,
}

/// Downsampling: images drawn finer than `aboveDpi` go to `toDpi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownsampleSettings {
    pub enabled: bool,
    pub to_dpi: u32,
    pub above_dpi: u32,
    pub method: ResampleMethod,
}

/// How colour or greyscale images are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageSettings {
    pub downsample: DownsampleSettings,
    pub compression: ImageCompression,
    /// 1 to 100.
    pub quality: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompressRemovals {
    pub thumbnails: bool,
    pub private_data: bool,
    pub attachments: bool,
    pub javascript: bool,
    pub tags: bool,
}

/// The Compress dialog's settings, as it sends them to `compress_run` and as they are
/// remembered for the next time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompressSettings {
    pub preset: CompressPreset,
    /// The separate settings for colour, greyscale and black-and-white images are shown.
    pub separate: bool,
    pub color: ImageSettings,
    pub gray: ImageSettings,
    /// Black-and-white images: always lossless.
    pub mono: DownsampleSettings,
    pub color_mode: ImageColorMode,
    pub subset_fonts: bool,
    pub object_streams: bool,
    pub remove: CompressRemovals,
}

impl From<pdf_core::compress::Resample> for ResampleMethod {
    fn from(m: pdf_core::compress::Resample) -> Self {
        match m {
            pdf_core::compress::Resample::Bicubic => ResampleMethod::Bicubic,
            pdf_core::compress::Resample::Average => ResampleMethod::Average,
        }
    }
}

impl From<ResampleMethod> for pdf_core::compress::Resample {
    fn from(m: ResampleMethod) -> Self {
        match m {
            ResampleMethod::Bicubic => pdf_core::compress::Resample::Bicubic,
            ResampleMethod::Average => pdf_core::compress::Resample::Average,
        }
    }
}

impl DownsampleSettings {
    /// Settings for `d`; one turned off keeps the preset's numbers to show.
    fn from_core(d: Option<pdf_core::compress::Downsample>, shown: u32) -> Self {
        match d {
            Some(d) => DownsampleSettings {
                enabled: true,
                to_dpi: d.to_dpi,
                above_dpi: d.above_dpi,
                method: d.method.into(),
            },
            None => DownsampleSettings {
                enabled: false,
                to_dpi: shown,
                above_dpi: shown * 3 / 2,
                method: ResampleMethod::Bicubic,
            },
        }
    }

    fn to_core(self) -> Option<pdf_core::compress::Downsample> {
        self.enabled.then_some(pdf_core::compress::Downsample {
            to_dpi: self.to_dpi,
            above_dpi: self.above_dpi,
            method: self.method.into(),
        })
    }
}

impl ImageSettings {
    fn from_core(s: pdf_core::compress::ImageSettings, shown: u32) -> Self {
        use pdf_core::compress::Compression;
        ImageSettings {
            downsample: DownsampleSettings::from_core(s.downsample, shown),
            compression: match s.compression {
                Compression::Keep => ImageCompression::Keep,
                Compression::Jpeg => ImageCompression::Jpeg,
                Compression::Jpeg2000 => ImageCompression::Jpeg2000,
            },
            quality: s.quality,
        }
    }

    fn to_core(self) -> pdf_core::compress::ImageSettings {
        use pdf_core::compress::Compression;
        pdf_core::compress::ImageSettings {
            downsample: self.downsample.to_core(),
            compression: match self.compression {
                ImageCompression::Keep => Compression::Keep,
                ImageCompression::Jpeg => Compression::Jpeg,
                ImageCompression::Jpeg2000 => Compression::Jpeg2000,
            },
            quality: self.quality,
        }
    }
}

impl CompressSettings {
    /// The settings of `preset`, which must not be `Custom`.
    pub fn of_preset(preset: CompressPreset) -> CompressSettings {
        use pdf_core::compress::{ColorMode, Preset};
        let core = match preset {
            CompressPreset::Lossless => Preset::Lossless,
            CompressPreset::HighQuality => Preset::HighQuality,
            CompressPreset::Balanced | CompressPreset::Custom => Preset::Balanced,
            CompressPreset::Smallest => Preset::Smallest,
        }
        .options();
        CompressSettings {
            preset,
            separate: false,
            color: ImageSettings::from_core(core.color, 150),
            gray: ImageSettings::from_core(core.gray, 150),
            mono: DownsampleSettings::from_core(core.mono, 300),
            color_mode: match core.color_mode {
                ColorMode::Keep => ImageColorMode::Keep,
                ColorMode::Gray => ImageColorMode::Gray,
                ColorMode::BlackAndWhite => ImageColorMode::BlackAndWhite,
            },
            subset_fonts: core.subset_fonts,
            object_streams: core.object_streams,
            remove: CompressRemovals {
                thumbnails: core.remove.thumbnails,
                private_data: core.remove.private_data,
                attachments: core.remove.attachments,
                javascript: core.remove.javascript,
                tags: core.remove.tags,
            },
        }
    }

    pub fn to_core(self) -> pdf_core::compress::CompressOptions {
        use pdf_core::compress::{ColorMode, CompressOptions, Removals};
        CompressOptions {
            color: self.color.to_core(),
            gray: self.gray.to_core(),
            mono: self.mono.to_core(),
            color_mode: match self.color_mode {
                ImageColorMode::Keep => ColorMode::Keep,
                ImageColorMode::Gray => ColorMode::Gray,
                ImageColorMode::BlackAndWhite => ColorMode::BlackAndWhite,
            },
            subset_fonts: self.subset_fonts,
            object_streams: self.object_streams,
            remove: Removals {
                thumbnails: self.remove.thumbnails,
                private_data: self.remove.private_data,
                attachments: self.remove.attachments,
                javascript: self.remove.javascript,
                tags: self.remove.tags,
            },
        }
    }
}

/// The settings of each preset, so the dialog has them from Rust (`pdf_core::compress`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompressPresets {
    pub lossless: CompressSettings,
    pub high_quality: CompressSettings,
    pub balanced: CompressSettings,
    pub smallest: CompressSettings,
}

impl CompressPresets {
    pub fn new() -> Self {
        CompressPresets {
            lossless: CompressSettings::of_preset(CompressPreset::Lossless),
            high_quality: CompressSettings::of_preset(CompressPreset::HighQuality),
            balanced: CompressSettings::of_preset(CompressPreset::Balanced),
            smallest: CompressSettings::of_preset(CompressPreset::Smallest),
        }
    }
}

/// Bytes, and how many objects hold them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SizeUsage {
    #[ts(type = "number")]
    pub bytes: u64,
    pub count: u32,
}

impl From<pdf_core::compress::Usage> for SizeUsage {
    fn from(u: pdf_core::compress::Usage) -> Self {
        SizeUsage {
            bytes: u.bytes,
            count: u32::try_from(u.count).unwrap_or(u32::MAX),
        }
    }
}

/// What the file has that Compress can remove (the dialog offers only these).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompressFound {
    pub thumbnails: u32,
    pub private_data: u32,
    pub attachments: u32,
    pub javascript: u32,
    pub tags: bool,
}

/// What a file's size is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompressAnalysis {
    #[ts(type = "number")]
    pub file_size: u64,
    pub pages: u32,
    pub images: SizeUsage,
    pub fonts: SizeUsage,
    pub content: SizeUsage,
    pub attachments: SizeUsage,
    pub thumbnails: SizeUsage,
    pub private_data: SizeUsage,
    pub metadata: SizeUsage,
    #[ts(type = "number")]
    pub other: u64,
    pub found: CompressFound,
}

impl From<pdf_core::compress::Analysis> for CompressAnalysis {
    fn from(a: pdf_core::compress::Analysis) -> Self {
        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        CompressAnalysis {
            file_size: a.file_size,
            pages: n(a.pages),
            images: a.images.into(),
            fonts: a.fonts.into(),
            content: a.content.into(),
            attachments: a.attachments.into(),
            thumbnails: a.thumbnails.into(),
            private_data: a.private_data.into(),
            metadata: a.metadata.into(),
            other: a.other,
            found: CompressFound {
                thumbnails: n(a.found.thumbnails),
                private_data: n(a.found.private_data),
                attachments: n(a.found.attachments),
                javascript: n(a.found.javascript),
                tags: a.found.tags,
            },
        }
    }
}

/// What follows picking a file to compress, or typing its password.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CompressStep {
    /// No file was picked.
    Cancelled,
    /// Ask for the password and call `compress_password`.
    NeedsPassword {
        token: u32,
        name: String,
        /// True after a wrong password.
        retry: bool,
    },
    /// The file is open: show the dialog, then call `compress_run`, `compress_save` and
    /// `compress_cancel`.
    Ready {
        token: u32,
        name: String,
        analysis: CompressAnalysis,
        /// Its signatures will not match the file written whole: say so.
        signed: bool,
        /// The PDF/A level it claims, which compressing takes away.
        pdfa: Option<PdfaLevel>,
        presets: Box<CompressPresets>,
        /// The settings used last, to start from.
        remembered: Option<Box<CompressSettings>>,
    },
    Failed {
        name: String,
        error: AppError,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CompressStage {
    Reading,
    Images,
    Fonts,
    Writing,
}

/// Sent on `compress_run`'s channel while compressing.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompressProgress {
    pub stage: CompressStage,
    /// Pages read or images done so far, of `total` (0 for the stages without a count).
    pub done: u32,
    pub total: u32,
}

/// What compressing made, waiting to be saved or given up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompressOutcome {
    #[ts(type = "number")]
    pub before: u64,
    #[ts(type = "number")]
    pub after: u64,
    pub images: u32,
    pub images_replaced: u32,
    pub images_downsampled: u32,
    pub images_skipped: u32,
    pub fonts_subset: u32,
    pub removed: CompressFound,
    /// The page to preview first (0-based).
    pub preview_page: u32,
    pub pages: u32,
}

/// The outcome of `compress_save`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CompressSaved {
    /// The Save dialog was given up: nothing was written, and the result is still waiting.
    Cancelled,
    /// The result was written, and opened as `opened` says.
    Saved {
        name: String,
        /// The user saved it over the file itself.
        replaced: bool,
        opened: OpenResult,
        /// The file was open in a tab, with no unsaved changes: its document as read again.
        reloaded: Option<Box<DocumentInfo>>,
        #[ts(type = "number")]
        before: u64,
        #[ts(type = "number")]
        after: u64,
        /// The PDF/A level the file no longer claims.
        pdfa_removed: Option<PdfaLevel>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Compress dialog's simple form sets black-and-white images from the colour
    /// settings (`simpleMono` in src/lib/features/compress/compress.ts): twice their
    /// resolution, at least 300 dpi, above one and a half times that. The presets agree, so
    /// choosing one in the simple form keeps its name.
    #[test]
    fn presets_are_what_the_simple_form_makes_of_them() {
        for preset in [
            CompressPreset::Lossless,
            CompressPreset::HighQuality,
            CompressPreset::Balanced,
            CompressPreset::Smallest,
        ] {
            let s = CompressSettings::of_preset(preset);
            assert_eq!(s.gray, s.color, "{preset:?}");
            let to = (s.color.downsample.to_dpi * 2).max(300);
            assert_eq!(s.mono.enabled, s.color.downsample.enabled, "{preset:?}");
            assert_eq!(s.mono.to_dpi, to, "{preset:?}");
            assert_eq!(s.mono.above_dpi, to * 3 / 2, "{preset:?}");
            assert_eq!(s.mono.method, s.color.downsample.method, "{preset:?}");
            pdf_core::compress::validate(&s.to_core()).unwrap();
        }
    }
}
