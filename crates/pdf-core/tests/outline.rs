//! Bookmark editing (AGENTS.md section 6.2, Phase 2): reading targets of every kind, edits
//! through the session with undo/redo, expanded states saved without dirtying, damaged
//! outlines refused, and untouched bookmarks keeping their destinations exactly.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{assemble, objects_written_after, open, out_dir, qpdf_check, raw_object};
use mupdf::pdf::PdfObject;
use pdf_core::Error;
use pdf_core::ops::Operation;
use pdf_core::outline::{Bookmark, Outline, Target, ViewDest, read_bookmarks};
use pdf_core::save::SaveKind;
use pdf_core::session::Session;

/// Object numbers of the fixture's outline items.
const A: u32 = 11;
const A1: u32 = 12;
const A1A: u32 = 13;
const A1B: u32 = 14;
const A2: u32 = 15;
const A3: u32 = 16;
const B: u32 = 17;
const B1: u32 = 18;
const B2: u32 = 19;
const C: u32 = 20;
const D: u32 = 21;

/// A PDF written by hand, not by MuPDF, so its bytes use formatting MuPDF would not
/// produce (`612.000`, `720.0`, hex strings, unusual spacing). Six pages, a three-level
/// outline with explicit, named (string, through a name tree) and action targets, styles
/// and an unknown key.
fn fixture() -> Vec<u8> {
    let page = "<< /Type /Page /Parent 2 0 R /Resources << >> >>".to_string();
    let objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R /Names << /Dests 4 0 R >> >>".into(),
        "<< /Type /Pages /Kids [5 0 R 6 0 R 7 0 R 8 0 R 9 0 R 10 0 R] /Count 6 /MediaBox [0 0 612.000 792] >>".into(),
        "<< /Type /Outlines /First 11 0 R /Last 21 0 R /Count 7 >>".into(),
        "<< /Names [(chap2) [6 0 R /FitH 700.50] (intro) << /D [5 0 R /XYZ 72.000 720 0] >>] >>".into(),
        page.clone(),
        page.clone(),
        page.clone(),
        page.clone(),
        page.clone(),
        page,
        // 11 A
        "<< /Title (Part I) /Parent 3 0 R /First 12 0 R /Last 16 0 R /Next 17 0 R /Count 3 /Dest [5 0 R /XYZ 72.000 720.0 null] >>".into(),
        // 12 A1
        "<< /Title (Chapter 1) /Parent 11 0 R /First 13 0 R /Last 14 0 R /Next 15 0 R /Count -2 /Dest (intro) >>".into(),
        // 13 A1a
        "<< /Title (Section 1.1) /Parent 12 0 R /Next 14 0 R /A << /S /GoTo /D (chap2) >> >>".into(),
        // 14 A1b
        "<< /Title (Section 1.2) /Parent 12 0 R /Prev 13 0 R /A << /S /URI /URI (https://example.org/a%20b) >> >>".into(),
        // 15 A2
        "<< /Title (Chapter 2) /Parent 11 0 R /Prev 12 0 R /Next 16 0 R /Dest [6 0 R /Fit] /C [0.000 0 1] /F 3 /Foo /Bar >>".into(),
        // 16 A3
        "<< /Title (Chapter 3) /Parent 11 0 R /Prev 15 0 R /A << /S /GoToR /F (other.pdf) /D [0 /Fit] >> >>".into(),
        // 17 B
        "<< /Title (Part II) /Parent 3 0 R /Prev 11 0 R /Next 20 0 R /First 18 0 R /Last 19 0 R /Count -2 /Dest [7 0 R /XYZ null 500.25 null] >>".into(),
        // 18 B1
        "<< /Title (Chapter 4) /Parent 17 0 R /Next 19 0 R /A << /S /JavaScript /JS (app.alert\\(1\\)) >> >>".into(),
        // 19 B2
        "<< /Title (Chapter 5) /Parent 17 0 R /Prev 18 0 R /Dest [8 0 R /FitR 10 20 300.5 400] >>".into(),
        // 20 C: "Приложение" as UTF-16BE
        "<< /Title <FEFF041F04400438043B043E04360435043D04380435> /Parent 3 0 R /Prev 17 0 R /Next 21 0 R /Dest [9 0 R /XYZ 0 792 0] >>".into(),
        // 21 D
        "<< /Title (Index) /Parent 3 0 R /Prev 20 0 R /Dest [10 0 R /XYZ 0 792 null] >>".into(),
    ];
    assemble(&objects)
}

