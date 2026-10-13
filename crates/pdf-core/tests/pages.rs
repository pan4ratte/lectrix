//! Moving and deleting pages (ADR 0018): the page tree changes where it must and nowhere
//! else, pages look the same in their new place, links and bookmarks follow them, labels
//! stay with the positions, and what led to deleted pages goes with them. And extracting
//! pages into a new file: they come as the document is now, with their labels and the
//! bookmarks and links among them, and the document is not changed.

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use common::{
    assemble, objects_written_after, open, out_dir, qpdf_check, raw_object, sample_file, writing,
};
use mupdf::pdf::{PdfDocument, PdfObject};
use pdf_core::Error;
use pdf_core::ops::Operation;
use pdf_core::outline::{Bookmark, Target, lookup_dest};
use pdf_core::save::SaveKind;
use pdf_core::session::{ExtractStep, Session};
use pdf_core::testgen::SampleSpec;

fn content(text: &str) -> String {
    let body = format!("BT /F1 24 Tf 20 150 Td ({text}) Tj ET");
    format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
}

/// "Book": six pages in two nodes. The first node turns its pages 90 degrees, the second
/// crops its pages to 200 x 200; the media box and the font come from the root. Labels
/// are i, ii, 1, 2, 3, 4. Page 1 links to page 4, to page 2 by name and to page 6; page 3
/// links to page 2; page 5 links to page 2 by its page number (as some producers write).
/// Page 2 has a sticky note, the field "Name" and one widget of the field "Shared", whose
/// other widget is on page 5. Bookmarks: "Part" (page 2) with "Two again" (page 2) and
/// "Four" (page 4) inside, "Two by name" and "Six". The structure tree has one element,
/// which is on page 2.
fn book() -> Vec<u8> {
    let page = |parent: u32, contents: u32, annots: &str| {
        format!("<< /Type /Page /Parent {parent} 0 R /Contents {contents} 0 R {annots} >>")
    };
    let link = |x: u32, target: &str| {
        format!(
            "<< /Type /Annot /Subtype /Link /Rect [{x} 10 {} 30] /Border [0 0 0] {target} >>",
            x + 40
        )
    };
    assemble(&[
        // 1
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 3 0 R >> /StructTreeRoot 28 0 R /Outlines 30 0 R \
          /PageLabels << /Nums [0 << /S /r >> 2 << /S /D >>] >> \
          /AcroForm << /Fields [24 0 R 25 0 R] /DA (/Helv 0 Tf 0 g) /CO [24 0 R] >> >>"
            .into(),
        // 2
        "<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 6 /MediaBox [0 0 300 400] \
          /Resources << /Font << /F1 29 0 R >> >> >>"
            .into(),
        // 3
        "<< /Names [(four) [9 0 R /Fit] (two) [7 0 R /Fit]] >>".into(),
        // 4-5: the two nodes
        "<< /Type /Pages /Parent 2 0 R /Kids [6 0 R 7 0 R 8 0 R] /Count 3 /Rotate 90 >>".into(),
        "<< /Type /Pages /Parent 2 0 R /Kids [9 0 R 10 0 R 11 0 R] /Count 3 /CropBox [0 0 200 200] >>".into(),
        // 6-11: pages
        page(4, 12, "/Annots [18 0 R 19 0 R 20 0 R]"),
        page(4, 13, "/Annots [22 0 R 24 0 R 26 0 R]"),
        page(4, 14, "/Annots [23 0 R]"),
        page(5, 15, ""),
        page(5, 16, "/Annots [21 0 R 27 0 R]"),
        page(5, 17, ""),
        // 12-17: contents
        content("Book 1"),
        content("Book 2"),
        content("Book 3"),
        content("Book 4"),
        content("Book 5"),
        content("Book 6"),
        // 18-20: links on page 1
        link(10, "/Dest [9 0 R /Fit]"),
        link(60, "/A << /S /GoTo /D (two) >>"),
        link(110, "/Dest [11 0 R /Fit]"),
        // 21: link on page 5, to page 2 by number
        link(10, "/Dest [1 /Fit]"),
        // 22: sticky note on page 2
        "<< /Type /Annot /Subtype /Text /Rect [20 300 40 320] /Contents (Note) /T (Ann) /P 7 0 R /NM (note-1) /F 4 >>".into(),
        // 23: link on page 3
        link(10, "/Dest [7 0 R /Fit]"),
        // 24: field "Name", its own widget, on page 2
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /V (Ada) /Rect [20 100 200 120] /P 7 0 R /F 4 >>".into(),
        // 25-27: field "Shared" with a widget on page 2 and one on page 5
        "<< /FT /Tx /T (Shared) /Kids [26 0 R 27 0 R] >>".into(),
        "<< /Type /Annot /Subtype /Widget /Parent 25 0 R /Rect [20 150 200 170] /P 7 0 R /F 4 >>".into(),
        "<< /Type /Annot /Subtype /Widget /Parent 25 0 R /Rect [20 150 200 170] /P 10 0 R /F 4 >>".into(),
        // 28: a structure tree with one paragraph, on page 2
        "<< /Type /StructTreeRoot /K [ << /Type /StructElem /S /P /P 28 0 R /Pg 7 0 R /K 0 >> ] >>".into(),
        // 29
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        // 30: outline
        "<< /Type /Outlines /First 31 0 R /Last 33 0 R /Count 3 >>".into(),
        // 31-33: top level
        "<< /Title (Part) /Parent 30 0 R /Next 32 0 R /First 34 0 R /Last 35 0 R /Count -2 /Dest [7 0 R /Fit] >>".into(),
        "<< /Title (Two by name) /Parent 30 0 R /Prev 31 0 R /Next 33 0 R /Dest (two) >>".into(),
        "<< /Title (Six) /Parent 30 0 R /Prev 32 0 R /Dest [11 0 R /Fit] >>".into(),
        // 34-35: inside "Part"
        "<< /Title (Two again) /Parent 31 0 R /Next 35 0 R /Dest [7 0 R /XYZ 0 400 null] >>".into(),
        "<< /Title (Four) /Parent 31 0 R /Prev 34 0 R /Dest [9 0 R /Fit] >>".into(),
    ])
}

