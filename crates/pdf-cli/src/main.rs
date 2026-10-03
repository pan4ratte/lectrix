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
use pdf_core::annot::{self, MarkupKind, MarkupSpec, Rgb};
use pdf_core::geometry::Rect;
use pdf_core::labels::{self, LabelRule, LabelStyle};
use pdf_core::merge::{self, BookmarkMode, LabelMode, MergeOptions, MergeSource};
use pdf_core::outline::{self, OutlineItem, OutlineTarget, ReadOutlineItem};
use pdf_core::render;
use pdf_core::save::{self, SaveKind};
use pdf_core::testgen::{self, SampleSpec};
use pdf_core::{Error, Result};

#[derive(Parser)]
#[command(name = "pdf-cli", version, about = "Headless Folio PDF engine")]
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
        #[arg(long, default_value = "Folio sample")]
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
}

#[derive(Subcommand)]
enum AnnotAction {
    /// Mark up text: finds TEXT on PAGE and creates one annotation over every match.
    Markup {
        input: PathBuf,
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = KindArg::Highlight)]
        kind: KindArg,
        /// Page number (1-based).
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long)]
        text: String,
        /// RRGGBB.
        #[arg(long, default_value = "FFEB3B", value_parser = parse_color)]
        color: Rgb,
        #[arg(long, default_value_t = 1.0)]
        opacity: f32,
        #[arg(long, default_value = "Folio")]
        author: String,
        #[arg(long)]
        note: Option<String>,
    },
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
        },
        Command::Outline {
            action: OutlineAction::Set { input, out, items },
        } => {
            let tree = parse_outline(&items)?;
            edit(&input, &out, |doc| outline::write_outline(doc, &tree))?;
        }
        Command::Merge {
            out,
            inputs,
            bookmarks,
            labels,
        } => {
            let sources = inputs
                .iter()
                .map(|p| {
                    Ok(MergeSource {
                        doc: open(p)?,
                        name: source_name(p),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let options = MergeOptions {
                bookmarks: match bookmarks {
                    BookmarksArg::Nest => BookmarkMode::NestUnderSource,
                    BookmarksArg::Flat => BookmarkMode::Flat,
                    BookmarksArg::Drop => BookmarkMode::Drop,
                },
                labels: match labels {
                    LabelsArg::Keep => LabelMode::KeepSources,
                    LabelsArg::Continuous => LabelMode::Continuous,
                    LabelsArg::None => LabelMode::None,
                },
            };
            let merged = merge::merge(&sources, options)?;
            save::save_atomic(&merged, SaveKind::Optimized, None, &out)?;
            println!("wrote {} ({} pages)", out.display(), merged.page_count()?);
        }
        Command::Annot {
            action:
                AnnotAction::Markup {
                    input,
                    out,
                    kind,
                    page,
                    text,
                    color,
                    opacity,
                    author,
                    note,
                },
        } => {
            let index = page.checked_sub(1).ok_or(Error::PageOutOfRange(0))?;
            edit(&input, &out, |doc| {
                let quads = find_text(doc, index, &text)?;
                if quads.is_empty() {
                    return Err(Error::InvalidArgument(format!(
                        "\"{text}\" not found on page {page}"
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
    }
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

fn source_name(path: &Path) -> String {
    if let Ok(doc) = open(path)
        && let Ok(title) = doc.metadata(mupdf::document::MetadataName::Title)
        && !title.trim().is_empty()
    {
        return title;
    }
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into())
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

fn print_outline(items: &[ReadOutlineItem], depth: usize) {
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