fn write_fixture(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, fixture()).unwrap();
    path
}

fn find(outline: &Outline, id: u32) -> Bookmark {
    let mut found = None;
    outline.for_each(|b| {
        if b.id == id {
            found = Some(b.clone());
        }
    });
    found.unwrap_or_else(|| panic!("bookmark {id} not found"))
}

/// (title, children) for comparing shapes.
fn shape(items: &[Bookmark]) -> Vec<(String, Vec<String>)> {
    items
        .iter()
        .map(|b| {
            (
                b.title.clone(),
                b.children.iter().map(|c| c.title.clone()).collect(),
            )
        })
        .collect()
}

#[test]
fn reads_every_kind_of_target() {
    let dir = out_dir("outline-read");
    let doc = open(&write_fixture(&dir, "fixture.pdf"));
    let outline = read_bookmarks(&doc).unwrap();
    assert!(!outline.damaged);
    assert_eq!(
        shape(&outline.items),
        vec![
            (
                "Part I".into(),
                vec!["Chapter 1".into(), "Chapter 2".into(), "Chapter 3".into()]
            ),
            (
                "Part II".into(),
                vec!["Chapter 4".into(), "Chapter 5".into()]
            ),
            ("Приложение".into(), vec![]),
            ("Index".into(), vec![]),
        ]
    );
    let page = |page, x: Option<f32>, y: Option<f32>, named: Option<&str>| Target::Page {
        page,
        x,
        y,
        named: named.map(str::to_owned),
    };
    let expect = [
        (A, page(0, Some(72.0), Some(72.0), None)),
        (A1, page(0, Some(72.0), Some(72.0), Some("intro"))),
        (A1A, page(1, None, Some(91.5), Some("chap2"))),
        (A1B, Target::Uri("https://example.org/a%20b".into())),
        (A2, page(1, None, None, None)),
        (A3, Target::File("other.pdf".into())),
        (B, page(2, None, Some(291.75), None)),
        (B1, Target::Action("JavaScript".into())),
        (B2, page(3, Some(10.0), Some(392.0), None)),
        (C, page(4, Some(0.0), Some(0.0), None)),
        (D, page(5, Some(0.0), Some(0.0), None)),
    ];
    for (id, target) in expect {
        assert_eq!(find(&outline, id).target, target, "bookmark {id}");
    }
    let a = find(&outline, A);
    assert!(a.open);
    assert!(!find(&outline, A1).open && !find(&outline, B).open);
    let a2 = find(&outline, A2);
    assert!(a2.bold && a2.italic);
    assert_eq!(a2.color, Some([0.0, 0.0, 1.0]));
}

/// The serialized `/Dest` and `/A` of an outline item, as MuPDF prints them.
fn dest_and_action(item: &PdfObject) -> (Option<String>, Option<String>) {
    let print = |key| item.get_dict(key).unwrap().map(|v| v.to_string());
    (print("Dest"), print("A"))
}