fn book_file(dir: &Path) -> PathBuf {
    let path = dir.join("book.pdf");
    std::fs::write(&path, book()).unwrap();
    path
}

fn page_text(doc: &PdfDocument, page: i32) -> String {
    let page = doc.load_page(page).unwrap();
    page.to_text_page(mupdf::text_page::TextPageFlags::empty())
        .unwrap()
        .to_text()
        .unwrap()
}

/// The number each page shows ("Book 3" is 3), in document order.
fn order(doc: &PdfDocument) -> Vec<u32> {
    (0..doc.page_count().unwrap())
        .map(|p| {
            let text = page_text(doc, p);
            let digits: String = text.chars().filter(char::is_ascii_digit).collect();
            digits
                .parse()
                .unwrap_or_else(|_| panic!("page {p}: {text:?}"))
        })
        .collect()
}

fn annots(doc: &PdfDocument, page: i32) -> Vec<PdfObject> {
    match doc.find_page(page).unwrap().get_dict("Annots").unwrap() {
        Some(a) => (0..a.len().unwrap() as i32)
            .map(|i| a.get_array(i).unwrap().unwrap())
            .collect(),
        None => Vec::new(),
    }
}

/// The page a link leads to.
fn link_page(doc: &PdfDocument, item: &PdfObject) -> Option<i32> {
    let dest = match item.get_dict("Dest").unwrap() {
        Some(d) => d,
        None => item.get_dict("A").unwrap()?.get_dict("D").unwrap()?,
    };
    let array = if dest.is_array().unwrap() {
        dest
    } else {
        lookup_dest(doc, &dest.as_bytes().unwrap()).unwrap()?
    };
    Some(
        doc.lookup_page_number(&array.get_array(0).unwrap().unwrap())
            .unwrap(),
    )
}

/// Where the links on `page` lead, in the order of its annotations.
fn link_pages(doc: &PdfDocument, page: i32) -> Vec<Option<i32>> {
    annots(doc, page)
        .iter()
        .filter(|a| a.get_dict("Subtype").unwrap().unwrap().as_name().unwrap() == b"Link")
        .map(|a| link_page(doc, a))
        .collect()
}

