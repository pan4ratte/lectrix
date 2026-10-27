//! `pdf-cli`: every pdf-core operation from the command line, for tests, the interop
//! harness, and reproducing engine bugs without the UI.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

mod survey;

use clap::{Parser, Subcommand, ValueEnum};
use mupdf::pdf::{Encryption, PdfDocument, PdfWriteOptions, Permission};
use mupdf::text_page::TextPageFlags;

use pdf_core::annot::quads::Quad;
use pdf_core::annot::{self, AnnotationEdit, Body, MarkupKind, MarkupSpec, NewAnnotation, Rgb};
use pdf_core::docinfo::{self, PdfA};
use pdf_core::geometry::{Point, Rect};
use pdf_core::labels::{self, LabelRule, LabelStyle};
use pdf_core::merge::{
    self, BookmarkMode, InsertLabels, InsertOptions, LabelMode, MergeOptions, MergeReport,
    MergeSource, PagePick,
};
use pdf_core::ops::{InsertSource, Operation};
use pdf_core::outline::{self, Bookmark, OutlineItem, OutlineTarget, Target, ViewDest};
use pdf_core::render;
use pdf_core::save::{self, SaveKind};
use pdf_core::session::Session;
use pdf_core::testgen::{self, SampleSpec};
use pdf_core::{Error, Result};

#[derive(Parser)]
#[command(name = "pdf-cli", version, about = "Headless Lectrix PDF engine")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a sample PDF (text pages; optional rotation, CropBox, UserUnit, PDF/A claim).
    Gen {
        out: PathBuf,
        #[arg(long, default_value_t = 3)]
        pages: usize,
        #[arg(long, default_value_t = 0)]
        rotate: i32,
        /// CropBox as x0,y0,x1,y1 in PDF user space.
        #[arg(long, value_parser = parse_rect)]
        crop: Option<Rect>,
        #[arg(long)]
        user_unit: Option<f64>,
        #[arg(long, default_value = "Lectrix sample")]
        title: String,
        /// Claim a PDF/A level in XMP metadata, such as 1b, 2u or 4 (the claim only: the
        /// file does not comply).
        #[arg(long, value_parser = parse_pdfa)]
        pdfa: Option<PdfA>,
        /// Encrypt it (AES-256) with this open password.
        #[arg(long)]
        password: Option<String>,
        /// Encrypt it (AES-256) with this permissions password, allowing what `--allow`
        /// says. Alone, the file opens without a password.
        #[arg(long)]
        owner_password: Option<String>,
        /// What an encrypted file allows, comma-separated: print, print-hq, modify, copy,
        /// annotate, form, accessibility, assemble; or none. Annotating alone by default.
        #[arg(long, value_parser = parse_permissions, default_value = "annotate")]
        allow: u32,
    },
    /// List the printers the system has (section 6.12), the default one marked, with what
    /// each takes.
    Printers,
    /// Print a file as File > Print does (section 6.12). `--output` has the printer write
    /// a file instead (Windows' Microsoft Print to PDF; on CUPS, the PDF lp would get).
    Print {
        input: PathBuf,
        #[arg(long)]
        printer: String,
        #[arg(long)]
        output: Option<PathBuf>,
        /// Physical page numbers, "1-3,7"; all pages by default.
        #[arg(long)]
        pages: Option<String>,
        #[arg(long, value_enum, default_value_t = SizingArg::Fit)]
        sizing: SizingArg,
        /// Percent, with `--sizing scale`.
        #[arg(long, default_value_t = 100.0)]
        scale: f32,
        #[arg(long, value_enum, default_value_t = OrientationArg::Auto)]
        orientation: OrientationArg,
        /// The printer's paper id (`pdf-cli printers` lists them).
        #[arg(long)]
        paper: Option<i32>,
        #[arg(long)]
        no_annotations: bool,
        #[arg(long)]
        grey: bool,
        #[arg(long, default_value_t = 1)]
        copies: u16,
        #[arg(long)]
        uncollated: bool,
        #[arg(long, value_enum, default_value_t = DuplexArg::One)]
        duplex: DuplexArg,
        #[arg(long)]
        password: Option<String>,
    },
    /// Remove a file's password and restrictions (the start screen's Unlock), writing it
    /// whole without encryption. Any password that opens it will do.
    Unlock {
        input: PathBuf,
        out: PathBuf,
        #[arg(long)]
        password: Option<String>,
    },
    /// Give a file a password to open it, restrictions, or both (Protect), in place of any
    /// protection it had: AES-256, written whole. `--password` opens a protected input.
    Protect {
        input: PathBuf,
        out: PathBuf,
        #[arg(long)]
        password: Option<String>,
        /// The password to open the written file.
        #[arg(long)]
        open_password: Option<String>,
        /// The permissions password; with it, the file restricts what the options below
        /// leave out.
        #[arg(long)]
        permissions_password: Option<String>,
        #[arg(long, value_enum, default_value_t = PrintingArg::High)]
        printing: PrintingArg,
        #[arg(long, value_enum, default_value_t = ChangesArg::None)]
        changes: ChangesArg,
        /// Allow copying text and images.
        #[arg(long)]
        copying: bool,
    },
    /// Print page count, PDF/A claim, page labels, outline and annotations.
    Info { input: PathBuf },
    /// Compare two documents (section 6.13): list the differences, and optionally write
    /// annotated copies of either and the report.
    Compare {
        old: PathBuf,
        new: PathBuf,
        #[arg(long, value_enum, default_value_t = CompareModeArg::Automatic)]
        mode: CompareModeArg,
        /// Leave punctuation out.
        #[arg(long)]
        no_punctuation: bool,
        /// Leave out words that differ by one letter.
        #[arg(long)]
        no_one_letter: bool,
        /// Compare headers and footers as body text.
        #[arg(long)]
        no_margins: bool,
        /// Write a copy of the old document with the differences marked.
        #[arg(long)]
        annotate_old: Option<PathBuf>,
        /// Write a copy of the new document with the differences marked.
        #[arg(long)]
        annotate_new: Option<PathBuf>,
        /// Write the report.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Print the differences as JSON lines.
        #[arg(long)]
        json: bool,
    },
    /// Generate a three-page sample whose images cover what Compress handles.
    GenImages { out: PathBuf },
    /// Generate a document of given text (Compare's tests): a page for each `--page`, its
    /// lines separated by `|`, each page with `--header` at its top and "Page n" at its foot.
    GenText {
        out: PathBuf,
        #[arg(long, default_value = "Lectrix sample")]
        header: String,
        #[arg(long = "page", required = true)]
        pages: Vec<String>,
    },
    /// Print what a file's size is made of, and what Compress could remove from it.
    Analyze {
        input: PathBuf,
        #[arg(long)]
        password: Option<String>,
    },
    /// Write a file smaller (Compress): a preset, changed by the options after it.
    Compress {
        input: PathBuf,
        out: PathBuf,
        #[arg(long)]
        password: Option<String>,
        #[arg(long, value_enum, default_value_t = PresetArg::Balanced)]
        preset: PresetArg,
        /// Downsample colour and greyscale images to this resolution (0: never).
        #[arg(long)]
        dpi: Option<u32>,
        /// Downsample black-and-white images to this resolution (0: never).
        #[arg(long)]
        mono_dpi: Option<u32>,
        /// Downsample only images finer than this many times the resolution.
        #[arg(long, default_value_t = 1.5)]
        above: f32,
        #[arg(long, value_enum)]
        method: Option<MethodArg>,
        #[arg(long, value_enum)]
        compression: Option<CompressionArg>,
        /// JPEG or JPEG 2000 quality, 1 to 100.
        #[arg(long)]
        quality: Option<u8>,
        #[arg(long, value_enum, default_value_t = ColorModeArg::Keep)]
        color_mode: ColorModeArg,
        #[arg(long)]
        no_subset_fonts: bool,
        #[arg(long)]
        no_object_streams: bool,
        /// What to remove, comma-separated: thumbnails, private, attachments, javascript,
        /// tags; or none. Default: the preset's (thumbnails and private data).
        #[arg(long)]
        remove: Option<String>,
    },
    /// Page labels.
    Labels {
        #[command(subcommand)]
        action: LabelsAction,
    },
    /// Bookmarks.
    Outline {
        #[command(subcommand)]
        action: OutlineAction,
    },
    /// Combine PDFs into a new file.
    Merge {
        out: PathBuf,
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = BookmarksArg::Nest)]
        bookmarks: BookmarksArg,
        #[arg(long, value_enum, default_value_t = LabelsArg::Keep)]
        labels: LabelsArg,
        /// Pages of the result, in order (default: every page of every input). Items
        /// separated by commas: FILE:PAGE or FILE:FIRST-LAST, 1-based, with an optional
        /// `@DEGREES` rotation, e.g. `2:1-3,1:5@90,2:4`.
        #[arg(long)]
        pages: Option<String>,
    },
    /// Insert pages from another file, as the app does (one journal step through a
    /// session, then an incremental save).
    Insert {
        input: PathBuf,
        out: PathBuf,
        /// The file to take pages from.
        #[arg(long)]
        from: PathBuf,
        /// Insert before this page (1-based); the page count + 1 appends. Default: append.
        #[arg(long)]
        at: Option<usize>,
        /// Pages of `--from` to insert (1-based), e.g. `1-3,7`. Default: all.
        #[arg(long)]
        pages: Option<String>,
        #[arg(long, value_enum, default_value_t = BookmarksArg::Nest)]
        bookmarks: BookmarksArg,
        #[arg(long, value_enum, default_value_t = InsertLabelsArg::Keep)]
        labels: InsertLabelsArg,
    },
    /// Move, delete or extract pages, as the app does (through a session; moving and
    /// deleting are one journal step, then an incremental save).
    Pages {
        #[command(subcommand)]
        action: PagesAction,
    },
    /// Annotations.
    Annot {
        #[command(subcommand)]
        action: AnnotAction,
    },
    /// Print the links on a page: where each is (view space) and what it does.
    Links {
        input: PathBuf,
        /// Page number (1-based).
        #[arg(long, default_value_t = 1)]
        page: usize,
    },
    /// Render one page to PNG.
    Render {
        input: PathBuf,
        out: PathBuf,
        /// Page number (1-based).
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long, default_value_t = 1.5)]
        scale: f32,
        /// Draw page content only.
        #[arg(long)]
        no_annotations: bool,
    },
    /// Print a read-only profile of each file as one JSON line (for choosing test files).
    Survey {
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<PathBuf>,
    },
    /// Show the installed-font index (ADR 0005): size, build time, and what font names
    /// resolve to (`--find NAME[:bold][:italic]`, or every font a document does not
    /// embed with `--document FILE`).
    Fonts {
        #[arg(long = "find")]
        find: Vec<String>,
        #[arg(long)]
        document: Option<PathBuf>,
    },
    /// Crash recovery (section 7): turn page 1 in a session, write a recovery copy to OUT
    /// and time it, then restore the copy as INPUT's document (nothing is saved).
    Recovery { input: PathBuf, out: PathBuf },
    /// Measure open, render and encode times (AGENTS.md section 2 targets).
    Bench {
        input: PathBuf,
        #[arg(long, default_value_t = 1.5)]
        scale: f32,
        /// Pages to render, spread evenly through the document.
        #[arg(long, default_value_t = 20)]
        samples: usize,
    },
}

