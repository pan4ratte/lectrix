//! `pdf-cli`: every pdf-core operation from the command line, for tests, the interop
//! harness, and reproducing engine bugs without the UI.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

mod survey;

use clap::{Parser, Subcommand, ValueEnum};
use mupdf::pdf::PdfDocument;
use mupdf::text_page::TextPageFlags;

use pdf_core::annot::quads::Quad;
use pdf_core::annot::{self, AnnotationEdit, Body, MarkupKind, MarkupSpec, NewAnnotation, Rgb};
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
    /// Generate a sample PDF (text pages; optional rotation, CropBox, UserUnit).
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
    },
    /// Print page count, page labels, outline and annotations.
    Info { input: PathBuf },
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
    /// Annotations.
    Annot {
        #[command(subcommand)]
        action: AnnotAction,
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
    ///   delete:ID   retarget:ID:PAGE:X:Y   open:ID   close:ID
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
    },
    /// Delete annotation ID on PAGE with its popup and replies.
    Delete {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long)]
        id: u32,
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
enum KindArg {
    Highlight,
    Underline,
    Strikeout,
    Squiggly,
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

fn main() -> ExitCode {
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
        } => {
            let doc = testgen::sample_document(&SampleSpec {
                pages,
                rotate,
                crop_box: crop,
                user_unit,
                title,
                ..SampleSpec::default()
            })?;
            save::save_atomic(&doc, SaveKind::Full, None, &out)?;
            println!("wrote {} ({pages} pages)", out.display());
        }
        Command::Info { input } => info(&input)?,
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
                    kind: match kind {
                        KindArg::Highlight => MarkupKind::Highlight,
                        KindArg::Underline => MarkupKind::Underline,
                        KindArg::Strikeout => MarkupKind::StrikeOut,
                        KindArg::Squiggly => MarkupKind::Squiggly,
                    },
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
    let doc = PdfDocument::open(path_str)?;
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
        let target = match &b.target {
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
        };
        println!(
            "{}{marker} [{}] {} -> {target}",
            "  ".repeat(depth + 1),
            b.id,
            b.title
        );
        print_bookmarks(&b.children, depth + 1);
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
            style,
        } => add(
            &input,
            &out,
            page,
            Body::FreeText {
                rect,
                text,
                font_size: size,
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
                },
            };
            edit_in_session(&input, &out, vec![op])
        }
        AnnotAction::Delete {
            input,
            out,
            page,
            id,
        } => edit_in_session(
            &input,
            &out,
            vec![Operation::DeleteAnnotation {
                page: page_index(page)?,
                id,
            }],
        ),
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