fn sizes(session: &Session) -> Vec<(f32, f32, u16)> {
    session
        .info()
        .unwrap()
        .pages
        .iter()
        .map(|p| (p.width, p.height, p.rotation))
        .collect()
}

fn labels(session: &Session) -> Vec<String> {
    session.info().unwrap().labels.unwrap().labels
}

/// Title, target page and number of children of each bookmark.
fn titles(items: &[Bookmark]) -> Vec<(String, Option<usize>, usize)> {
    items
        .iter()
        .map(|b| {
            let page = match b.target {
                Target::Page { page, .. } => Some(page),
                _ => None,
            };
            (b.title.clone(), page, b.children.len())
        })
        .collect()
}

fn kids(doc: &PdfDocument, num: i32) -> Vec<i32> {
    let node = doc.new_indirect(num, 0).unwrap();
    let kids = node.get_dict("Kids").unwrap().unwrap();
    (0..kids.len().unwrap() as i32)
        .map(|i| kids.get_array(i).unwrap().unwrap().as_indirect().unwrap())
        .collect()
}

fn count(doc: &PdfDocument, num: i32) -> i32 {
    let node = doc.new_indirect(num, 0).unwrap();
    node.get_dict("Count").unwrap().unwrap().as_int().unwrap()
}

fn move_pages(pages: &[usize], to: usize) -> Operation {
    Operation::MovePages {
        pages: pages.to_vec(),
        to,
    }
}

const TURNED: (f32, f32, u16) = (400.0, 300.0, 90);
const CROPPED: (f32, f32, u16) = (200.0, 200.0, 0);

#[test]
fn moved_pages_look_the_same_and_links_follow_them() {
    let dir = out_dir("pages-move");
    let path = book_file(&dir);
    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.pages.len(), 6);
    assert_eq!(
        sizes(&session),
        [TURNED, TURNED, TURNED, CROPPED, CROPPED, CROPPED]
    );

    // Pages 1 and 2 to between pages 5 and 6: out of the turned node, into the cropped one.
    let change = session.apply(move_pages(&[1, 0], 5)).unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Move pages"));
    assert_eq!(change.page_count, 6);
    assert_eq!(
        change.page_map,
        Some(vec![Some(2), Some(3), Some(4), Some(0), Some(1), Some(5)])
    );
    // They stay turned and are not cropped: what they inherited went with them.
    assert_eq!(
        sizes(&session),
        [TURNED, CROPPED, CROPPED, TURNED, TURNED, CROPPED]
    );
    // Labels belong to the positions, so none changed.
    assert_eq!(change.labels, None);
    assert_eq!(labels(&session), ["i", "ii", "1", "2", "3", "4"]);
    // Bookmarks lead to the same pages, which are elsewhere now.
    let outline = change.outline.unwrap();
    assert_eq!(
        titles(&outline.items),
        [
            ("Part".to_string(), Some(4), 2),
            ("Two by name".to_string(), Some(4), 0),
            ("Six".to_string(), Some(5), 0)
        ]
    );
    assert_eq!(change.deleted, None);

    let undone = session.undo().unwrap();
    assert_eq!(
        undone.page_map,
        Some(vec![Some(3), Some(4), Some(0), Some(1), Some(2), Some(5)])
    );
    assert_eq!(
        sizes(&session),
        [TURNED, TURNED, TURNED, CROPPED, CROPPED, CROPPED]
    );
    assert_eq!(session.redo().unwrap().page_count, 6);

    let out = dir.join("moved.pdf");
    writing(|| session.save(SaveKind::Incremental, Some(out.clone()))).unwrap();
    session.close();
    qpdf_check(&out);
    let doc = open(&out);
    assert_eq!(order(&doc), [3, 4, 5, 1, 2, 6]);
    // The first node keeps its one page; the second lists the moved ones before page 6.
    assert_eq!(kids(&doc, 4), [8]);
    assert_eq!(count(&doc, 4), 1);
    assert_eq!(kids(&doc, 5), [9, 10, 6, 7, 11]);
    assert_eq!(count(&doc, 5), 5);
    assert_eq!(count(&doc, 2), 6);
    // Page 1's links: to page 4, to page 2 by name, to page 6.
    assert_eq!(link_pages(&doc, 3), [Some(1), Some(4), Some(5)]);
    // Page 5's link named page 2 by its number; it leads to that page still.
    assert_eq!(link_pages(&doc, 2), [Some(4)]);
    // The sticky note went with its page.
    let note = annots(&doc, 4)
        .into_iter()
        .find(|a| a.get_dict("NM").unwrap().is_some())
        .unwrap();
    assert_eq!(
        doc.lookup_page_number(&note.get_dict("P").unwrap().unwrap())
            .unwrap(),
        4
    );
}