#[derive(Subcommand)]
enum LabelsAction {
    /// Replace the labels. Rule: PAGE:STYLE[:PREFIX[:START]], PAGE 1-based; STYLE one of
    /// decimal, roman-lower, roman-upper, letters-lower, letters-upper, none.
    Set {
        input: PathBuf,
        out: PathBuf,
        #[arg(long = "rule", required = true)]
        rules: Vec<String>,
    },
    /// Remove all labels.
    Clear { input: PathBuf, out: PathBuf },
    /// Set the labels as the app does (one journal step through a session, then an
    /// incremental save). Rules as for `set`; no rules removes the labels. Rules equal to
    /// the stored ones change nothing.
    Edit {
        input: PathBuf,
        out: PathBuf,
        #[arg(long = "rule")]
        rules: Vec<String>,
    },
}

#[derive(Subcommand)]
enum PagesAction {
    /// Move pages, keeping their order. Page labels stay with the positions.
    Move {
        input: PathBuf,
        out: PathBuf,
        /// Pages to move (1-based), e.g. `1-3,7`.
        #[arg(long)]
        pages: String,
        /// Move them before this page (1-based); the page count + 1 moves them to the end.
        #[arg(long)]
        before: usize,
    },
    /// Delete pages, with the links, bookmarks, named destinations and form fields that
    /// lead to them or were on them.
    Delete {
        input: PathBuf,
        out: PathBuf,
        /// Pages to delete (1-based), e.g. `1-3,7`.
        #[arg(long)]
        pages: String,
    },
    /// Copy pages into a new file, with their annotations, the links among them, and the
    /// bookmarks and labels they have. The input is not changed.
    Extract {
        input: PathBuf,
        out: PathBuf,
        /// Pages to copy (1-based), in this order, e.g. `1-3,7`.
        #[arg(long)]
        pages: String,
    },
}

#[derive(Subcommand)]
enum OutlineAction {
    /// Replace the outline. Item: LEVEL:PAGE:TITLE (LEVEL from 0, PAGE 1-based); append
    /// `+` to LEVEL to make the item open, e.g. `0+:1:Part I`.
    Set {
        input: PathBuf,
        out: PathBuf,
        #[arg(long = "item", required = true)]
        items: Vec<String>,
    },
    /// Print the outline with ids (object numbers) and targets.
    Show { input: PathBuf },
    /// Edit bookmarks in place, as the app does (one journal step per operation, then an
    /// incremental save). Operations, applied in order; PAGE is 1-based, X and Y are the
    /// view-space point (points from the page's top-left), PARENT is an id or `-` for the
    /// top level:
    ///   add:PARENT:INDEX:PAGE:X:Y:TITLE   rename:ID:TITLE   move:ID:PARENT:INDEX
    ///   delete:ID   retarget:ID:PAGE:X:Y   color:ID:RRGGBB|none   open:ID   close:ID
    Edit {
        input: PathBuf,
        out: PathBuf,
        #[arg(long = "op", required = true)]
        ops: Vec<String>,
    },
}

#[derive(Subcommand)]
enum AnnotAction {
    /// Mark up text: finds TEXT on PAGE and creates one annotation over every match, or
    /// marks up an area (--rect, one quad, as Alt-drag does in the app).
    Markup {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = KindArg::Highlight)]
        kind: KindArg,
        /// Page number (1-based).
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long, required_unless_present = "rect")]
        text: Option<String>,
        /// An area instead of text: x0,y0,x1,y1 in view space (points, top-left origin).
        #[arg(long, value_parser = parse_rect, conflicts_with = "text")]
        rect: Option<Rect>,
        /// RRGGBB.
        #[arg(long, default_value = "FFEB3B", value_parser = parse_color)]
        color: Rgb,
        #[arg(long, default_value_t = 1.0)]
        opacity: f32,
        #[arg(long, default_value = "Lectrix")]
        author: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// A sticky note whose icon's top-left corner is at X,Y (view space).
    Note {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long, value_parser = parse_point)]
        at: Point,
        #[arg(long, default_value = "")]
        text: String,
        #[command(flatten)]
        style: StyleArgs,
    },
    /// A reply to annotation --to (its object number), as the app writes it (ADR 0012).
    Reply {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long)]
        to: u32,
        #[arg(long)]
        text: String,
        #[arg(long, default_value = "Lectrix")]
        author: String,
    },
    /// A freehand drawing. Each --stroke is "x,y;x,y;..." in view space.
    Ink {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long = "stroke", required = true, value_parser = parse_stroke)]
        strokes: Vec<Vec<Point>>,
        #[arg(long, default_value_t = 2.0)]
        width: f64,
        #[command(flatten)]
        style: StyleArgs,
    },
    /// A text box at x0,y0,x1,y1 (view space); its height grows to fit the text.
    Text {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long, value_parser = parse_rect)]
        rect: Rect,
        #[arg(long)]
        text: String,
        #[arg(long, default_value_t = 12.0)]
        size: f64,
        /// How far the page is turned on screen where the box is typed, clockwise (0, 90,
        /// 180 or 270): the text stands upright as shown so.
        #[arg(long, default_value_t = 0)]
        turn: i32,
        #[command(flatten)]
        style: StyleArgs,
    },
    /// List every annotation with what needs repair.
    List { input: PathBuf },
    /// Change annotation ID (object number) on PAGE.
    Edit {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long)]
        id: u32,
        #[arg(long, value_parser = parse_color)]
        color: Option<Rgb>,
        #[arg(long)]
        opacity: Option<f32>,
        #[arg(long)]
        contents: Option<String>,
        #[arg(long)]
        author: Option<String>,
        /// Move or resize: x0,y0,x1,y1 in view space.
        #[arg(long, value_parser = parse_rect)]
        bounds: Option<Rect>,
        #[arg(long)]
        width: Option<f64>,
        #[arg(long)]
        size: Option<f64>,
        /// Turn text markup into another text-markup type.
        #[arg(long = "type", value_enum)]
        kind: Option<KindArg>,
    },
    /// Delete annotation ID on PAGE with its popup and replies; several (--id given more
    /// than once) in one undo step.
    Delete {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long = "id", required = true)]
        ids: Vec<u32>,
    },
    /// Scan for problems; with OUT, repair them and write the result there.
    Repair {
        input: PathBuf,
        out: Option<PathBuf>,
    },
}

#[derive(clap::Args)]
struct StyleArgs {
    /// RRGGBB.
    #[arg(long, default_value = "FFEB3B", value_parser = parse_color)]
    color: Rgb,
    #[arg(long, default_value_t = 1.0)]
    opacity: f32,
    #[arg(long, default_value = "Lectrix")]
    author: String,
}

#[derive(Clone, Copy, ValueEnum)]
enum CompareModeArg {
    Automatic,
    Text,
    Visual,
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    Highlight,
    Underline,
    Strikeout,
    Squiggly,
}