#[test]
fn edits_touch_only_what_they_must_and_untouched_destinations_survive() {
    let dir = out_dir("outline-edit");
    let path = write_fixture(&dir, "edited.pdf");
    let original = std::fs::read(&path).unwrap();
    let before = open(&path);
    let untouched = [A, A1, A1A, A2, A3, B, B1, B2, C];
    let before_values: Vec<_> = untouched
        .iter()
        .map(|&n| dest_and_action(&before.new_indirect(n as i32, 0).unwrap()))
        .collect();
    drop(before);

    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.outline.items.len(), 4);
    let rename = session
        .apply(Operation::RenameBookmark {
            id: A2,
            title: "Chapter Two".into(),
        })
        .unwrap();
    assert_eq!(rename.state.undo_name.as_deref(), Some("Rename bookmark"));
    session
        .apply(Operation::MoveBookmark {
            id: B2,
            parent: Some(A),
            index: 3,
        })
        .unwrap();
    session.apply(Operation::DeleteBookmark { id: D }).unwrap();
    let added = session
        .apply(Operation::AddBookmark {
            parent: None,
            index: 1,
            title: "New\nbookmark".into(),
            dest: ViewDest {
                page: 2,
                x: 0.0,
                y: 100.0,
            },
        })
        .unwrap();
    let new_id = added.created.expect("the new bookmark's id");
    let outline = added.outline.expect("the outline changed");
    assert_eq!(find(&outline, new_id).title, "New bookmark");
    session
        .apply(Operation::SetBookmarkDestination {
            id: A1B,
            dest: ViewDest {
                page: 1,
                x: 36.0,
                y: 50.0,
            },
        })
        .unwrap();
    // Expanding a bookmark is not an edit: no dirty change, no undo step.
    let state_before = session.info().unwrap().state;
    session.set_bookmark_open(B, true).unwrap();
    let info = session.info().unwrap();
    assert_eq!(info.state, state_before);
    assert!(find(&info.outline, B).open);

    common::writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    session.close();

    let saved = std::fs::read(&path).unwrap();
    assert!(
        saved.starts_with(&original),
        "incremental save keeps the original bytes"
    );
    qpdf_check(&path);

    // Only the edited items, the neighbours whose links changed, the parents whose counts
    // changed, the outline dictionary and the new item were written again.
    let written = objects_written_after(&saved, original.len());
    let new_num = new_id;
    let expected: BTreeSet<u32> = [A2, B2, A3, A, B1, B, C, 3, new_num, A1B].into();
    assert_eq!(written, expected);
    // So these keep their original bytes exactly.
    for num in [A1, A1A, D, 4, 1, 2] {
        assert_eq!(
            raw_object(&saved, num),
            raw_object(&original, num),
            "object {num}"
        );
    }

    // Every bookmark the user did not retarget keeps its destination or action, also
    // where its dictionary was written again for new links.
    let after = open(&path);
    for (&num, before) in untouched.iter().zip(&before_values) {
        let item = after.new_indirect(num as i32, 0).unwrap();
        assert_eq!(&dest_and_action(&item), before, "bookmark {num}");
    }
    // The renamed item keeps its style and unknown keys.
    let a2 = after.new_indirect(A2 as i32, 0).unwrap();
    assert_eq!(a2.get_dict("Foo").unwrap().unwrap().to_string(), "/Bar");
    assert_eq!(a2.get_dict("F").unwrap().unwrap().as_int().unwrap(), 3);

    let outline = read_bookmarks(&after).unwrap();
    assert_eq!(
        shape(&outline.items),
        vec![
            (
                "Part I".into(),
                vec![
                    "Chapter 1".into(),
                    "Chapter Two".into(),
                    "Chapter 3".into(),
                    "Chapter 5".into()
                ]
            ),
            ("New bookmark".into(), vec![]),
            ("Part II".into(), vec!["Chapter 4".into()]),
            ("Приложение".into(), vec![]),
        ]
    );
    assert!(find(&outline, B).open, "the expanded state was saved");
    assert_eq!(
        find(&outline, new_num).target,
        Target::Page {
            page: 2,
            x: Some(0.0),
            y: Some(100.0),
            named: None
        }
    );
    assert_eq!(
        find(&outline, A1B).target,
        Target::Page {
            page: 1,
            x: Some(36.0),
            y: Some(50.0),
            named: None
        }
    );
    // New and retargeted bookmarks: explicit /XYZ, null zoom, no action left over.
    let a1b = after.new_indirect(A1B as i32, 0).unwrap();
    assert!(a1b.get_dict("A").unwrap().is_none());
    assert_eq!(
        a1b.get_dict("Dest").unwrap().unwrap().to_string(),
        "[6 0 R/XYZ 36 742 null]"
    );
    // Counts: Part I is open with 4 visible children (Chapter 1 closed), Part II open
    // with 1, the outline shows 4 top-level items plus 5 below open parents.
    let count = |n: u32| {
        after
            .new_indirect(n as i32, 0)
            .unwrap()
            .get_dict("Count")
            .unwrap()
            .map(|c| c.as_int().unwrap())
    };
    assert_eq!(count(A), Some(4));
    assert_eq!(count(B), Some(1));
    assert_eq!(count(3), Some(9));
}