#[test]
fn moving_within_a_node_rewrites_only_its_kids() {
    let dir = out_dir("pages-move-within");
    let path = book_file(&dir);
    let original = std::fs::metadata(&path).unwrap().len() as usize;
    let (session, _) = Session::open(&path, None).unwrap();
    // Page 3 to the front: all within the first node.
    let change = session.apply(move_pages(&[2], 0)).unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Move page"));
    let out = dir.join("moved.pdf");
    writing(|| session.save(SaveKind::Incremental, Some(out.clone()))).unwrap();
    session.close();
    qpdf_check(&out);
    let bytes = std::fs::read(&out).unwrap();
    // The node, and the link that named a page by its number (now a reference). No page,
    // no other node, not the catalog.
    let written = objects_written_after(&bytes, original);
    assert_eq!(written.into_iter().collect::<Vec<_>>(), [4, 21]);
    assert!(raw_object(&bytes, 21).contains("7 0 R"));
    let doc = open(&out);
    assert_eq!(order(&doc), [3, 1, 2, 4, 5, 6]);
    assert_eq!(kids(&doc, 4), [8, 6, 7]);
}

#[test]
fn a_move_that_leaves_every_page_in_place_changes_nothing() {
    let dir = out_dir("pages-move-nothing");
    let path = book_file(&dir);
    let (session, info) = Session::open(&path, None).unwrap();
    let in_place: [(&[usize], usize); 4] = [
        (&[1, 2], 1),
        (&[1, 2], 2),
        (&[1, 2], 3),
        (&[0, 1, 2, 3, 4, 5], 4),
    ];
    for (pages, to) in in_place {
        let change = session.apply(move_pages(pages, to)).unwrap();
        assert_eq!(
            change.state.revision, info.state.revision,
            "{pages:?} to {to}"
        );
        assert!(!change.state.dirty);
        assert_eq!(change.page_map, None);
    }
    // Pages that are apart come together, in their order.
    let change = session.apply(move_pages(&[4, 0], 2)).unwrap();
    assert_eq!(
        change.page_map,
        Some(vec![Some(1), Some(0), Some(4), Some(2), Some(3), Some(5)])
    );
    session.close();
}

#[test]
fn a_node_left_without_pages_leaves_the_tree() {
    let dir = out_dir("pages-move-prune");
    let path = book_file(&dir);
    let (session, _) = Session::open(&path, None).unwrap();
    // Every page of the second node, to the front: into the first node, which turns its
    // pages. These were not turned, and must not be now.
    session.apply(move_pages(&[3, 4, 5], 0)).unwrap();
    assert_eq!(
        sizes(&session),
        [CROPPED, CROPPED, CROPPED, TURNED, TURNED, TURNED]
    );
    let out = dir.join("moved.pdf");
    writing(|| session.save(SaveKind::Incremental, Some(out.clone()))).unwrap();
    session.close();
    qpdf_check(&out);
    let doc = open(&out);
    assert_eq!(order(&doc), [4, 5, 6, 1, 2, 3]);
    assert_eq!(kids(&doc, 2), [4]);
    assert_eq!(kids(&doc, 4), [9, 10, 11, 6, 7, 8]);
    assert_eq!((count(&doc, 2), count(&doc, 4)), (6, 6));
    // A page that moved has its own media box and resources now, and no rotation.
    let moved = doc.find_page(0).unwrap();
    for key in ["MediaBox", "CropBox", "Resources"] {
        assert!(moved.get_dict(key).unwrap().is_some(), "{key}");
    }
    assert_eq!(
        moved.get_dict("Rotate").unwrap().unwrap().as_int().unwrap(),
        0
    );
    assert!(page_text(&doc, 0).contains("Book 4"));
}