impl KindArg {
    fn markup_kind(self) -> MarkupKind {
        match self {
            KindArg::Highlight => MarkupKind::Highlight,
            KindArg::Underline => MarkupKind::Underline,
            KindArg::Strikeout => MarkupKind::StrikeOut,
            KindArg::Squiggly => MarkupKind::Squiggly,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PresetArg {
    Lossless,
    High,
    Balanced,
    Smallest,
}

#[derive(Clone, Copy, ValueEnum)]
enum MethodArg {
    Bicubic,
    Average,
}

#[derive(Clone, Copy, ValueEnum)]
enum CompressionArg {
    Keep,
    Jpeg,
    Jpeg2000,
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorModeArg {
    Keep,
    Gray,
    Bw,
}

#[derive(Clone, Copy, ValueEnum)]
enum SizingArg {
    Fit,
    Actual,
    Shrink,
    Scale,
}

#[derive(Clone, Copy, ValueEnum)]
enum OrientationArg {
    Auto,
    Portrait,
    Landscape,
}

#[derive(Clone, Copy, ValueEnum)]
enum DuplexArg {
    One,
    Long,
    Short,
}

#[derive(Clone, Copy, ValueEnum)]
enum PrintingArg {
    None,
    Low,
    High,
}

#[derive(Clone, Copy, ValueEnum)]
enum ChangesArg {
    None,
    Pages,
    Forms,
    Comments,
    Any,
}

#[derive(Clone, Copy, ValueEnum)]
enum BookmarksArg {
    Nest,
    Flat,
    Drop,
}

#[derive(Clone, Copy, ValueEnum)]
enum LabelsArg {
    Keep,
    Continuous,
    None,
}

#[derive(Clone, Copy, ValueEnum)]
enum InsertLabelsArg {
    /// The inserted pages keep their own labels.
    Keep,
    /// The inserted pages continue the numbering around them.
    Follow,
}

impl From<BookmarksArg> for BookmarkMode {
    fn from(b: BookmarksArg) -> Self {
        match b {
            BookmarksArg::Nest => BookmarkMode::NestUnderSource,
            BookmarksArg::Flat => BookmarkMode::Flat,
            BookmarksArg::Drop => BookmarkMode::Drop,
        }
    }
}

fn parse_removals(s: &str) -> Result<pdf_core::compress::Removals> {
    let mut removals = pdf_core::compress::Removals::default();
    for item in s.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        match item {
            "none" => {}
            "thumbnails" => removals.thumbnails = true,
            "private" => removals.private_data = true,
            "attachments" => removals.attachments = true,
            "javascript" => removals.javascript = true,
            "tags" => removals.tags = true,
            other => {
                return Err(Error::InvalidArgument(format!(
                    "unknown removal {other:?}: thumbnails, private, attachments, javascript, tags or none"
                )));
            }
        }
    }
    Ok(removals)
}

fn analyze(input: &Path, password: Option<&str>) -> Result<()> {
    let a = pdf_core::compress::analyze(input, password)?;
    let mb = |b: u64| b as f64 / 1_048_576.0;
    println!(
        "{}: {:.2} MB, {} pages",
        input.display(),
        mb(a.file_size),
        a.pages
    );
    let rows = [
        ("images", a.images),
        ("fonts", a.fonts),
        ("page content", a.content),
        ("attachments", a.attachments),
        ("thumbnails", a.thumbnails),
        ("private data", a.private_data),
        ("metadata", a.metadata),
    ];
    for (name, usage) in rows {
        if usage.count > 0 {
            println!(
                "  {name:<14} {:>9.2} MB  ({} objects)",
                mb(usage.bytes),
                usage.count
            );
        }
    }
    println!("  {:<14} {:>9.2} MB", "other", mb(a.other));
    let f = a.found;
    println!(
        "found: {} thumbnails, {} with private data, {} attachments, {} scripts, tags: {}",
        f.thumbnails,
        f.private_data,
        f.attachments,
        f.javascript,
        if f.tags { "yes" } else { "no" }
    );
    println!(
        "encrypted: {}, signed: {}, PDF/A: {}",
        a.encrypted,
        a.signed,
        a.pdfa.map(|p| p.to_string()).unwrap_or_else(|| "no".into())
    );
    Ok(())
}

/// What `pdf-cli print` was asked for.
struct CliJob {
    pages: Option<String>,
    sizing: pdf_core::print::Sizing,
    orientation: pdf_core::print::Orientation,
    paper: Option<i32>,
    annotations: bool,
    grey: bool,
    copies: u16,
    collate: bool,
    duplex: pdf_core::print::Duplex,
}

/// `pdf-cli print`: the job File > Print makes, on `printer`.
fn print_file(
    input: &Path,
    printer: &str,
    output: Option<PathBuf>,
    asked: &CliJob,
    password: Option<&str>,
) -> Result<()> {
    use pdf_core::print::{self, Job, PrintSetup};
    let (session, info) = Session::open(input, password)?;
    let max_dpi = print::allowed_dpi(info.flags.printing)?;
    let count = info.pages.len();
    let pages: Vec<usize> = match &asked.pages {
        None => (0..count).collect(),
        Some(text) => {
            let mut pages = Vec::new();
            for part in text.split(',') {
                let (from, to) = part.split_once('-').unwrap_or((part, part));
                let number = |s: &str| -> Result<usize> {
                    s.trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|&n| n >= 1 && n <= count)
                        .ok_or_else(|| Error::InvalidArgument(format!("no page {s}")))
                };
                pages.extend(number(from)? - 1..number(to)?);
            }
            pages
        }
    };
    let landscape = print::job_landscape(
        asked.orientation,
        pages
            .iter()
            .map(|&p| (info.pages[p].width, info.pages[p].height)),
    );
    let setup = PrintSetup {
        paper: asked.paper,
        landscape,
        duplex: asked.duplex,
        grey: asked.grey,
        copies: asked.copies,
        collate: asked.collate,
        title: input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        output,
        max_dpi,
    };
    let target = pdf_core::platform::printers().start(printer, &setup)?;
    let sheet = target.sheet();
    println!(
        "sheet {:.0} x {:.0} pt, printable {:.0},{:.0} {:.0} x {:.0}",
        sheet.width,
        sheet.height,
        sheet.printable.x,
        sheet.printable.y,
        sheet.printable.width,
        sheet.printable.height
    );
    let job = Job {
        pages,
        sizing: asked.sizing,
        orientation: asked.orientation,
        annotations: asked.annotations,
        grey: asked.grey,
        copies: asked.copies,
        collate: asked.collate,
        duplex: asked.duplex,
    };
    let start = Instant::now();
    let mut sheets = 0;
    print::print(&session, &job, target, &mut |_, total| {
        sheets = total;
        true
    })?;
    session.close();
    println!(
        "printed {sheets} sheets on {printer} in {:.2} s",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn main() -> ExitCode {
    // On a thread with room: the parser clap derives for this many commands takes more
    // than the 1 MB of stack Windows gives the main thread, in debug builds.
    let main = std::thread::Builder::new().stack_size(16 << 20).spawn(|| {
        let cli = Cli::parse();
        // The same font policy as the app, so renders and timings match what users see.
        pdf_core::fonts::install();
        match run(cli.command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        }
    });
    match main.map(|thread| thread.join()) {
        Ok(Ok(code)) => code,
        _ => ExitCode::FAILURE,
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Gen {
            out,
            pages,
            rotate,
            crop,
            user_unit,
            title,
            pdfa,
            password,
            owner_password,
            allow,
        } => {
            let doc = testgen::sample_document(&SampleSpec {
                pages,
                rotate,
                crop_box: crop,
                user_unit,
                title,
                pdfa,
                ..SampleSpec::default()
            })?;
            if password.is_some() || owner_password.is_some() {
                let mut options = PdfWriteOptions::default();
                options
                    .set_encryption(Encryption::Aes256)
                    .set_permissions(Permission::from_bits_truncate(allow))
                    .set_user_password(password.as_deref().unwrap_or(""))
                    .set_owner_password(owner_password.as_deref().unwrap_or(""));
                let out_str = out.to_str().ok_or_else(|| {
                    Error::InvalidArgument(format!("path is not valid Unicode: {}", out.display()))
                })?;
                doc.save_with_options(out_str, options)?;
            } else {
                save::save_atomic(&doc, SaveKind::Full, None, &out)?;
            }
            println!("wrote {} ({pages} pages)", out.display());
        }
        Command::Printers => {
            let printers = pdf_core::platform::printers();
            for entry in printers.list()? {
                let mark = if entry.default { " (default)" } else { "" };
                println!("{}{mark}", entry.name);
                match printers.capabilities(&entry.name) {
                    Ok(caps) => {
                        println!(
                            "  duplex: {}, colour: {}, paper: {:?}",
                            caps.duplex, caps.colour, caps.paper
                        );
                        for paper in caps.papers {
                            println!(
                                "  {:>5}  {} ({:.0} x {:.0} pt)",
                                paper.id, paper.name, paper.width, paper.height
                            );
                        }
                    }
                    Err(e) => println!("  capabilities: {e}"),
                }
            }
        }
        Command::Print {
            input,
            printer,
            output,
            pages,
            sizing,
            scale,
            orientation,
            paper,
            no_annotations,
            grey,
            copies,
            uncollated,
            duplex,
            password,
        } => {
            use pdf_core::print::{Duplex, Orientation, Sizing};
            let job = CliJob {
                pages,
                sizing: match sizing {
                    SizingArg::Fit => Sizing::Fit,
                    SizingArg::Actual => Sizing::ActualSize,
                    SizingArg::Shrink => Sizing::ShrinkOversized,
                    SizingArg::Scale => Sizing::Scale(scale / 100.0),
                },
                orientation: match orientation {
                    OrientationArg::Auto => Orientation::Auto,
                    OrientationArg::Portrait => Orientation::Portrait,
                    OrientationArg::Landscape => Orientation::Landscape,
                },
                paper,
                annotations: !no_annotations,
                grey,
                copies,
                collate: !uncollated,
                duplex: match duplex {
                    DuplexArg::One => Duplex::OneSided,
                    DuplexArg::Long => Duplex::LongEdge,
                    DuplexArg::Short => Duplex::ShortEdge,
                },
            };
            print_file(&input, &printer, output, &job, password.as_deref())?;
        }
        Command::Unlock {
            input,
            out,
            password,
        } => {
            pdf_core::unprotect::remove_protection(&input, password.as_deref(), &out)?;
            println!("wrote {} without a password or restrictions", out.display());
        }
        Command::Protect {
            input,
            out,
            password,
            open_password,
            permissions_password,
            printing,
            changes,
            copying,
        } => {
            use pdf_core::protection::{
                Changes, Printing, ProtectionSettings, Restrict, Restrictions,
            };
            let restrictions = Restrictions {
                printing: match printing {
                    PrintingArg::None => Printing::NotAllowed,
                    PrintingArg::Low => Printing::LowResolution,
                    PrintingArg::High => Printing::HighResolution,
                },
                changes: match changes {
                    ChangesArg::None => Changes::None,
                    ChangesArg::Pages => Changes::PageAssembly,
                    ChangesArg::Forms => Changes::FormsAndSigning,
                    ChangesArg::Comments => Changes::CommentsFormsAndSigning,
                    ChangesArg::Any => Changes::AnyExceptExtracting,
                },
                copying,
            };
            let settings = ProtectionSettings {
                open_password,
                restrict: permissions_password.map(|password| Restrict {
                    password,
                    restrictions,
                }),
            };
            let removed =
                pdf_core::protection::protect_file(&input, password.as_deref(), &settings, &out)?;
            println!("wrote {} protected", out.display());
            if let Some(level) = removed {
                println!("it no longer claims {level}");
            }
        }
        Command::Info { input } => info(&input)?,
        Command::Compare {
            old,
            new,
            mode,
            no_punctuation,
            no_one_letter,
            no_margins,
            annotate_old,
            annotate_new,
            report,
            json,
        } => compare_files(
            &old,
            &new,
            pdf_core::compare::Options {
                punctuation: !no_punctuation,
                one_letter: !no_one_letter,
                margins: !no_margins,
            },
            match mode {
                CompareModeArg::Automatic => pdf_core::compare::Mode::Automatic,
                CompareModeArg::Text => pdf_core::compare::Mode::Text,
                CompareModeArg::Visual => pdf_core::compare::Mode::Visual,
            },
            [annotate_old, annotate_new, report],
            json,
        )?,
        Command::GenImages { out } => {
            let doc = testgen::image_sample()?;
            save::save_atomic(&doc, SaveKind::Full, None, &out)?;
            println!("wrote {}", out.display());
            for (name, what) in testgen::IMAGE_SAMPLE_CASES {
                println!("  {name}: {what}");
            }
        }
        Command::GenText { out, header, pages } => {
            let pages: Vec<Vec<String>> = pages
                .iter()
                .map(|p| p.split('|').map(str::to_owned).collect())
                .collect();
            let doc = testgen::text_document(&header, &pages)?;
            save::save_atomic(&doc, SaveKind::Full, None, &out)?;
            println!("wrote {}", out.display());
        }
        Command::Analyze { input, password } => analyze(&input, password.as_deref())?,
        Command::Compress {
            input,
            out,
            password,
            preset,
            dpi,
            mono_dpi,
            above,
            method,
            compression,
            quality,
            color_mode,
            no_subset_fonts,
            no_object_streams,
            remove,
        } => {
            use pdf_core::compress::{self, ColorMode, Compression, Downsample, Preset, Resample};
            let mut options = match preset {
                PresetArg::Lossless => Preset::Lossless,
                PresetArg::High => Preset::HighQuality,
                PresetArg::Balanced => Preset::Balanced,
                PresetArg::Smallest => Preset::Smallest,
            }
            .options();
            let method = method.map(|m| match m {
                MethodArg::Bicubic => Resample::Bicubic,
                MethodArg::Average => Resample::Average,
            });
            let downsample = |to: u32, was: Option<Downsample>| {
                (to > 0).then(|| Downsample {
                    to_dpi: to,
                    above_dpi: (to as f32 * above).round() as u32,
                    method: method
                        .or(was.map(|d| d.method))
                        .unwrap_or(Resample::Bicubic),
                })
            };
            for settings in [&mut options.color, &mut options.gray] {
                if let Some(dpi) = dpi {
                    settings.downsample = downsample(dpi, settings.downsample);
                } else if let (Some(d), Some(m)) = (settings.downsample.as_mut(), method) {
                    d.method = m;
                }
                if let Some(c) = compression {
                    settings.compression = match c {
                        CompressionArg::Keep => Compression::Keep,
                        CompressionArg::Jpeg => Compression::Jpeg,
                        CompressionArg::Jpeg2000 => Compression::Jpeg2000,
                    };
                }
                if let Some(q) = quality {
                    settings.quality = q;
                }
            }
            if let Some(dpi) = mono_dpi {
                options.mono = downsample(dpi, options.mono);
            }
            options.color_mode = match color_mode {
                ColorModeArg::Keep => ColorMode::Keep,
                ColorModeArg::Gray => ColorMode::Gray,
                ColorModeArg::Bw => ColorMode::BlackAndWhite,
            };
            options.subset_fonts = !no_subset_fonts;
            options.object_streams = !no_object_streams;
            if let Some(remove) = remove {
                options.remove = parse_removals(&remove)?;
            }
            let started = Instant::now();
            // When each stage began, to say how long each took.
            let mut stages: Vec<(&str, Instant)> = Vec::new();
            let report =
                compress::compress_file(&input, password.as_deref(), &options, &out, &mut |p| {
                    let stage = match p {
                        compress::Progress::Reading { .. } => "reading pages",
                        compress::Progress::Images { .. } => "images",
                        compress::Progress::Fonts => "fonts and removals",
                        compress::Progress::Writing => "writing",
                    };
                    if stages.last().is_none_or(|(s, _)| *s != stage) {
                        stages.push((stage, Instant::now()));
                    }
                    true
                })?;
            let ended = Instant::now();
            let timings: Vec<String> = stages
                .iter()
                .enumerate()
                .map(|(i, (stage, at))| {
                    let until = stages.get(i + 1).map_or(ended, |(_, t)| *t);
                    format!("{stage} {:.2} s", (until - *at).as_secs_f64())
                })
                .collect();
            let percent = 100.0 - report.after as f64 * 100.0 / report.before.max(1) as f64;
            println!(
                "wrote {}: {} -> {} bytes ({percent:.1}% smaller) in {:.2} s",
                out.display(),
                report.before,
                report.after,
                started.elapsed().as_secs_f64()
            );
            let i = report.images;
            println!(
                "images: {} in all, {} replaced ({} downsampled), {} left as they were",
                i.total, i.replaced, i.downsampled, i.skipped
            );
            println!("fonts subset: {}", report.fonts_subset);
            println!("time: {}", timings.join(", "));
            let r = report.removed;
            println!(
                "removed: {} thumbnails, {} private data, {} attachments, {} scripts{}",
                r.thumbnails,
                r.private_data,
                r.attachments,
                r.javascript,
                if r.tags { ", tags" } else { "" }
            );
            if let Some(level) = report.pdfa_removed {
                println!("it no longer claims {level}");
            }
        }
        Command::Fonts { find, document } => {
            let mut find = find;
            if let Some(path) = document {
                find.extend(non_embedded_fonts(&open(&path)?)?);
            }
            fonts(&find);
        }
        Command::Survey { inputs } => {
            for input in inputs {
                let line = serde_json::to_string(&survey::survey(&input))
                    .map_err(|e| Error::InvalidArgument(e.to_string()))?;
                println!("{line}");
            }
        }
        Command::Labels { action } => match action {
            LabelsAction::Set { input, out, rules } => {
                let rules = rules
                    .iter()
                    .map(|r| parse_label_rule(r))
                    .collect::<Result<Vec<_>>>()?;
                edit(&input, &out, |doc| labels::write_rules(doc, rules))?;
            }
            LabelsAction::Clear { input, out } => {
                edit(&input, &out, |doc| labels::write_rules(doc, Vec::new()))?;
            }
            LabelsAction::Edit { input, out, rules } => {
                let rules = rules
                    .iter()
                    .map(|r| parse_label_rule(r))
                    .collect::<Result<Vec<_>>>()?;
                edit_in_session(&input, &out, vec![Operation::SetPageLabels { rules }])?;
            }
        },
        Command::Outline { action } => match action {
            OutlineAction::Set { input, out, items } => {
                let tree = parse_outline(&items)?;
                edit(&input, &out, |doc| outline::write_outline(doc, &tree))?;
            }
            OutlineAction::Show { input } => {
                let outline = outline::read_bookmarks(&open(&input)?)?;
                if outline.damaged {
                    println!("(damaged: shown as far as it can be read; not editable)");
                }
                print_bookmarks(&outline.items, 0);
            }
            OutlineAction::Edit { input, out, ops } => edit_outline(&input, &out, &ops)?,
        },
        Command::Merge {
            out,
            inputs,
            bookmarks,
            labels,
            pages,
        } => {
            if inputs.iter().any(|i| same_file(i, &out)) {
                return Err(Error::InvalidArgument(
                    "write to a new file; pdf-cli never modifies its input".into(),
                ));
            }
            let sources = inputs
                .iter()
                .map(|p| MergeSource::open(p, None))
                .collect::<Result<Vec<_>>>()?;
            let options = MergeOptions {
                bookmarks: bookmarks.into(),
                labels: match labels {
                    LabelsArg::Keep => LabelMode::KeepSources,
                    LabelsArg::Continuous => LabelMode::Continuous,
                    LabelsArg::None => LabelMode::None,
                },
            };
            let (merged, report) = match pages {
                Some(spec) => {
                    let picks = parse_picks(&spec, &sources)?;
                    merge::merge(&sources, &picks, options, &mut |_, _| true)?
                }
                None => merge::merge_all(&sources, options)?,
            };
            save::save_atomic(&merged, SaveKind::Full, None, &out)?;
            println!("wrote {} ({} pages)", out.display(), merged.page_count()?);
            print_report(&report);
        }
        Command::Insert {
            input,
            out,
            from,
            at,
            pages,
            bookmarks,
            labels,
        } => {
            let count = usize::try_from(open(&input)?.page_count()?).unwrap_or(0);
            let at = match at {
                Some(n) if n >= 1 && n <= count + 1 => n - 1,
                Some(n) => {
                    return Err(Error::InvalidArgument(format!(
                        "--at {n}: the document has {count} pages"
                    )));
                }
                None => count,
            };
            let from_count = usize::try_from(open(&from)?.page_count()?).unwrap_or(0);
            let pages = match pages {
                Some(spec) => parse_page_list(&spec, from_count)?,
                None => Vec::new(),
            };
            let op = Operation::InsertPages {
                source: InsertSource {
                    path: from,
                    password: None,
                    pages,
                },
                at,
                options: InsertOptions {
                    bookmarks: bookmarks.into(),
                    labels: match labels {
                        InsertLabelsArg::Keep => InsertLabels::KeepSource,
                        InsertLabelsArg::Follow => InsertLabels::FollowDocument,
                    },
                },
            };
            edit_in_session(&input, &out, vec![op])?;
        }
        Command::Pages {
            action: PagesAction::Extract { input, out, pages },
        } => {
            if same_file(&input, &out) {
                return Err(Error::InvalidArgument(
                    "write to a new file; pdf-cli never modifies its input".into(),
                ));
            }
            let (session, info) = Session::open(&input, None)?;
            let pages = parse_page_list(&pages, info.pages.len());
            let result = pages.and_then(|pages| session.extract(pages, &out, |_| true));
            session.close();
            let report = result?;
            println!("wrote {} ({} pages)", out.display(), report.pages);
            print_report(&report);
        }
        Command::Pages { action } => {
            let (input, out, pages, before) = match action {
                PagesAction::Move {
                    input,
                    out,
                    pages,
                    before,
                } => (input, out, pages, Some(before)),
                PagesAction::Delete { input, out, pages } => (input, out, pages, None),
                // Handled above.
                PagesAction::Extract { .. } => return Ok(()),
            };
            let count = usize::try_from(open(&input)?.page_count()?).unwrap_or(0);
            let pages = parse_page_list(&pages, count)?;
            let op = match before {
                Some(n) if n >= 1 && n <= count + 1 => Operation::MovePages { pages, to: n - 1 },
                Some(n) => {
                    return Err(Error::InvalidArgument(format!(
                        "--before {n}: the document has {count} pages"
                    )));
                }
                None => Operation::DeletePages { pages },
            };
            edit_in_session(&input, &out, vec![op])?;
        }
        Command::Annot {
            action:
                AnnotAction::Markup {
                    input,
                    out,
                    kind,
                    page,
                    text,
                    rect,
                    color,
                    opacity,
                    author,
                    note,
                },
        } => {
            let index = page.checked_sub(1).ok_or(Error::PageOutOfRange(0))?;
            edit(&input, &out, |doc| {
                let quads = match (&text, rect) {
                    (_, Some(rect)) => vec![Quad::from_view_rect(rect)],
                    (Some(text), None) => find_text(doc, index, text)?,
                    (None, None) => Vec::new(),
                };
                if quads.is_empty() {
                    return Err(Error::InvalidArgument(format!(
                        "\"{}\" not found on page {page}",
                        text.as_deref().unwrap_or_default()
                    )));
                }
                let spec = MarkupSpec {
                    kind: kind.markup_kind(),
                    page: index,
                    quads,
                    color,
                    opacity,
                    author: author.clone(),
                    note: note.clone(),
                };
                let created = annot::add_text_markup(doc, &spec)?;
                println!(
                    "created annotation {} (object {})",
                    created.name, created.xref
                );
                Ok(())
            })?;
        }
        Command::Annot { action } => annot_command(action)?,
        Command::Links { input, page } => {
            let doc = open(&input)?;
            let index = page.checked_sub(1).ok_or(Error::PageOutOfRange(0))?;
            let links = pdf_core::links::page_links(&doc, index)?;
            for link in &links {
                let [x0, y0, x1, y1] = link.rect;
                println!(
                    "[{x0:.1} {y0:.1} {x1:.1} {y1:.1}] -> {}",
                    describe_target(&link.target)
                );
            }
            println!("{} links on page {page}", links.len());
        }
        Command::Render {
            input,
            out,
            page,
            scale,
            no_annotations,
        } => {
            let doc = open(&input)?;
            let index = page.checked_sub(1).ok_or(Error::PageOutOfRange(0))?;
            let rendered = render::render_png_with(&doc, index, scale, !no_annotations)?;
            std::fs::write(&out, &rendered.png)?;
            println!(
                "{}x{} px, render {:.1} ms, encode {:.1} ms",
                rendered.width,
                rendered.height,
                ms(rendered.timings.render()),
                ms(rendered.timings.encode)
            );
        }
        Command::Bench {
            input,
            scale,
            samples,
        } => bench(&input, scale, samples)?,
        Command::Recovery { input, out } => recovery(&input, &out)?,
    }
    Ok(())
}

fn recovery(input: &Path, out: &Path) -> Result<()> {
    use pdf_core::session::{RecoveryWrite, Session};
    if same_file(input, out) {
        return Err(Error::InvalidArgument(
            "write to a new file; pdf-cli never modifies its input".into(),
        ));
    }
    let (session, info) = Session::open(input, None)?;
    session.apply(Operation::RotatePages {
        pages: vec![0],
        degrees: 90,
    })?;
    let t0 = Instant::now();
    let written = session.write_recovery(out, None)?;
    let write_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let RecoveryWrite::Written { revision } = written else {
        return Err(Error::InvalidArgument(format!(
            "no copy written: {written:?}"
        )));
    };
    let copy_len = std::fs::metadata(out)?.len();
    println!(
        "copy of revision {revision}: {copy_len} bytes in {write_ms:.0} ms ({}; input {} bytes)",
        if info.flags.repaired {
            "full copy, the input was repaired"
        } else {
            "snapshot"
        },
        std::fs::metadata(input)?.len()
    );
    session.close();
    let t0 = Instant::now();
    let (restored, info) = Session::restore(out, input, None)?;
    println!(
        "restored in {:.0} ms: {} pages, page 1 {}x{} pt, dirty {}",
        t0.elapsed().as_secs_f64() * 1000.0,
        info.pages.len(),
        info.pages.first().map_or(0.0, |p| p.width),
        info.pages.first().map_or(0.0, |p| p.height),
        info.state.dirty
    );
    restored.close();
    Ok(())
}

fn open(path: &Path) -> Result<PdfDocument> {
    let path_str = path.to_str().ok_or_else(|| {
        Error::InvalidArgument(format!("path is not valid Unicode: {}", path.display()))
    })?;
    let mut doc = PdfDocument::open(path_str)?;
    // As the app opens it: a cross-reference table that leads nowhere is rebuilt. A file
    // that stays without pages is shown as MuPDF reads it.
    let _ = pdf_core::session::repair_if_without_pages(&mut doc);
    Ok(doc)
}

/// Opens `input`, applies `f`, saves incrementally to `out` (a new file).
fn edit(input: &Path, out: &Path, f: impl FnOnce(&mut PdfDocument) -> Result<()>) -> Result<()> {
    if same_file(input, out) {
        return Err(Error::InvalidArgument(
            "write to a new file; pdf-cli never modifies its input".into(),
        ));
    }
    let mut doc = open(input)?;
    f(&mut doc)?;
    let outcome = save::save_atomic(&doc, SaveKind::Incremental, Some(input), out)?;
    if outcome.fell_back_to_full {
        println!("note: the input cannot be saved incrementally; wrote a full file instead");
    }
    println!("wrote {}", out.display());
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Parses `--pages` for `merge`: FILE:PAGE[-LAST][@DEGREES], comma separated, 1-based.
fn parse_picks(spec: &str, sources: &[MergeSource]) -> Result<Vec<PagePick>> {
    let bad = |item: &str| Error::InvalidArgument(format!("bad page item {item:?}"));
    let mut picks = Vec::new();
    for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (body, rotate) = match item.split_once('@') {
            Some((b, r)) => (b, r.parse::<i32>().map_err(|_| bad(item))?),
            None => (item, 0),
        };
        let (file, range) = body.split_once(':').ok_or_else(|| bad(item))?;
        let file: usize = file.parse().map_err(|_| bad(item))?;
        let source = file
            .checked_sub(1)
            .filter(|&s| s < sources.len())
            .ok_or_else(|| bad(item))?;
        let count = usize::try_from(sources[source].doc.page_count()?).unwrap_or(0);
        for page in parse_page_list(range, count)? {
            picks.push(PagePick {
                source,
                page,
                rotate,
            });
        }
    }
    Ok(picks)
}

/// Parses `1-3,7` (1-based, inclusive) into 0-based page indices, in order.
fn parse_page_list(spec: &str, count: usize) -> Result<Vec<usize>> {
    let bad = || {
        Error::InvalidArgument(format!(
            "bad page list {spec:?} (the file has {count} pages)"
        ))
    };
    let mut pages = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (first, last) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let first: usize = first.parse().map_err(|_| bad())?;
        let last: usize = last.parse().map_err(|_| bad())?;
        if first == 0 || last < first || last > count {
            return Err(bad());
        }
        pages.extend(first - 1..last);
    }
    Ok(pages)
}

fn print_report(report: &MergeReport) {
    if report.renamed_destinations > 0 {
        println!(
            "renamed {} named destination(s) that another file already used",
            report.renamed_destinations
        );
    }
    if report.renamed_attachments > 0 {
        println!(
            "renamed {} attached file(s) that another file already used",
            report.renamed_attachments
        );
    }
    if report.renamed_fields > 0 {
        println!(
            "renamed {} form field(s) that another file already used",
            report.renamed_fields
        );
    }
    if report.dropped_links > 0 {
        println!(
            "left out {} link(s) to pages that were not included",
            report.dropped_links
        );
    }
    if report.dropped_bookmarks > 0 {
        println!(
            "left out {} bookmark(s) to pages that were not included",
            report.dropped_bookmarks
        );
    }
    if report.bookmarks_skipped {
        println!("the document's bookmarks are damaged, so none were added");
    }
}

fn find_text(doc: &PdfDocument, index: usize, needle: &str) -> Result<Vec<Quad>> {
    let page_no = i32::try_from(index).map_err(|_| Error::PageOutOfRange(index))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(index));
    }
    let page = doc.load_page(page_no)?;
    let text = page.to_text_page(TextPageFlags::empty())?;
    Ok(text.search(needle)?.iter().map(Quad::from).collect())
}

fn info(path: &Path) -> Result<()> {
    let doc = open(path)?;
    let count = usize::try_from(doc.page_count()?).unwrap_or(0);
    println!("pages: {count}");
    if let Some(level) = docinfo::read_pdfa(&doc) {
        println!("claims: {level}");
    }
    let flags = docinfo::read_flags(&doc)?;
    if flags.encrypted {
        let allowed: Vec<&str> = [
            (flags.can_annotate, "annotating"),
            (flags.can_assemble, "changing pages"),
            (flags.can_copy, "copying"),
        ]
        .into_iter()
        .filter_map(|(yes, what)| yes.then_some(what))
        .collect();
        println!(
            "encrypted; allows: {}",
            if allowed.is_empty() {
                "none of annotating, changing pages, copying".into()
            } else {
                allowed.join(", ")
            }
        );
    }
    let mut rotated = Vec::new();
    for p in 0..count {
        let obj = doc.find_page(i32::try_from(p).unwrap_or(i32::MAX))?;
        let rotation =
            pdf_core::geometry::PageGeometry::new(&pdf_core::geometry::read_page_boxes(&obj)?)
                .rotation;
        if rotation != 0 {
            rotated.push(format!("{}:{rotation}", p + 1));
        }
    }
    if !rotated.is_empty() {
        println!("rotated pages (page:degrees): {}", rotated.join(" "));
    }
    let rules = labels::read_rules(&doc)?;
    if rules.is_empty() {
        println!("labels: none");
    } else {
        println!("labels:");
        for r in &rules {
            println!(
                "  from page {}: {:?} prefix {:?} start {}",
                r.start_page + 1,
                r.style,
                r.prefix,
                r.first_number
            );
        }
        let preview: Vec<String> = (0..count.min(12))
            .map(|p| labels::label_for_page(&rules, p))
            .collect();
        println!("  first labels: {}", preview.join(" "));
    }
    let tree = outline::read_outline(&doc)?;
    println!("outline: {}", if tree.is_empty() { "none" } else { "" });
    print_outline(&tree, 1);
    for p in 0..count {
        let page = doc.load_pdf_page(i32::try_from(p).unwrap_or(i32::MAX))?;
        for a in page.annotations() {
            println!(
                "annotation p{}: {:?} object {} author {:?}",
                p + 1,
                a.r#type()?,
                a.xref()?,
                a.author()?.unwrap_or("")
            );
        }
    }
    Ok(())
}

/// One `outline edit` step: an operation, or an expanded state (written at save).
enum OutlineStep {
    Op(Operation),
    Open(u32, bool),
}

fn parse_outline_step(spec: &str) -> Result<OutlineStep> {
    let bad = || Error::InvalidArgument(format!("bad operation {spec:?}"));
    let id = |s: &str| s.parse::<u32>().map_err(|_| bad());
    let parent = |s: &str| if s == "-" { Ok(None) } else { id(s).map(Some) };
    let number = |s: &str| s.parse::<f64>().map_err(|_| bad());
    let page = |s: &str| {
        s.parse::<usize>()
            .ok()
            .and_then(|p| p.checked_sub(1))
            .ok_or_else(bad)
    };
    let (kind, rest) = spec.split_once(':').ok_or_else(bad)?;
    Ok(match kind {
        "add" => {
            let f: Vec<&str> = rest.splitn(6, ':').collect();
            let [p, index, pg, x, y, title] = f.as_slice() else {
                return Err(bad());
            };
            OutlineStep::Op(Operation::AddBookmark {
                parent: parent(p)?,
                index: index.parse().map_err(|_| bad())?,
                title: (*title).to_owned(),
                dest: ViewDest {
                    page: page(pg)?,
                    x: number(x)?,
                    y: number(y)?,
                },
            })
        }
        "rename" => {
            let (i, title) = rest.split_once(':').ok_or_else(bad)?;
            OutlineStep::Op(Operation::RenameBookmark {
                id: id(i)?,
                title: title.to_owned(),
            })
        }
        "move" => {
            let f: Vec<&str> = rest.split(':').collect();
            let [i, p, index] = f.as_slice() else {
                return Err(bad());
            };
            OutlineStep::Op(Operation::MoveBookmark {
                id: id(i)?,
                parent: parent(p)?,
                index: index.parse().map_err(|_| bad())?,
            })
        }
        "delete" => OutlineStep::Op(Operation::DeleteBookmark { id: id(rest)? }),
        "retarget" => {
            let f: Vec<&str> = rest.split(':').collect();
            let [i, pg, x, y] = f.as_slice() else {
                return Err(bad());
            };
            OutlineStep::Op(Operation::SetBookmarkDestination {
                id: id(i)?,
                dest: ViewDest {
                    page: page(pg)?,
                    x: number(x)?,
                    y: number(y)?,
                },
            })
        }
        "color" => {
            let (i, c) = rest.split_once(':').ok_or_else(bad)?;
            let color = if c == "none" {
                None
            } else {
                let hex = c.strip_prefix('#').unwrap_or(c);
                let ch = |at: usize| {
                    hex.get(at..at + 2)
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                        .map(|v| f32::from(v) / 255.0)
                        .ok_or_else(bad)
                };
                if hex.len() != 6 {
                    return Err(bad());
                }
                Some([ch(0)?, ch(2)?, ch(4)?])
            };
            OutlineStep::Op(Operation::SetBookmarkColor { id: id(i)?, color })
        }
        "open" => OutlineStep::Open(id(rest)?, true),
        "close" => OutlineStep::Open(id(rest)?, false),
        _ => return Err(bad()),
    })
}

/// Applies `ops` through a session, as the app does, and saves to `out`.
fn edit_in_session(input: &Path, out: &Path, ops: Vec<Operation>) -> Result<()> {
    if same_file(input, out) {
        return Err(Error::InvalidArgument(
            "write to a new file; pdf-cli never modifies its input".into(),
        ));
    }
    let (session, info) = Session::open(input, None)?;
    let mut revision = info.state.revision;
    let result = (|| {
        for op in ops {
            let name = op.name();
            let change = session.apply(op)?;
            if change.state.revision == revision {
                println!("{name}: nothing changed");
            } else {
                println!("{name}");
            }
            if let Some(report) = &change.merge_report {
                print_report(report);
            }
            if let Some(deleted) = &change.deleted {
                println!(
                    "deleted {} page(s), with {} link(s), {} bookmark(s) and {} form field(s)",
                    deleted.pages, deleted.links, deleted.bookmarks, deleted.fields
                );
            }
            revision = change.state.revision;
        }
        session.save(SaveKind::Incremental, Some(out.to_path_buf()))
    })();
    session.close();
    let saved = result?;
    if saved.outcome.fell_back_to_full {
        println!("note: the input cannot be saved incrementally; wrote a full file instead");
    }
    println!("wrote {}", out.display());
    Ok(())
}

fn edit_outline(input: &Path, out: &Path, specs: &[String]) -> Result<()> {
    if same_file(input, out) {
        return Err(Error::InvalidArgument(
            "write to a new file; pdf-cli never modifies its input".into(),
        ));
    }
    let steps = specs
        .iter()
        .map(|s| parse_outline_step(s))
        .collect::<Result<Vec<_>>>()?;
    let (session, _) = Session::open(input, None)?;
    let result = (|| {
        for step in steps {
            match step {
                OutlineStep::Op(op) => {
                    let name = op.name();
                    let change = session.apply(op)?;
                    match change.created {
                        Some(id) => println!("{name}: id {id}"),
                        None => println!("{name}"),
                    }
                }
                OutlineStep::Open(id, open) => session.set_bookmark_open(id, open)?,
            }
        }
        session.save(SaveKind::Incremental, Some(out.to_path_buf()))
    })();
    session.close();
    let saved = result?;
    if saved.outcome.fell_back_to_full {
        println!("note: the input cannot be saved incrementally; wrote a full file instead");
    }
    println!("wrote {}", out.display());
    Ok(())
}

fn print_bookmarks(items: &[Bookmark], depth: usize) {
    for b in items {
        let marker = if b.children.is_empty() {
            " "
        } else if b.open {
            "-"
        } else {
            "+"
        };
        let color = b
            .color
            .map(|c| {
                let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                format!(" (colour #{r:02x}{g:02x}{b:02x})")
            })
            .unwrap_or_default();
        println!(
            "{}{marker} [{}] {} -> {}{color}",
            "  ".repeat(depth + 1),
            b.id,
            b.title,
            describe_target(&b.target)
        );
        print_bookmarks(&b.children, depth + 1);
    }
}

/// What a bookmark or a link does, in a few words.
fn describe_target(target: &Target) -> String {
    match target {
        Target::None => "no target".to_owned(),
        Target::Page { page, x, y, named } => {
            let at = |v: &Option<f32>| v.map_or("-".to_owned(), |v| format!("{v}"));
            let name = named
                .as_ref()
                .map(|n| format!(" (named {n:?})"))
                .unwrap_or_default();
            format!("page {} at {},{}{name}", page + 1, at(x), at(y))
        }
        Target::Broken { named } => format!("broken destination {named:?}"),
        Target::Uri(u) => format!("link {u}"),
        Target::File(f) => format!("file {f}"),
        Target::Action(a) => format!("action {a}"),
    }
}

fn print_outline(items: &[outline::ReadOutlineItem], depth: usize) {
    for item in items {
        let page = item
            .page
            .map(|p| (p + 1).to_string())
            .unwrap_or_else(|| "-".into());
        let marker = if item.children.is_empty() {
            " "
        } else if item.open {
            "-"
        } else {
            "+"
        };
        println!(
            "{}{marker} {} (page {page})",
            "  ".repeat(depth),
            item.title
        );
        print_outline(&item.children, depth + 1);
    }
}

/// Names of the fonts that page resources use without embedding them (font dictionaries
/// directly in each page's resources; fonts inside form XObjects are not visited).
fn non_embedded_fonts(doc: &PdfDocument) -> Result<Vec<String>> {
    let mut names = std::collections::BTreeSet::new();
    for p in 0..doc.page_count()? {
        let page = doc.find_page(p)?;
        let Some(fonts) = page
            .get_dict_inheritable("Resources")?
            .and_then(|r| r.get_dict("Font").ok().flatten())
        else {
            continue;
        };
        for i in 0..i32::try_from(fonts.dict_len()?).unwrap_or(0) {
            let Some(font) = fonts.get_dict_val(i)? else {
                continue;
            };
            let described = match font.get_dict("DescendantFonts")? {
                Some(d) => d.get_array(0)?.unwrap_or(font.clone()),
                None => font.clone(),
            };
            let embedded = match described.get_dict("FontDescriptor")? {
                Some(fd) => {
                    fd.get_dict("FontFile")?.is_some()
                        || fd.get_dict("FontFile2")?.is_some()
                        || fd.get_dict("FontFile3")?.is_some()
                }
                None => false,
            };
            let is_type3 = font
                .get_dict("Subtype")?
                .is_some_and(|s| s.as_name().unwrap_or_default() == b"Type3");
            if embedded || is_type3 {
                continue;
            }
            if let Some(name) = font.get_dict("BaseFont")? {
                let name = String::from_utf8_lossy(&name.as_name()?).into_owned();
                let bare = match name.split_once('+') {
                    Some((prefix, rest)) if prefix.len() == 6 => rest.to_owned(),
                    _ => name,
                };
                names.insert(bare);
            }
        }
    }
    Ok(names.into_iter().collect())
}

fn fonts(queries: &[String]) {
    let index = pdf_core::fonts::installed_fonts();
    let built = pdf_core::fonts::index_build_time().unwrap_or_default();
    println!(
        "installed faces: {} (index built in {:.1} ms)",
        index.len(),
        built.as_secs_f64() * 1000.0
    );
    for query in queries {
        let mut parts = query.split(':');
        let name = parts.next().unwrap_or_default();
        let flags: Vec<&str> = parts.collect();
        let (bold, italic) = (flags.contains(&"bold"), flags.contains(&"italic"));
        let found = match pdf_core::fonts::builtin_for(name) {
            Some(builtin) => format!("built-in {builtin}"),
            None => match index.lookup(name, bold, italic) {
                Some(f) => format!(
                    "{} ({}, weight {}, {:?}) in {} #{}",
                    f.postscript_name.as_deref().unwrap_or("?"),
                    f.family_name(),
                    f.weight,
                    f.style,
                    f.path.display(),
                    f.index
                ),
                None => "not installed (MuPDF substitutes a font)".into(),
            },
        };
        println!("{query}: {found}");
    }
}

fn bench(path: &Path, scale: f32, samples: usize) -> Result<()> {
    let t0 = Instant::now();
    let doc = open(path)?;
    let count = usize::try_from(doc.page_count()?).unwrap_or(0);
    let opened = t0.elapsed();
    let first = render::render_png(&doc, 0, scale)?;
    let first_total = t0.elapsed();
    println!("pages: {count}");
    println!("open: {:.1} ms", ms(opened));
    println!(
        "first page: {:.1} ms from open start (display list {:.1}, raster {:.1}, encode {:.1}), {}x{} px, {} KiB",
        ms(first_total),
        ms(first.timings.display_list),
        ms(first.timings.raster),
        ms(first.timings.encode),
        first.width,
        first.height,
        first.png.len() / 1024
    );
    let samples = samples.clamp(1, count.max(1));
    let (mut render_sum, mut encode_sum, mut worst) = (0.0, 0.0, 0.0f64);
    for i in 0..samples {
        let page = i * count / samples;
        let r = render::render_png(&doc, page, scale)?;
        let (render_ms, encode_ms) = (ms(r.timings.render()), ms(r.timings.encode));
        render_sum += render_ms;
        encode_sum += encode_ms;
        worst = worst.max(render_ms + encode_ms);
    }
    let n = samples as f64;
    println!(
        "{samples} pages: mean render {:.1} ms, mean encode {:.1} ms ({:.0}% of render), worst total {:.1} ms",
        render_sum / n,
        encode_sum / n,
        100.0 * encode_sum / render_sum.max(f64::EPSILON),
        worst
    );
    Ok(())
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn parse_point(s: &str) -> std::result::Result<Point, String> {
    let v: Vec<f64> = s
        .split(',')
        .map(|p| p.trim().parse::<f64>().map_err(|e| e.to_string()))
        .collect::<std::result::Result<_, _>>()?;
    match v.as_slice() {
        [x, y] => Ok(Point::new(*x, *y)),
        _ => Err("expected x,y".into()),
    }
}

fn parse_stroke(s: &str) -> std::result::Result<Vec<Point>, String> {
    s.split(';')
        .filter(|p| !p.trim().is_empty())
        .map(parse_point)
        .collect()
}

/// `annot` subcommands other than `markup`: through a session, as the app does.
fn annot_command(action: AnnotAction) -> Result<()> {
    let page_index = |page: usize| page.checked_sub(1).ok_or(Error::PageOutOfRange(0));
    let add = |input: &Path, out: &Path, page: usize, body: Body, style: StyleArgs| {
        let op = Operation::AddAnnotation {
            annotation: NewAnnotation {
                page: page_index(page)?,
                body,
                color: style.color,
                opacity: style.opacity,
                author: style.author,
            },
        };
        edit_in_session(input, out, vec![op])
    };
    match action {
        AnnotAction::Markup { .. } => Err(Error::InvalidArgument(
            "markup is handled before this".into(),
        )),
        AnnotAction::Note {
            input,
            out,
            page,
            at,
            text,
            style,
        } => add(&input, &out, page, Body::Note { at, text }, style),
        AnnotAction::Reply {
            input,
            out,
            page,
            to,
            text,
            author,
        } => {
            let op = Operation::AddReply {
                page: page_index(page)?,
                parent: to,
                text,
                author,
            };
            edit_in_session(&input, &out, vec![op])
        }
        AnnotAction::Ink {
            input,
            out,
            page,
            strokes,
            width,
            style,
        } => add(&input, &out, page, Body::Ink { strokes, width }, style),
        AnnotAction::Text {
            input,
            out,
            page,
            rect,
            text,
            size,
            turn,
            style,
        } => add(
            &input,
            &out,
            page,
            Body::FreeText {
                rect,
                text,
                font_size: size,
                turn,
            },
            style,
        ),
        AnnotAction::List { input } => list_annotations(&input),
        AnnotAction::Edit {
            input,
            out,
            page,
            id,
            color,
            opacity,
            contents,
            author,
            bounds,
            width,
            size,
            kind,
        } => {
            let op = Operation::UpdateAnnotation {
                page: page_index(page)?,
                id,
                edit: AnnotationEdit {
                    color,
                    opacity,
                    contents,
                    author,
                    bounds,
                    width,
                    font_size: size,
                    kind: kind.map(KindArg::markup_kind),
                },
            };
            edit_in_session(&input, &out, vec![op])
        }
        AnnotAction::Delete {
            input,
            out,
            page,
            ids,
        } => {
            let page = page_index(page)?;
            let op = match ids.as_slice() {
                [id] => Operation::DeleteAnnotation { page, id: *id },
                _ => Operation::DeleteAnnotations {
                    annotations: ids.iter().map(|&id| (page, id)).collect(),
                },
            };
            edit_in_session(&input, &out, vec![op])
        }
        AnnotAction::Repair { input, out } => repair_annotations(&input, out.as_deref()),
    }
}

fn list_annotations(input: &Path) -> Result<()> {
    let (session, info) = Session::open(input, None)?;
    session.close();
    for page in &info.annotations {
        for a in page {
            let r = a.rect;
            let mut line = format!(
                "p{} object {} {} rect [{:.1} {:.1} {:.1} {:.1}] author {:?}",
                a.page + 1,
                a.id,
                a.subtype,
                r.x0,
                r.y0,
                r.x1,
                r.y1,
                a.author
            );
            if !a.contents.is_empty() {
                let short: String = a.contents.chars().take(40).collect();
                line.push_str(&format!(" text {short:?}"));
            }
            if a.turn != 0 {
                line.push_str(&format!(" turned {}", a.turn));
            }
            if let Some(parent) = a.reply_to {
                line.push_str(&format!(" reply-to {parent}"));
            }
            if !a.problems.is_empty() {
                line.push_str(&format!(" needs-repair {:?}", a.problems));
            }
            println!("{line}");
        }
    }
    Ok(())
}

fn repair_annotations(input: &Path, out: Option<&Path>) -> Result<()> {
    if out.is_some_and(|o| same_file(input, o)) {
        return Err(Error::InvalidArgument(
            "write to a new file; pdf-cli never modifies its input".into(),
        ));
    }
    let (session, _) = Session::open(input, None)?;
    let result = (|| {
        let found = session.scan_annotations()?;
        if found.is_clean() {
            println!("no problems found");
        }
        for (problem, count) in &found.counts {
            println!("{problem:?}: {count}");
        }
        println!(
            "annotations to repair: {}, with problems repair can't fix: {}",
            found.fixable, found.unfixable
        );
        let Some(out) = out else {
            return Ok(());
        };
        let change = session.apply(Operation::RepairAnnotations)?;
        for c in change.repairs.iter().flatten() {
            println!("{c}");
        }
        session.save(SaveKind::Incremental, Some(out.to_path_buf()))?;
        println!("wrote {}", out.display());
        Ok(())
    })();
    session.close();
    result
}

fn parse_rect(s: &str) -> std::result::Result<Rect, String> {
    let v: Vec<f64> = s
        .split(',')
        .map(|p| p.trim().parse::<f64>().map_err(|e| e.to_string()))
        .collect::<std::result::Result<_, _>>()?;
    match v.as_slice() {
        [x0, y0, x1, y1] => Ok(Rect::new(*x0, *y0, *x1, *y1)),
        _ => Err("expected x0,y0,x1,y1".into()),
    }
}

/// A PDF/A level as it is usually written after "PDF/A-": 1b, 2u, 3a, 4, 4f.
/// The bits of `/P` for `gen --allow`.
fn parse_permissions(s: &str) -> std::result::Result<u32, String> {
    let mut allowed = Permission::empty();
    for name in s.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        allowed |= match name {
            "none" => Permission::empty(),
            "print" => Permission::PRINT,
            "print-hq" => Permission::PRINT_HQ,
            "modify" => Permission::MODIFY,
            "copy" => Permission::COPY,
            "annotate" => Permission::ANNOTATE,
            "form" => Permission::FORM,
            "accessibility" => Permission::ACCESSIBILITY,
            "assemble" => Permission::ASSEMBLE,
            other => return Err(format!("unknown permission: {other}")),
        };
    }
    Ok(allowed.bits())
}

fn parse_pdfa(s: &str) -> std::result::Result<PdfA, String> {
    let bad = || "expected a part and a conformance level, such as 1b, 2u or 4".to_string();
    let mut chars = s.trim().chars();
    let part = chars
        .next()
        .and_then(|c| c.to_digit(10))
        .and_then(|d| u8::try_from(d).ok())
        .filter(|&d| d >= 1)
        .ok_or_else(bad)?;
    let conformance = chars.next().map(|c| c.to_ascii_uppercase());
    if chars.next().is_some() || conformance.is_some_and(|c| !c.is_ascii_alphabetic()) {
        return Err(bad());
    }
    Ok(PdfA { part, conformance })
}

fn parse_color(s: &str) -> std::result::Result<Rgb, String> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return Err("expected RRGGBB".into());
    }
    let channel = |i: usize| {
        u8::from_str_radix(&s[i..i + 2], 16)
            .map(|v| f32::from(v) / 255.0)
            .map_err(|e| e.to_string())
    };
    Ok(Rgb {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
    })
}