#[test]
fn optimized_save_keeps_destination_values() {
    let dir = out_dir("outline-optimized");
    let path = write_fixture(&dir, "source.pdf");
    let before = open(&path);
    let values: Vec<_> = [A, A1, A1A, A1B, A2, A3, B, B1, B2, C, D]
        .iter()
        .map(|&n| {
            let item = before.new_indirect(n as i32, 0).unwrap();
            (
                item.get_dict("Title")
                    .unwrap()
                    .unwrap()
                    .as_string()
                    .unwrap(),
                dest_and_action(&item),
            )
        })
        .collect();
    drop(before);
    let (session, _) = Session::open(&path, None).unwrap();
    session
        .apply(Operation::RenameBookmark {
            id: C,
            title: "Appendix".into(),
        })
        .unwrap();
    let target = dir.join("optimized.pdf");
    session
        .save(SaveKind::Optimized, Some(target.clone()))
        .unwrap();
    session.close();
    qpdf_check(&target);
    // Object numbers change in a full rewrite, so compare by title, ignoring page
    // references (checked through the resolved targets instead).
    let after = open(&target);
    let outline = read_bookmarks(&after).unwrap();
    let mut by_title = std::collections::HashMap::new();
    outline.for_each(|b| {
        by_title.insert(b.title.clone(), b.id);
    });
    let strip_refs = |s: &Option<String>| {
        s.as_ref().map(|s| {
            let words: Vec<&str> = s.split(' ').collect();
            let mut out = Vec::new();
            let mut i = 0;
            while i < words.len() {
                if i + 2 < words.len() && words[i + 1] == "0" && words[i + 2].starts_with('R') {
                    out.push(format!("REF{}", &words[i + 2][1..]));
                    i += 3;
                } else {
                    out.push(words[i].to_owned());
                    i += 1;
                }
            }
            out.join(" ")
        })
    };
    for (title, (dest, action)) in values {
        let title = if title == "Приложение" {
            "Appendix".to_owned()
        } else {
            title
        };
        let id = by_title[&title];
        let item = after.new_indirect(id as i32, 0).unwrap();
        let (d, a) = dest_and_action(&item);
        assert_eq!(strip_refs(&d), strip_refs(&dest), "{title}");
        assert_eq!(strip_refs(&a), strip_refs(&action), "{title}");
    }
    let before_targets = read_bookmarks(&open(&path)).unwrap();
    let mut targets = Vec::new();
    before_targets.for_each(|b| targets.push(b.target.clone()));
    let mut after_targets = Vec::new();
    outline.for_each(|b| after_targets.push(b.target.clone()));
    assert_eq!(targets, after_targets);
}

#[test]
fn bookmark_edits_undo_and_redo() {
    let dir = out_dir("outline-undo");
    let path = write_fixture(&dir, "undo.pdf");
    let (session, info) = Session::open(&path, None).unwrap();
    let original = info.outline;
    let add = session
        .apply(Operation::AddBookmark {
            parent: Some(B1),
            index: 0,
            title: "Under Chapter 4".into(),
            dest: ViewDest {
                page: 0,
                x: 0.0,
                y: 0.0,
            },
        })
        .unwrap();
    assert!(add.state.dirty);
    let with_child = add.outline.unwrap();
    // A bookmark that gains its first child opens.
    assert!(find(&with_child, B1).open);
    let undone = session.undo().unwrap();
    assert_eq!(undone.outline.as_ref(), Some(&original));
    assert!(!undone.state.dirty);
    assert_eq!(undone.state.redo_name.as_deref(), Some("Add bookmark"));
    let redone = session.redo().unwrap();
    assert_eq!(redone.outline.as_ref(), Some(&with_child));

    session.apply(Operation::DeleteBookmark { id: A }).unwrap();
    let info = session.info().unwrap();
    assert_eq!(info.outline.items.len(), 3);
    assert_eq!(info.state.undo_name.as_deref(), Some("Delete bookmark"));
    session.undo().unwrap();
    assert_eq!(session.info().unwrap().outline.items[0].title, "Part I");
    session.close();
}