#[test]
fn deleting_pages_takes_what_led_to_them() {
    let dir = out_dir("pages-delete");
    let path = book_file(&dir);
    let (session, _) = Session::open(&path, None).unwrap();

    // Pages 2 and 3.
    let change = session
        .apply(Operation::DeletePages { pages: vec![2, 1] })
        .unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Delete pages"));
    assert_eq!(change.page_count, 4);
    assert_eq!(
        change.page_map,
        Some(vec![Some(0), Some(3), Some(4), Some(5)])
    );
    let report = change.deleted.unwrap();
    assert_eq!(report.pages, 2);
    // Page 1's link by name and page 5's link by number (page 3's own link went with it).
    assert_eq!(report.links, 2);
    // "Two by name" and "Two again"; "Part" has "Four" inside and stays as a heading.
    assert_eq!(report.bookmarks, 2);
    // "Name"; "Shared" keeps its widget on page 5.
    assert_eq!(report.fields, 1);
    assert_eq!(sizes(&session), [TURNED, CROPPED, CROPPED, CROPPED]);
    // The body started on page 3 and starts on page 2 now: the numbering closes up.
    assert_eq!(labels(&session), ["i", "1", "2", "3"]);
    let outline = change.outline.unwrap();
    assert_eq!(
        titles(&outline.items),
        [
            ("Part".to_string(), None, 1),
            ("Six".to_string(), Some(3), 0)
        ]
    );
    assert_eq!(
        titles(&outline.items[0].children),
        [("Four".to_string(), Some(1), 0)]
    );
    assert_eq!(outline.items[0].target, Target::None);

    let undone = session.undo().unwrap();
    assert_eq!(undone.page_count, 6);
    assert_eq!(undone.deleted, None);
    assert_eq!(labels(&session), ["i", "ii", "1", "2", "3", "4"]);
    assert_eq!(titles(&undone.outline.unwrap().items).len(), 3);
    assert_eq!(session.redo().unwrap().page_count, 4);

    let out = dir.join("deleted.pdf");
    writing(|| session.save(SaveKind::Incremental, Some(out.clone()))).unwrap();
    qpdf_check(&out);
    let doc = open(&out);
    assert_eq!(order(&doc), [1, 4, 5, 6]);
    assert_eq!(kids(&doc, 4), [6]);
    assert_eq!((count(&doc, 2), count(&doc, 4), count(&doc, 5)), (4, 1, 3));
    // Page 1 keeps its links to pages 4 and 6; page 5 keeps the widget alone.
    assert_eq!(link_pages(&doc, 0), [Some(1), Some(3)]);
    assert!(link_pages(&doc, 2).is_empty());
    assert_eq!(annots(&doc, 2).len(), 1);
    // The name that led to page 2 is gone, the other stays.
    assert!(lookup_dest(&doc, b"two").unwrap().is_none());
    assert!(lookup_dest(&doc, b"four").unwrap().is_some());
    // The form: "Shared" with one widget, nothing in the calculation order.
    let form = doc
        .catalog()
        .unwrap()
        .get_dict("AcroForm")
        .unwrap()
        .unwrap();
    let fields = form.get_dict("Fields").unwrap().unwrap();
    assert_eq!(fields.len().unwrap(), 1);
    assert_eq!(
        fields.get_array(0).unwrap().unwrap().as_indirect().unwrap(),
        25
    );
    assert_eq!(kids(&doc, 25), [27]);
    assert_eq!(form.get_dict("CO").unwrap().unwrap().len().unwrap(), 0);
    // The deleted pages' objects hold nothing.
    for num in [7, 8] {
        let page = doc.new_indirect(num, 0).unwrap();
        assert_eq!(page.dict_len().unwrap(), 1, "object {num}");
    }

    // An optimized save leaves none of the deleted pages' content in the file, though the
    // structure tree still names page 2.
    let optimized = dir.join("optimized.pdf");
    writing(|| session.save(SaveKind::Optimized, Some(optimized.clone()))).unwrap();
    session.close();
    qpdf_check(&optimized);
    let doc = open(&optimized);
    assert_eq!(order(&doc), [1, 4, 5, 6]);
    let mut streams = Vec::new();
    for num in 1..doc.xref_len().unwrap() as i32 {
        let obj = doc.new_indirect(num, 0).unwrap();
        if obj.is_stream().unwrap_or(false) {
            streams.push(String::from_utf8_lossy(&obj.read_stream().unwrap()).into_owned());
        }
    }
    assert!(streams.iter().any(|s| s.contains("Book 4")));
    assert!(
        !streams
            .iter()
            .any(|s| s.contains("Book 2") || s.contains("Book 3"))
    );
}