fn parse_label_rule(s: &str) -> Result<LabelRule> {
    let bad = || {
        Error::InvalidArgument(format!(
            "bad label rule \"{s}\"; expected PAGE:STYLE[:PREFIX[:START]]"
        ))
    };
    let mut parts = s.splitn(4, ':');
    let page: usize = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
    let style = match parts.next().ok_or_else(bad)? {
        "decimal" => LabelStyle::Decimal,
        "roman-lower" => LabelStyle::LowerRoman,
        "roman-upper" => LabelStyle::UpperRoman,
        "letters-lower" => LabelStyle::LowerLetters,
        "letters-upper" => LabelStyle::UpperLetters,
        "none" => LabelStyle::None,
        _ => return Err(bad()),
    };
    let prefix = parts.next().unwrap_or("").to_owned();
    let first_number = match parts.next() {
        Some(n) => n.parse().map_err(|_| bad())?,
        None => 1,
    };
    Ok(LabelRule {
        start_page: page.checked_sub(1).ok_or_else(bad)?,
        style,
        prefix,
        first_number,
    })
}

/// Builds a tree from `LEVEL[+]:PAGE:TITLE` lines in document order.
fn parse_outline(lines: &[String]) -> Result<Vec<OutlineItem>> {
    let mut flat = Vec::new();
    for line in lines {
        let bad = || {
            Error::InvalidArgument(format!(
                "bad outline item \"{line}\"; expected LEVEL:PAGE:TITLE"
            ))
        };
        let mut parts = line.splitn(3, ':');
        let level_part = parts.next().ok_or_else(bad)?;
        let open = level_part.ends_with('+');
        let level: usize = level_part
            .trim_end_matches('+')
            .parse()
            .map_err(|_| bad())?;
        let page: usize = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
        let title = parts.next().ok_or_else(bad)?.to_owned();
        let page = page.checked_sub(1).ok_or_else(bad)?;
        flat.push((
            level,
            OutlineItem {
                title,
                target: OutlineTarget::Xyz {
                    page,
                    left: None,
                    top: None,
                },
                open,
                children: Vec::new(),
            },
        ));
    }
    let mut pos = 0;
    let tree = build_level(&mut flat.into_iter().peekable(), 0, &mut pos)?;
    Ok(tree)
}

