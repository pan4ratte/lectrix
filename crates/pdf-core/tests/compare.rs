//! Compare (section 6.13) on generated documents: the differences found between two
//! versions, and the annotated copies and the report written from them.

mod common;

use std::path::{Path, PathBuf};

use common::{out_dir, qpdf_check, writing};
use pdf_core::compare::annotate::{self, Which};
use pdf_core::compare::report::{self, ReportInfo};
use pdf_core::compare::{self, DocText, Group, Kind, Options};
use pdf_core::save::{SaveKind, save_atomic};
use pdf_core::session::Session;
use pdf_core::testgen::text_document;
use pdf_core::text::PageText;

fn lines(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|t| (*t).to_owned()).collect()
}

fn write(dir: &Path, name: &str, header: &str, pages: &[Vec<String>]) -> PathBuf {
    let path = dir.join(name);
    let doc = text_document(header, pages).unwrap();
    writing(|| save_atomic(&doc, SaveKind::Full, None, &path)).unwrap();
    path
}

fn read(session: &Session) -> (Vec<PageText>, Vec<f32>) {
    let info = session.info().unwrap();
    let texts = (0..info.pages.len())
        .map(|p| session.content_text(p).unwrap())
        .collect();
    (texts, info.pages.iter().map(|p| p.height).collect())
}

/// Two versions of a contract: a word changed, a sentence deleted, a page inserted, the
/// running header changed, and a word spelt differently.
fn versions(dir: &Path) -> (PathBuf, PathBuf) {
    let old = write(
        dir,
        "old.pdf",
        "Supply agreement",
        &[
            lines(&[
                "1. The supplier delivers the goods by Friday.",
                "2. The buyer pays within thirty days.",
                "Late payment carries interest.",
            ]),
            lines(&[
                "3. Either party may end this agreement in writing.",
                "The colour of the goods is grey.",
            ]),
        ],
    );
    let new = write(
        dir,
        "new.pdf",
        "Supply agreement, second draft",
        &[
            lines(&[
                "1. The supplier delivers the goods by Monday.",
                "2. The buyer pays within thirty days.",
            ]),
            lines(&["Annex: prices are fixed for one year."]),
            lines(&[
                "3. Either party may end this agreement in writing.",
                "The color of the goods is grey.",
            ]),
        ],
    );
    (old, new)
}

#[test]
fn finds_the_differences_between_two_versions() {
    let dir = out_dir("compare");
    let (old, new) = versions(&dir);
    let (a, _) = Session::open(&old, None).unwrap();
    let (b, _) = Session::open(&new, None).unwrap();
    let ((ta, ha), (tb, hb)) = (read(&a), read(&b));
    let c = compare::compare(
        DocText {
            pages: &ta,
            heights: &ha,
        },
        DocText {
            pages: &tb,
            heights: &hb,
        },
        Options::default(),
        &|| false,
    )
    .unwrap();
    let found: Vec<(Kind, Group, &str, &str, usize, usize)> = c
        .differences
        .iter()
        .map(|d| {
            (
                d.kind,
                d.group,
                d.old.text.as_str(),
                d.new.text.as_str(),
                d.old.page(),
                d.new.page(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (Kind::Changed, Group::Body, "Friday", "Monday", 0, 0),
            (Kind::Inserted, Group::Margins, "", ", second draft", 0, 0),
            // The full stop is the same in both.
            (
                Kind::Changed,
                Group::Body,
                "Late payment carries interest",
                "Annex: prices are fixed for one year",
                0,
                1
            ),
            (Kind::Inserted, Group::Margins, "", ", second draft", 1, 1),
            (Kind::Changed, Group::Body, "colour", "color", 1, 2),
            // The new third page's header and footer.
            (
                Kind::Inserted,
                Group::Margins,
                "",
                "Supply agreement, second draft Page 3",
                1,
                2
            ),
        ]
    );
    // Without one-letter differences, the spelling goes.
    let quiet = compare::compare(
        DocText {
            pages: &ta,
            heights: &ha,
        },
        DocText {
            pages: &tb,
            heights: &hb,
        },
        Options {
            one_letter: false,
            ..Options::default()
        },
        &|| false,
    )
    .unwrap();
    assert!(quiet.differences.iter().all(|d| d.old.text != "colour"));

    // The annotated copies: each difference marked once, with its comment, in a file
    // qpdf finds sound; the documents compared are unchanged.
    let all: Vec<&compare::Difference> = c.differences.iter().collect();
    for (which, source, other) in [(Which::Old, &old, "new.pdf"), (Which::New, &new, "old.pdf")] {
        let marks = annotate::annotations(&all, which, other, "Tester");
        let target = dir.join(format!("{which:?} annotated.pdf"));
        let before = std::fs::read(source).unwrap();
        writing(|| annotate::write_copy(source, None, &target, marks)).unwrap();
        assert_eq!(std::fs::read(source).unwrap(), before);
        qpdf_check(&target);
        let (copy, info) = Session::open(&target, None).unwrap();
        copy.close();
        let marks: Vec<_> = info
            .annotations
            .iter()
            .flatten()
            .filter(|a| a.subtype != "Popup")
            .collect();
        assert_eq!(marks.len(), 6, "{which:?}: {marks:?}");
        let contents: Vec<&str> = marks.iter().map(|a| a.contents.as_str()).collect();
        // A difference this document has no text for is a sticky note.
        assert_eq!(
            marks.iter().filter(|a| a.subtype == "Text").count(),
            3 * usize::from(which == Which::Old)
        );
        let expected = match which {
            Which::Old => "Changed. In new.pdf: “Monday”",
            Which::New => "Changed. In old.pdf: “Friday”",
        };
        assert!(contents.contains(&expected), "{which:?}: {contents:?}");
    }

    let name = |p: usize| (p + 1).to_string();
    let info = ReportInfo {
        old_name: "old.pdf",
        new_name: "new.pdf",
        when: "27 October 2026, 14:05",
        options: Options::default(),
        grouped: true,
        old_page: &name,
        new_page: &name,
    };
    let target = dir.join("report.pdf");
    writing(|| report::write(&target, &all, &info)).unwrap();
    qpdf_check(&target);
    a.close();
    b.close();
}