#[test]
fn deleting_a_whole_label_range_drops_its_rule() {
    let dir = out_dir("pages-delete-labels");
    let path = book_file(&dir);
    let (session, _) = Session::open(&path, None).unwrap();
    // The front matter: the document is numbered 1, 2, 3, 4.
    let change = session
        .apply(Operation::DeletePages { pages: vec![0, 1] })
        .unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Delete pages"));
    assert_eq!(labels(&session), ["1", "2", "3", "4"]);
    let rules = session.info().unwrap().labels.unwrap().rules;
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].start_page, 0);
    // One page from the end changes no rule.
    let change = session
        .apply(Operation::DeletePages { pages: vec![3] })
        .unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Delete page"));
    assert_eq!(labels(&session), ["1", "2", "3"]);
    let out = dir.join("deleted.pdf");
    writing(|| session.save(SaveKind::Incremental, Some(out.clone()))).unwrap();
    session.close();
    qpdf_check(&out);
    assert_eq!(order(&open(&out)), [3, 4, 5]);
}

#[test]
fn a_document_keeps_at_least_one_page() {
    let dir = out_dir("pages-delete-all");
    let path = book_file(&dir);
    let (session, info) = Session::open(&path, None).unwrap();
    let all = Operation::DeletePages {
        pages: (0..6).collect(),
    };
    assert!(matches!(session.apply(all), Err(Error::InvalidArgument(_))));
    assert!(matches!(
        session.apply(Operation::DeletePages { pages: vec![6] }),
        Err(Error::PageOutOfRange(6))
    ));
    assert_eq!(session.info().unwrap().state.revision, info.state.revision);
    // All but one is fine.
    let change = session
        .apply(Operation::DeletePages {
            pages: (0..5).collect(),
        })
        .unwrap();
    assert_eq!(change.page_count, 1);
    assert_eq!(change.page_map, Some(vec![Some(5)]));
    session.close();
}

#[test]
fn a_page_listed_twice_is_not_rearranged() {
    let dir = out_dir("pages-damaged");
    let path = dir.join("twice.pdf");
    std::fs::write(
        &path,
        assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R 3 0 R 4 0 R] /Count 3 /MediaBox [0 0 300 400] >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
        ]),
    )
    .unwrap();
    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.pages.len(), 3);
    assert!(matches!(
        session.apply(move_pages(&[2], 0)),
        Err(Error::DamagedPageTree)
    ));
    assert!(matches!(
        session.apply(Operation::DeletePages { pages: vec![0] }),
        Err(Error::DamagedPageTree)
    ));
    let after = session.info().unwrap();
    assert_eq!(after.pages.len(), 3);
    assert!(!after.state.dirty);
    session.close();
}

