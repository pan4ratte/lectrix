//! The comparison report (section 6.13): a PDF listing the differences in a table, as
//! FineReader's report lists them, with a summary and the options compared with. Laid out
//! from HTML by MuPDF ([`crate::ffi::write_html_pdf`]).

use std::fmt::Write as _;
use std::path::Path;

use crate::error::Result;

use super::{Difference, Group, Kind, Options};

/// What the report says besides the differences.
pub struct ReportInfo<'a> {
    pub old_name: &'a str,
    pub new_name: &'a str,
    /// When the documents were compared, as the reader's clock and language put it.
    pub when: &'a str,
    pub options: Options,
    /// Differences listed in their groups (Body text, Headers and footers, Numbering), or
    /// all in one table.
    pub grouped: bool,
    /// How a page of each document is named: its label, or its number.
    pub old_page: &'a dyn Fn(usize) -> String,
    pub new_page: &'a dyn Fn(usize) -> String,
}

/// A4, in points, and the margin around the text.
const PAGE: (f32, f32) = (595.0, 842.0);
const MARGIN: f32 = 42.0;
/// The longest text a cell quotes.
const CELL_MAX: usize = 2_000;

const CSS: &str = "
body { font-family: sans-serif; font-size: 9pt; color: #1f1f1f; }
h1 { font-size: 16pt; margin: 0 0 4pt 0; }
h2 { font-size: 11pt; margin: 14pt 0 4pt 0; }
p { margin: 0 0 3pt 0; }
.muted { color: #5f5f5f; }
table { border-collapse: collapse; width: 100%; margin-top: 4pt; }
th { text-align: left; background-color: #eeeeee; font-weight: bold; }
th, td { border: 0.5pt solid #b8b8b8; padding: 3pt 4pt; vertical-align: top; }
th.n { width: 18pt; }
th.type { width: 46pt; }
th.page { width: 30pt; }
th.text { width: 150pt; }
td.n, td.page { color: #5f5f5f; }
.inserted { color: #1a7f37; font-weight: bold; }
.deleted { color: #c62828; font-weight: bold; }
.changed { color: #9a6700; font-weight: bold; }
.none { color: #8f8f8f; }
";

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (count, c) in text.chars().enumerate() {
        if count == CELL_MAX {
            out.push('…');
            break;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn kind_name(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Inserted => ("Inserted", "inserted"),
        Kind::Deleted => ("Deleted", "deleted"),
        Kind::Changed => ("Changed", "changed"),
    }
}

/// The report as HTML.
pub fn html(differences: &[&Difference], info: &ReportInfo<'_>) -> String {
    let mut out = String::new();
    let count = |k: Kind| differences.iter().filter(|d| d.kind == k).count();
    let _ = write!(
        out,
        "<h1>Comparison report</h1>\
         <p><b>First document:</b> {}</p>\
         <p><b>Second document:</b> {}</p>\
         <p class=\"muted\">Compared {}</p>",
        escape(info.old_name),
        escape(info.new_name),
        escape(info.when),
    );
    let summary = if differences.is_empty() {
        "No differences found.".to_owned()
    } else {
        format!(
            "{}: {}, {}, {}.",
            counted(differences.len(), "difference", "differences"),
            counted(count(Kind::Inserted), "insertion", "insertions"),
            counted(count(Kind::Deleted), "deletion", "deletions"),
            counted(count(Kind::Changed), "change", "changes"),
        )
    };
    let yes_no = |on: bool| if on { "compared" } else { "ignored" };
    let _ = write!(
        out,
        "<h2>Summary</h2><p>{summary}</p>\
         <p class=\"muted\">Punctuation: {}. One-letter differences: {}. Headers and footers: {}.</p>",
        yes_no(info.options.punctuation),
        yes_no(info.options.one_letter),
        if info.options.margins {
            "found automatically"
        } else {
            "compared as body text"
        },
    );
    if differences.is_empty() {
        return out;
    }
    let groups: Vec<(Option<Group>, &str)> = if info.grouped {
        vec![
            (Some(Group::Body), "Body text"),
            (Some(Group::Margins), "Headers and footers"),
            (Some(Group::Numbering), "Numbering"),
        ]
    } else {
        vec![(None, "Differences")]
    };
    let mut number = 0;
    for (group, title) in groups {
        let rows: Vec<&&Difference> = differences
            .iter()
            .filter(|d| group.is_none_or(|g| d.group == g))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let _ = write!(
            out,
            "<h2>{title} ({})</h2><table><tr><th class=\"n\">#</th><th class=\"type\">Type</th>             <th class=\"page\">Page</th><th class=\"text\">{}</th><th class=\"page\">Page</th>             <th class=\"text\">{}</th></tr>",
            rows.len(),
            escape(info.old_name),
            escape(info.new_name)
        );
        for d in rows {
            number += 1;
            let (name, class) = kind_name(d.kind);
            let cell = |text: &str, area: bool| {
                if !text.is_empty() {
                    escape(text)
                } else if area {
                    "<span class=\"none\">(an area of the page)</span>".to_owned()
                } else {
                    "<span class=\"none\">—</span>".to_owned()
                }
            };
            let _ = write!(
                out,
                "<tr><td class=\"n\">{number}</td><td class=\"{class}\">{name}</td>\
                 <td class=\"page\">{}</td><td>{}</td><td class=\"page\">{}</td><td>{}</td></tr>",
                escape(&(info.old_page)(d.old.page())),
                cell(&d.old.text, !d.old.rects.is_empty()),
                escape(&(info.new_page)(d.new.page())),
                cell(&d.new.text, !d.new.rects.is_empty()),
            );
        }
        out.push_str("</table>");
    }
    out
}

/// Writes the report to `path`: to a file beside it first, which then replaces it, so a
/// failure leaves what was there.
pub fn write(path: &Path, differences: &[&Difference], info: &ReportInfo<'_>) -> Result<()> {
    let html = html(differences, info);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "report.pdf".into());
    let temp = path.with_file_name(format!(".{name}.lectrix-tmp"));
    let written = crate::ffi::write_html_pdf(&temp, &html, CSS, PAGE, MARGIN)
        .and_then(|()| std::fs::rename(&temp, path).map_err(Into::into));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{PageRect, Side};

    fn side(text: &str, page: usize) -> Side {
        Side {
            text: text.into(),
            spans: Vec::new(),
            rects: vec![PageRect {
                page,
                rect: [0.0, 0.0, 1.0, 1.0],
            }],
            caret: None,
        }
    }

    #[test]
    fn lists_differences_by_group() {
        let a = Difference {
            kind: Kind::Changed,
            group: Group::Body,
            old: side("brown <fox> — Привет, 世界", 0),
            new: side("red fox", 1),
        };
        let b = Difference {
            kind: Kind::Inserted,
            group: Group::Margins,
            old: Side::default(),
            new: side("v2", 2),
        };
        let name = |p: usize| (p + 1).to_string();
        let info = ReportInfo {
            old_name: "a.pdf",
            new_name: "b.pdf",
            when: "27 October 2026, 14:05",
            options: Options::default(),
            grouped: true,
            old_page: &name,
            new_page: &name,
        };
        let html = html(&[&a, &b], &info);
        assert!(html.contains("2 differences: 1 insertion, 0 deletions, 1 change."));
        assert!(html.contains("brown &lt;fox&gt;"));
        assert!(html.find("Body text (1)") < html.find("Headers and footers (1)"));
        assert!(!html.contains("Numbering ("));

        // The app's fonts: text in scripts the built-in fonts lack comes from the system's.
        crate::fonts::install();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("compare-report.pdf");
        write(&path, &[&a, &b], &info).unwrap();
        let doc = mupdf::pdf::PdfDocument::open(path.to_str().unwrap()).unwrap();
        assert_eq!(doc.page_count().unwrap(), 1);
    }
}
