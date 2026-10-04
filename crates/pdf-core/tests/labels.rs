//! Page labels (AGENTS.md section 6.3): labels another app wrote are read exactly
//! as stored, keep their bytes through edits that do not touch them, and an edit rewrites
//! only the number tree, which MuPDF then reads back as written.

mod common;

use std::path::{Path, PathBuf};

use common::{assemble, objects_written_after, open, out_dir, qpdf_check, raw_object, writing};
use pdf_core::labels::{LabelRule, LabelStyle, labels_for_pages, read_rules};
use pdf_core::ops::Operation;
use pdf_core::outline::ViewDest;
use pdf_core::save::SaveKind;
use pdf_core::session::Session;

const PAGES: usize = 12;
/// Object numbers of the fixture's number tree: the root and its two leaves.
const ROOT: u32 = 3;
const LEAF_A: u32 = 4;
const LEAF_B: u32 = 5;

/// A PDF written by hand the way other apps write labels: a number tree with `/Kids` and
/// `/Limits` instead of one `/Nums` array, a `/Type` key, a UTF-16BE prefix ("Äh-"), and
/// formatting MuPDF would not produce.
fn fixture() -> Vec<u8> {
    let kids: Vec<String> = (0..PAGES).map(|i| format!("{} 0 R", 6 + i)).collect();
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /PageLabels 3 0 R >>".into(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {PAGES} /MediaBox [0 0 612.000 792] >>",
            kids.join(" ")
        ),
        "<< /Kids [4 0 R 5 0 R] >>".into(),
        "<< /Limits [0 4] /Nums [0 << /Type /PageLabel /P (Cover) >> 1 << /S /r >>  4 << /S /D /St 1 >>] >>".into(),
        "<< /Limits [9 11] /Nums [9 << /S /A /P <FEFF00C40068002D> /St 3 >> 11 << /S /D /P (Index ) /St 120 >>] >>".into(),
    ];
    objects
        .extend((0..PAGES).map(|_| "<< /Type /Page /Parent 2 0 R /Resources << >> >>".to_string()));
    assemble(&objects)
}

fn stored_rules() -> Vec<LabelRule> {
    vec![
        LabelRule {
            start_page: 0,
            style: LabelStyle::None,
            prefix: "Cover".into(),
            first_number: 1,
        },
        LabelRule {
            start_page: 1,
            style: LabelStyle::LowerRoman,
            prefix: String::new(),
            first_number: 1,
        },
        LabelRule::decimal_from_one(4),
        LabelRule {
            start_page: 9,
            style: LabelStyle::UpperLetters,
            prefix: "Äh-".into(),
            first_number: 3,
        },
        LabelRule {
            start_page: 11,
            style: LabelStyle::Decimal,
            prefix: "Index ".into(),
            first_number: 120,
        },
    ]
}

fn write_fixture(dir: &Path) -> PathBuf {
    let path = dir.join("labels.pdf");
    std::fs::write(&path, fixture()).unwrap();
    path
}

/// MuPDF's own label for every page (`fz_page_label`), an independent reading.
fn mupdf_labels(path: &Path) -> Vec<String> {
    let doc = open(path);
    (0..PAGES).map(|p| doc.page_label(p).unwrap()).collect()
}

#[test]
fn another_apps_labels_are_read_as_stored() {
    let path = write_fixture(&out_dir("labels-read"));
    let (session, info) = Session::open(&path, None).unwrap();
    let labels = info.labels.expect("the fixture has labels");
    assert_eq!(labels.rules, stored_rules());
    assert_eq!(
        labels.labels,
        [
            "Cover",
            "i",
            "ii",
            "iii",
            "1",
            "2",
            "3",
            "4",
            "5",
            "Äh-C",
            "Äh-D",
            "Index 120"
        ]
    );
    assert_eq!(labels.labels, mupdf_labels(&path));
    session.close();
}

#[test]
fn labels_nobody_edits_keep_their_bytes() {
    let dir = out_dir("labels-untouched-bytes");
    let path = write_fixture(&dir);
    let original = std::fs::read(&path).unwrap();
    let (session, _) = Session::open(&path, None).unwrap();

    session
        .apply(Operation::RotatePages {
            pages: vec![0],
            degrees: 90,
        })
        .unwrap();
    session
        .apply(Operation::AddBookmark {
            parent: None,
            index: 0,
            title: "Chapter 1".into(),
            dest: ViewDest {
                page: 4,
                x: 0.0,
                y: 0.0,
            },
        })
        .unwrap();
    // Committing the labels as they are (the panel's field lost focus without a change)
    // is not an edit.
    let state = session.info().unwrap().state;
    let unchanged = session
        .apply(Operation::SetPageLabels {
            rules: stored_rules(),
        })
        .unwrap();
    assert_eq!(unchanged.state, state);
    assert_eq!(unchanged.labels, None);

    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    session.close();
    qpdf_check(&path);

    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&original));
    let written = objects_written_after(&saved, original.len());
    for num in [ROOT, LEAF_A, LEAF_B] {
        assert!(!written.contains(&num), "object {num} was written again");
    }
    // The catalog was written again for the new outline; it still points at the tree.
    assert!(raw_object(&saved, 1).contains("/PageLabels 3 0 R"));
    assert_eq!(read_rules(&open(&path)).unwrap(), stored_rules());
}

#[test]
fn an_edit_rewrites_only_the_tree_and_can_be_undone() {
    let dir = out_dir("labels-edit");
    let path = write_fixture(&dir);
    let original = std::fs::read(&path).unwrap();
    let (session, _) = Session::open(&path, None).unwrap();

    // The panel sends every rule, with the decimal range now starting at page 6.
    let mut edited = stored_rules();
    edited[2].start_page = 5;
    let change = session
        .apply(Operation::SetPageLabels {
            rules: edited.clone(),
        })
        .unwrap();
    let labels = change.labels.unwrap().unwrap();
    assert_eq!(labels.rules, edited);
    assert_eq!(&labels.labels[1..6], ["i", "ii", "iii", "iv", "1"]);

    let undone = session.undo().unwrap();
    assert_eq!(undone.labels.unwrap().unwrap().rules, stored_rules());
    session.redo().unwrap();

    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    session.close();
    qpdf_check(&path);

    // Only the tree's root object is written again (as one flat /Nums array); the
    // catalog keeps its bytes.
    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&original));
    assert_eq!(objects_written_after(&saved, original.len()), [ROOT].into());
    assert_eq!(raw_object(&saved, 1), raw_object(&original, 1));
    assert!(!raw_object(&saved, ROOT).contains("/Kids"));

    // MuPDF reads back what was written.
    assert_eq!(read_rules(&open(&path)).unwrap(), edited);
    assert_eq!(mupdf_labels(&path), labels_for_pages(&edited, PAGES));
}