#[test]
fn extracted_pages_come_as_the_document_is_now() {
    let dir = out_dir("pages-extract");
    let path = book_file(&dir);
    let original = std::fs::read(&path).unwrap();
    let (session, _) = Session::open(&path, None).unwrap();
    // An unsaved change: page 6 turned.
    session
        .apply(Operation::RotatePages {
            pages: vec![5],
            degrees: 90,
        })
        .unwrap();
    let before = session.info().unwrap().state;

    // Pages 6, 1 and 4, in that order.
    let out = dir.join("extract.pdf");
    let steps = Arc::new(Mutex::new(Vec::new()));
    let log = steps.clone();
    let report = writing(|| {
        session.extract(vec![5, 0, 3], &out, move |step| {
            log.lock().unwrap().push(step);
            true
        })
    })
    .unwrap();
    assert_eq!(report.pages, 3);
    // Page 1's link to page 2, and the two bookmarks that led there.
    assert_eq!(report.dropped_links, 1);
    assert_eq!(report.dropped_bookmarks, 2);
    assert_eq!(
        *steps.lock().unwrap(),
        [
            ExtractStep::Copying { done: 1, total: 3 },
            ExtractStep::Copying { done: 2, total: 3 },
            ExtractStep::Copying { done: 3, total: 3 },
            ExtractStep::Writing,
            ExtractStep::Replacing
        ]
    );
    // The document is as it was, and so is its file.
    assert_eq!(session.info().unwrap().state, before);
    assert_eq!(session.info().unwrap().pages.len(), 6);
    session.close();
    assert_eq!(std::fs::read(&path).unwrap(), original);

    qpdf_check(&out);
    let doc = open(&out);
    assert_eq!(order(&doc), [6, 1, 4]);
    // Page 1 keeps its links to pages 4 and 6, which are pages 3 and 1 here.
    assert_eq!(link_pages(&doc, 1), [Some(2), Some(0)]);
    assert!(lookup_dest(&doc, b"four").unwrap().is_some());
    assert!(lookup_dest(&doc, b"two").unwrap().is_none());
    // Not carried over: the structure tree.
    assert!(
        doc.catalog()
            .unwrap()
            .get_dict("StructTreeRoot")
            .unwrap()
            .is_none()
    );
    drop(doc);

    let (extract, info) = Session::open(&out, None).unwrap();
    // Each page looks as it did, page 6 with the turn that was not saved yet.
    assert_eq!(sizes(&extract), [(200.0, 200.0, 90), TURNED, CROPPED]);
    // Each keeps its label.
    assert_eq!(labels(&extract), ["4", "i", "2"]);
    // "Part" led to page 2, which stayed behind; "Four" inside it keeps it as a heading.
    assert_eq!(
        titles(&info.outline.items),
        [
            ("Part".to_string(), None, 1),
            ("Six".to_string(), Some(0), 0)
        ]
    );
    assert_eq!(
        titles(&info.outline.items[0].children),
        [("Four".to_string(), Some(2), 0)]
    );
    assert!(!info.state.dirty);
    extract.close();
}

#[test]
fn a_stopped_extraction_writes_nothing() {
    let dir = out_dir("pages-extract-stopped");
    let path = book_file(&dir);
    let (session, _) = Session::open(&path, None).unwrap();
    let out = dir.join("extract.pdf");
    // (The folder is kept between runs.)
    let _ = std::fs::remove_file(&out);
    // Stopped after the first page.
    let result = session.extract(vec![0, 1, 2], &out, |step| {
        step != ExtractStep::Copying { done: 1, total: 3 }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!out.exists());
    // Stopped once the file is written: a file of that name stays as it was.
    std::fs::write(&out, b"keep me").unwrap();
    let result = writing(|| session.extract(vec![0], &out, |step| step != ExtractStep::Replacing));
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
    let left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        left.iter().all(|n| !n.ends_with(".lectrix-tmp")),
        "no temporary file left: {left:?}"
    );

    // What cannot be extracted: nothing, a page twice, a page that is not there, and
    // anything into the document's own file.
    for pages in [vec![], vec![1, 1], vec![6]] {
        assert!(
            session.extract(pages.clone(), &out, |_| true).is_err(),
            "{pages:?}"
        );
    }
    assert!(matches!(
        session.extract(vec![0], &path, |_| true),
        Err(Error::InvalidArgument(_))
    ));
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
    session.close();
}

#[test]
fn pages_of_a_document_without_labels_get_none() {
    let dir = out_dir("pages-extract-plain");
    let path = sample_file(
        &dir,
        "plain.pdf",
        SampleSpec {
            pages: 5,
            ..SampleSpec::default()
        },
    );
    let (session, _) = Session::open(&path, None).unwrap();
    let out = dir.join("extract.pdf");
    let report = writing(|| session.extract(vec![3, 4], &out, |_| true)).unwrap();
    session.close();
    assert_eq!(report.pages, 2);
    qpdf_check(&out);
    let doc = open(&out);
    assert_eq!(doc.page_count().unwrap(), 2);
    assert!(
        doc.catalog()
            .unwrap()
            .get_dict("PageLabels")
            .unwrap()
            .is_none()
    );
}