fn build_level(
    items: &mut std::iter::Peekable<std::vec::IntoIter<(usize, OutlineItem)>>,
    level: usize,
    pos: &mut usize,
) -> Result<Vec<OutlineItem>> {
    let mut out: Vec<OutlineItem> = Vec::new();
    while let Some((l, _)) = items.peek() {
        let l = *l;
        if l < level {
            break;
        }
        if l > level {
            let parent = out.last_mut().ok_or_else(|| {
                Error::InvalidArgument(format!("outline item {} skips a level", *pos + 1))
            })?;
            if l != level + 1 {
                return Err(Error::InvalidArgument(format!(
                    "outline item {} skips a level",
                    *pos + 1
                )));
            }
            parent.children = build_level(items, level + 1, pos)?;
            continue;
        }
        if let Some((_, item)) = items.next() {
            *pos += 1;
            out.push(item);
        }
    }
    Ok(out)
}

fn compare_files(
    old: &Path,
    new: &Path,
    options: pdf_core::compare::Options,
    mode: pdf_core::compare::Mode,
    [annotate_old, annotate_new, report]: [Option<PathBuf>; 3],
    json: bool,
) -> Result<()> {
    use pdf_core::compare::annotate::{self, Which};
    use pdf_core::compare::{self, Kind};
    let start = Instant::now();
    let (a, _) = Session::open(old, None)?;
    let (b, _) = Session::open(new, None)?;
    let result = compare::run(&a, &b, options, mode, &|_| {}, &|| false);
    a.close();
    b.close();
    let c = result?;
    let elapsed = start.elapsed();
    let name = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    for (n, d) in c.differences.iter().enumerate() {
        let kind = match d.kind {
            Kind::Inserted => "inserted",
            Kind::Deleted => "deleted",
            Kind::Changed => "changed",
        };
        let group = format!("{:?}", d.group).to_lowercase();
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "kind": kind,
                    "group": group,
                    "oldPage": d.old.page() + 1,
                    "newPage": d.new.page() + 1,
                    "old": d.old.text,
                    "new": d.new.text,
                })
            );
        } else {
            println!(
                "{:>4}. {kind} ({group}), page {} -> {}: {:?} -> {:?}",
                n + 1,
                d.old.page() + 1,
                d.new.page() + 1,
                d.old.text,
                d.new.text
            );
        }
    }
    if !json {
        println!(
            "{} differences; {} and {} words; {:.0} ms",
            c.differences.len(),
            c.old_words,
            c.new_words,
            ms(elapsed)
        );
    }
    let all: Vec<&compare::Difference> = c.differences.iter().collect();
    for (target, which, source, other) in [
        (annotate_old, Which::Old, old, name(new)),
        (annotate_new, Which::New, new, name(old)),
    ] {
        if let Some(target) = target {
            let marks = annotate::annotations(&all, which, &other, "Lectrix");
            annotate::write_copy(source, None, &target, marks)?;
            println!("wrote {}", target.display());
        }
    }
    if let Some(target) = report {
        let page = |p: usize| (p + 1).to_string();
        let info = compare::report::ReportInfo {
            old_name: &name(old),
            new_name: &name(new),
            when: "",
            options,
            grouped: true,
            old_page: &page,
            new_page: &page,
        };
        compare::report::write(&target, &all, &info)?;
        println!("wrote {}", target.display());
    }
    Ok(())
}