#[test]
fn invalid_moves_and_titles_change_nothing() {
    let dir = out_dir("outline-invalid");
    let path = write_fixture(&dir, "invalid.pdf");
    let (session, info) = Session::open(&path, None).unwrap();
    let into_itself = session.apply(Operation::MoveBookmark {
        id: A,
        parent: Some(A1A),
        index: 0,
    });
    assert!(matches!(into_itself, Err(Error::InvalidArgument(_))));
    let empty = session.apply(Operation::RenameBookmark {
        id: A,
        title: " \t ".into(),
    });
    assert!(matches!(empty, Err(Error::InvalidArgument(_))));
    let missing = session.apply(Operation::DeleteBookmark { id: 9999 });
    assert!(matches!(missing, Err(Error::InvalidArgument(_))));
    // Moving an item to where it already is changes nothing.
    let same = session
        .apply(Operation::MoveBookmark {
            id: A2,
            parent: Some(A),
            index: 1,
        })
        .unwrap();
    assert!(same.outline.is_none());
    let now = session.info().unwrap();
    assert_eq!(now.state, info.state);
    assert_eq!(now.outline, info.outline);
    session.close();
}

#[test]
fn first_bookmark_creates_the_outline() {
    let dir = out_dir("outline-first");
    let path = common::sample_file(&dir, "plain.pdf", pdf_core::testgen::SampleSpec::default());
    let (session, info) = Session::open(&path, None).unwrap();
    assert!(info.outline.items.is_empty());
    let change = session
        .apply(Operation::AddBookmark {
            parent: None,
            index: 0,
            title: "Préface — 日本".into(),
            dest: ViewDest {
                page: 1,
                x: 10.0,
                y: 20.0,
            },
        })
        .unwrap();
    assert_eq!(change.outline.unwrap().items[0].title, "Préface — 日本");
    common::writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    session.close();
    qpdf_check(&path);
    let doc = open(&path);
    let root = doc
        .catalog()
        .unwrap()
        .get_dict("Outlines")
        .unwrap()
        .unwrap();
    assert_eq!(
        root.get_dict("Count").unwrap().unwrap().as_int().unwrap(),
        1
    );
    let item = root.get_dict("First").unwrap().unwrap();
    // Non-ASCII titles are UTF-16BE with a byte-order mark.
    assert_eq!(
        &item.get_dict("Title").unwrap().unwrap().as_bytes().unwrap()[..2],
        &[0xFE, 0xFF]
    );
}

#[test]
fn damaged_outline_is_shown_but_not_edited() {
    let dir = out_dir("outline-damaged");
    // Make D's /Next point back at A: a cycle.
    let mut bytes = fixture();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let fixed = text.replace(
        "/Prev 20 0 R /Dest [10 0 R",
        "/Prev 20 0 R /Next 11 0 R /Dest [10 0 R",
    );
    // Same length changes would keep offsets; rebuild instead.
    assert_ne!(fixed, text);
    bytes = rebuild(&fixed);
    let path = dir.join("cycle.pdf");
    std::fs::write(&path, bytes).unwrap();
    let (session, info) = Session::open(&path, None).unwrap();
    assert!(info.outline.damaged);
    assert_eq!(info.outline.items.len(), 4, "read up to the cycle");
    let edit = session.apply(Operation::RenameBookmark {
        id: A,
        title: "x".into(),
    });
    assert!(matches!(edit, Err(Error::DamagedOutline)));
    session.close();
}

/// Re-assembles a fixture whose object text was changed (offsets move).
fn rebuild(text: &str) -> Vec<u8> {
    let mut objects = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(" 0 obj\n") {
        let body_start = start + " 0 obj\n".len();
        let end = rest[body_start..].find("\nendobj").unwrap() + body_start;
        objects.push(rest[body_start..end].to_string());
        rest = &rest[end..];
    }
    assemble(&objects)
}
