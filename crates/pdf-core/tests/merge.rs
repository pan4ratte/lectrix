//! Stitching (AGENTS.md section 6.4, Phase 4): annotations travel with their pages, links
//! and named destinations follow the pages, colliding names and form fields are renamed,
//! picked pages can be reordered and rotated, and inserting pages is one undo step.

mod common;

use std::path::{Path, PathBuf};

use common::{assemble, open, out_dir, qpdf_check, writing};
use mupdf::pdf::{PdfDocument, PdfObject};
use pdf_core::Error;
use pdf_core::merge::{
    self, BookmarkMode, InsertLabels, InsertOptions, LabelMode, MergeOptions, MergeSource, PagePick,
};
use pdf_core::ops::{InsertSource, Operation};
use pdf_core::outline::{Target, lookup_dest, read_bookmarks};
use pdf_core::save::{SaveKind, save_atomic};
use pdf_core::session::Session;

fn content(text: &str) -> String {
    let body = format!("BT /F1 24 Tf 20 350 Td ({text}) Tj ET");
    format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
}

/// "Alpha": four pages with labels (i, ii, 1, 2), named destinations in a name tree, links
/// (explicit, named, to the last page, to the web), a sticky note with its popup and a
/// reply on the last page, a text field "Name", a two-item outline and a hidden layer.
/// Resources and the media box are inherited from the page tree.
fn alpha() -> Vec<u8> {
    let page = |n: u32, annots: &str| {
        format!("<< /Type /Page /Parent 2 0 R /Contents {n} 0 R {annots} >>")
    };
    assemble(&[
        // 1
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 3 0 R >> \
          /AcroForm << /Fields [20 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 22 0 R >> >> /NeedAppearances true >> \
          /Outlines 24 0 R /PageLabels << /Nums [0 << /S /r >> 2 << /S /D >>] >> \
          /OCProperties << /OCGs [25 0 R] /D << /OFF [25 0 R] /Order [25 0 R] >> >> >>"
            .into(),
        // 2
        "<< /Type /Pages /Kids [4 0 R 5 0 R 6 0 R 7 0 R] /Count 4 /MediaBox [0 0 300 400] \
          /Resources << /Font << /F1 23 0 R >> >> >>"
            .into(),
        // 3
        "<< /Names [(chap2) [5 0 R /Fit] (end) [7 0 R /XYZ 0 400 null]] >>".into(),
        // 4-7: pages
        page(8, "/Annots [12 0 R 13 0 R 14 0 R 15 0 R]"),
        page(9, "/Annots [16 0 R 17 0 R 20 0 R]"),
        page(10, ""),
        page(11, "/Annots [18 0 R]"),
        // 8-11: contents
        content("Alpha 1"),
        content("Alpha 2"),
        content("Alpha 3"),
        content("Alpha 4"),
        // 12-15: links on page 1
        "<< /Type /Annot /Subtype /Link /Rect [10 10 60 30] /Border [0 0 0] /Dest [6 0 R /Fit] >>".into(),
        "<< /Type /Annot /Subtype /Link /Rect [70 10 120 30] /Border [0 0 0] /A << /S /GoTo /D (chap2) >> >>".into(),
        "<< /Type /Annot /Subtype /Link /Rect [130 10 180 30] /Border [0 0 0] /Dest [7 0 R /Fit] >>".into(),
        "<< /Type /Annot /Subtype /Link /Rect [190 10 240 30] /Border [0 0 0] /A << /S /URI /URI (https://example.org/) >> >>".into(),
        // 16-17: sticky note and its popup on page 2
        "<< /Type /Annot /Subtype /Text /Rect [20 300 40 320] /Contents (Note) /T (Ann) /P 5 0 R /Popup 17 0 R /NM (note-1) /F 4 >>".into(),
        "<< /Type /Annot /Subtype /Popup /Rect [40 200 200 300] /Parent 16 0 R /P 5 0 R >>".into(),
        // 18: a reply on page 4
        "<< /Type /Annot /Subtype /Text /Rect [20 300 40 320] /Contents (Reply) /IRT 16 0 R /P 7 0 R >>".into(),
        // 19: unused
        "<< >>".into(),
        // 20: text field "Name", its own widget, on page 2
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /V (Ada) /Rect [20 100 200 120] /P 5 0 R /F 4 >>".into(),
        // 21: unused
        "<< >>".into(),
        // 22-23: fonts
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        // 24: outline
        "<< /Type /Outlines /First 26 0 R /Last 27 0 R /Count 2 >>".into(),
        // 25: layer
        "<< /Type /OCG /Name (Layer A) >>".into(),
        // 26-27: bookmarks
        "<< /Title (Chapter 2) /Parent 24 0 R /Next 27 0 R /Dest (chap2) >>".into(),
        "<< /Title (End) /Parent 24 0 R /Prev 26 0 R /Dest [7 0 R /Fit] >>".into(),
    ])
}

/// "Beta": three pages without labels, a PDF 1.1 `/Dests` dictionary that also defines
/// "chap2", a link using it as a name object, a text field "Name" (colliding with Alpha's)
/// and a field "Shared" with widgets on pages 1 and 3.
fn beta() -> Vec<u8> {
    let page = |n: u32, annots: &str| {
        format!(
            "<< /Type /Page /Parent 2 0 R /Contents {n} 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 15 0 R >> >> {annots} >>"
        )
    };
    assemble(&[
        // 1
        "<< /Type /Catalog /Pages 2 0 R /Dests 3 0 R /AcroForm << /Fields [10 0 R 12 0 R] >> >>".into(),
        // 2
        "<< /Type /Pages /Kids [4 0 R 5 0 R 6 0 R] /Count 3 >>".into(),
        // 3
        "<< /chap2 [5 0 R /XYZ 0 400 0] >>".into(),
        // 4-6
        page(7, "/Annots [10 0 R 11 0 R 13 0 R]"),
        page(8, ""),
        page(9, "/Annots [14 0 R]"),
        // 7-9
        content("Beta 1"),
        content("Beta 2"),
        content("Beta 3"),
        // 10: field "Name"
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /Rect [20 100 200 120] /P 4 0 R /F 4 >>".into(),
        // 11: link with a name-object destination
        "<< /Type /Annot /Subtype /Link /Rect [10 10 60 30] /Border [0 0 0] /Dest /chap2 >>".into(),
        // 12: field "Shared" with two widgets
        "<< /FT /Tx /T (Shared) /Kids [13 0 R 14 0 R] >>".into(),
        "<< /Type /Annot /Subtype /Widget /Parent 12 0 R /Rect [20 150 200 170] /P 4 0 R /F 4 >>".into(),
        "<< /Type /Annot /Subtype /Widget /Parent 12 0 R /Rect [20 150 200 170] /P 6 0 R /F 4 >>".into(),
        // 15
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ])
}

fn write(dir: &Path, name: &str, bytes: Vec<u8>) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn sources(dir: &Path) -> (PathBuf, PathBuf) {
    (
        write(dir, "alpha.pdf", alpha()),
        write(dir, "beta.pdf", beta()),
    )
}

fn save(doc: &PdfDocument, path: &Path) -> PdfDocument {
    writing(|| save_atomic(doc, SaveKind::Full, None, path)).unwrap();
    qpdf_check(path);
    open(path)
}

fn name(obj: &PdfObject, key: &str) -> Option<Vec<u8>> {
    obj.get_dict(key).unwrap().map(|v| v.as_name().unwrap())
}

fn page_text(doc: &PdfDocument, page: i32) -> String {
    let page = doc.load_page(page).unwrap();
    page.to_text_page(mupdf::text_page::TextPageFlags::empty())
        .unwrap()
        .to_text()
        .unwrap()
}

fn annots(doc: &PdfDocument, page: i32) -> Vec<PdfObject> {
    match doc.find_page(page).unwrap().get_dict("Annots").unwrap() {
        Some(a) => (0..a.len().unwrap() as i32)
            .map(|i| a.get_array(i).unwrap().unwrap())
            .collect(),
        None => Vec::new(),
    }
}

fn page_of(doc: &PdfDocument, page_ref: &PdfObject) -> i32 {
    doc.lookup_page_number(page_ref).unwrap()
}

/// The page a link (or bookmark) leads to.
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
    Some(page_of(doc, &array.get_array(0).unwrap().unwrap()))
}

fn field_names(doc: &PdfDocument) -> Vec<String> {
    let fields = doc
        .catalog()
        .unwrap()
        .get_dict("AcroForm")
        .unwrap()
        .unwrap()
        .get_dict("Fields")
        .unwrap()
        .unwrap();
    (0..fields.len().unwrap() as i32)
        .map(|i| {
            let f = fields.get_array(i).unwrap().unwrap();
            f.get_dict("T").unwrap().unwrap().as_string().unwrap()
        })
        .collect()
}

fn labels(doc: &PdfDocument) -> Vec<String> {
    (0..doc.page_count().unwrap() as usize)
        .map(|p| doc.page_label(p).unwrap())
        .collect()
}

fn titles(items: &[pdf_core::outline::Bookmark]) -> Vec<(String, Option<usize>, usize)> {
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

#[test]
fn combining_whole_files_keeps_annotations_links_names_fields_and_layers() {
    let dir = out_dir("merge-whole");
    let (a, b) = sources(&dir);
    let srcs = [
        MergeSource::open(&a, None).unwrap(),
        MergeSource::open(&b, None).unwrap(),
    ];
    let (merged, report) = merge::merge_all(&srcs, MergeOptions::default()).unwrap();
    assert_eq!(report.pages, 7);
    assert_eq!(report.renamed_destinations, 1, "Beta's chap2");
    assert_eq!(report.renamed_fields, 1, "Beta's Name");
    assert_eq!(report.dropped_links, 0);
    let out = save(&merged, &dir.join("merged.pdf"));

    assert_eq!(out.page_count().unwrap(), 7);
    for (i, text) in [
        "Alpha 1", "Alpha 2", "Alpha 3", "Alpha 4", "Beta 1", "Beta 2", "Beta 3",
    ]
    .iter()
    .enumerate()
    {
        assert!(page_text(&out, i as i32).contains(text), "page {i}");
    }
    assert_eq!(labels(&out), ["i", "ii", "1", "2", "1", "2", "3"]);

    // Every annotation sits on its page and says so in /P.
    for page in 0..7 {
        for annot in annots(&out, page) {
            if let Some(p) = annot.get_dict("P").unwrap() {
                assert_eq!(page_of(&out, &p), page);
            }
        }
    }
    // Alpha's links: explicit, named and to the last page; the web link stays.
    let links = annots(&out, 0);
    assert_eq!(links.len(), 4);
    assert_eq!(link_page(&out, &links[0]), Some(2));
    assert_eq!(link_page(&out, &links[1]), Some(1));
    assert_eq!(link_page(&out, &links[2]), Some(3));
    let uri = links[3].get_dict("A").unwrap().unwrap();
    assert_eq!(
        uri.get_dict("URI").unwrap().unwrap().as_string().unwrap(),
        "https://example.org/"
    );

    // Beta's link used the name object /chap2; it now uses the renamed string.
    let beta_link = annots(&out, 4)
        .into_iter()
        .find(|a| name(a, "Subtype").as_deref() == Some(b"Link"))
        .unwrap();
    let dest = beta_link.get_dict("Dest").unwrap().unwrap();
    assert!(dest.is_string().unwrap());
    assert_eq!(dest.as_bytes().unwrap(), b"src2_chap2");
    assert_eq!(link_page(&out, &beta_link), Some(5));
    // Both names resolve in the result.
    let chap2 = lookup_dest(&out, b"chap2").unwrap().unwrap();
    assert_eq!(page_of(&out, &chap2.get_array(0).unwrap().unwrap()), 1);
    let end = lookup_dest(&out, b"end").unwrap().unwrap();
    assert_eq!(page_of(&out, &end.get_array(0).unwrap().unwrap()), 3);

    // The note and its popup point at each other; the reply points at the note.
    let page2 = annots(&out, 1);
    let note = &page2[0];
    let popup = note.get_dict("Popup").unwrap().unwrap();
    assert_eq!(
        popup.as_indirect().unwrap(),
        page2[1].as_indirect().unwrap()
    );
    assert_eq!(
        popup
            .get_dict("Parent")
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap(),
        note.as_indirect().unwrap()
    );
    let reply = &annots(&out, 3)[0];
    assert_eq!(
        reply
            .get_dict("IRT")
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap(),
        note.as_indirect().unwrap()
    );

    // Form: Alpha's Name, Beta's renamed Name, and Shared with both widgets.
    assert_eq!(field_names(&out), ["Name", "src2_Name", "Shared"]);
    let form = out
        .catalog()
        .unwrap()
        .get_dict("AcroForm")
        .unwrap()
        .unwrap();
    assert!(
        form.get_dict("NeedAppearances")
            .unwrap()
            .unwrap()
            .as_bool()
            .unwrap()
    );
    assert!(
        form.get_dict("DR")
            .unwrap()
            .unwrap()
            .get_dict("Font")
            .unwrap()
            .unwrap()
            .get_dict("Helv")
            .unwrap()
            .is_some()
    );

    // Layers: Alpha's layer stays listed and hidden.
    let props = out
        .catalog()
        .unwrap()
        .get_dict("OCProperties")
        .unwrap()
        .unwrap();
    assert_eq!(props.get_dict("OCGs").unwrap().unwrap().len().unwrap(), 1);
    let off = props
        .get_dict("D")
        .unwrap()
        .unwrap()
        .get_dict("OFF")
        .unwrap()
        .unwrap();
    assert_eq!(off.len().unwrap(), 1);

    // Bookmarks nested under each source; Alpha's keep their named destination.
    let outline = read_bookmarks(&out).unwrap();
    assert_eq!(
        titles(&outline.items),
        [
            ("alpha".to_string(), Some(0), 2),
            ("beta".to_string(), Some(4), 0)
        ]
    );
    assert_eq!(
        titles(&outline.items[0].children),
        [
            ("Chapter 2".to_string(), Some(1), 0),
            ("End".to_string(), Some(3), 0)
        ]
    );
}

#[test]
fn picked_pages_are_reordered_rotated_and_take_only_what_they_need() {
    let dir = out_dir("merge-picked");
    let (a, b) = sources(&dir);
    let srcs = [
        MergeSource::open(&a, None).unwrap(),
        MergeSource::open(&b, None).unwrap(),
    ];
    // Beta page 3 turned, then Alpha pages 3 and 2. Alpha 1 and 4 and Beta 1 and 2 are out.
    let picks = [
        PagePick {
            source: 1,
            page: 2,
            rotate: 90,
        },
        PagePick {
            source: 0,
            page: 2,
            rotate: 0,
        },
        PagePick {
            source: 0,
            page: 1,
            rotate: -90,
        },
    ];
    let mut seen = Vec::new();
    let (merged, report) = merge::merge(
        &srcs,
        &picks,
        MergeOptions::default(),
        &mut |done, total| {
            seen.push((done, total));
            true
        },
    )
    .unwrap();
    assert_eq!(seen, [(1, 3), (2, 3), (3, 3)]);
    assert_eq!(report.dropped_bookmarks, 1, "End leads to Alpha 4");
    assert_eq!(report.renamed_fields, 0, "Beta's Name stayed behind");
    let out = save(&merged, &dir.join("picked.pdf"));

    assert_eq!(out.page_count().unwrap(), 3);
    assert!(page_text(&out, 0).contains("Beta 3"));
    assert!(page_text(&out, 1).contains("Alpha 3"));
    assert!(page_text(&out, 2).contains("Alpha 2"));
    let rotate = |p: i32| {
        out.find_page(p)
            .unwrap()
            .get_dict("Rotate")
            .unwrap()
            .map_or(0, |r| r.as_int().unwrap())
    };
    assert_eq!([rotate(0), rotate(1), rotate(2)], [90, 0, 270]);
    // The inherited media box and resources were written on the pages.
    let p1 = out.find_page(1).unwrap();
    assert!(p1.get_dict("MediaBox").unwrap().is_some());
    assert!(p1.get_dict("Resources").unwrap().is_some());

    // Labels as each page had them: Beta 3 -> "3", Alpha 3 -> "1", Alpha 2 -> "ii".
    assert_eq!(labels(&out), ["3", "1", "ii"]);

    // Only names whose page came along; Alpha's chap2 now leads to page 3.
    let chap2 = lookup_dest(&out, b"chap2").unwrap().unwrap();
    assert_eq!(page_of(&out, &chap2.get_array(0).unwrap().unwrap()), 2);
    assert!(lookup_dest(&out, b"end").unwrap().is_none());

    // Shared keeps only the widget on the copied page; Beta's Name did not come along.
    assert_eq!(field_names(&out), ["Name", "Shared"]);
    let fields = out
        .catalog()
        .unwrap()
        .get_dict("AcroForm")
        .unwrap()
        .unwrap()
        .get_dict("Fields")
        .unwrap()
        .unwrap();
    let shared = fields.get_array(1).unwrap().unwrap();
    assert_eq!(shared.get_dict("Kids").unwrap().unwrap().len().unwrap(), 1);

    // Bookmarks: Beta first (its page comes first), Alpha with Chapter 2 only.
    let outline = read_bookmarks(&out).unwrap();
    assert_eq!(
        titles(&outline.items),
        [
            ("beta".to_string(), Some(0), 0),
            ("alpha".to_string(), Some(1), 1)
        ]
    );
    assert_eq!(
        titles(&outline.items[1].children),
        [("Chapter 2".to_string(), Some(2), 0)]
    );

    // Nothing of the pages left behind was copied: no content stream mentions them.
    for num in 1..out.count_objects().unwrap() as i32 {
        if let Some(obj) = out.xref_object(num).unwrap()
            && obj.is_stream().unwrap()
        {
            let data = String::from_utf8_lossy(&obj.read_stream().unwrap()).into_owned();
            for gone in ["Alpha 1", "Alpha 4", "Beta 1", "Beta 2"] {
                assert!(!data.contains(gone), "object {num} holds {gone}");
            }
        }
    }
}

#[test]
fn links_to_pages_left_out_are_dropped_and_options_are_honored() {
    let dir = out_dir("merge-options");
    let (a, b) = sources(&dir);
    let srcs = [
        MergeSource::open(&a, None).unwrap(),
        MergeSource::open(&b, None).unwrap(),
    ];
    // Alpha page 1 without page 4: its link to page 4 goes.
    let picks: Vec<PagePick> = [0, 1, 2]
        .iter()
        .map(|&page| PagePick {
            source: 0,
            page,
            rotate: 0,
        })
        .collect();
    let options = MergeOptions {
        bookmarks: BookmarkMode::Flat,
        labels: LabelMode::None,
    };
    let (merged, report) = merge::merge(&srcs, &picks, options, &mut |_, _| true).unwrap();
    assert_eq!(report.dropped_links, 1);
    let out = save(&merged, &dir.join("flat.pdf"));
    assert_eq!(annots(&out, 0).len(), 3);
    assert!(
        out.catalog()
            .unwrap()
            .get_dict("PageLabels")
            .unwrap()
            .is_none()
    );
    let outline = read_bookmarks(&out).unwrap();
    assert_eq!(
        titles(&outline.items),
        [("Chapter 2".to_string(), Some(1), 0)]
    );

    let options = MergeOptions {
        bookmarks: BookmarkMode::Drop,
        labels: LabelMode::Continuous,
    };
    let (merged, _) = merge::merge_all(&srcs, options).unwrap();
    let out = save(&merged, &dir.join("continuous.pdf"));
    assert!(
        out.catalog()
            .unwrap()
            .get_dict("Outlines")
            .unwrap()
            .is_none()
    );
    assert_eq!(labels(&out), ["1", "2", "3", "4", "5", "6", "7"]);
}

#[test]
fn attached_files_come_along_and_colliding_names_are_renamed() {
    use mupdf::pdf::EmbeddedFileOptions;
    let dir = out_dir("merge-attachments");
    let (a, b) = sources(&dir);
    let mut srcs = [
        MergeSource::open(&a, None).unwrap(),
        MergeSource::open(&b, None).unwrap(),
    ];
    for (source, text) in srcs.iter_mut().zip([&b"from alpha"[..], b"from beta"]) {
        source
            .doc
            .add_embedded_file("notes.txt", text, EmbeddedFileOptions::new("notes.txt"))
            .unwrap();
    }
    let (merged, report) = merge::merge_all(&srcs, MergeOptions::default()).unwrap();
    assert_eq!(report.renamed_attachments, 1);
    let out = save(&merged, &dir.join("attachments.pdf"));
    assert_eq!(
        out.load_embedded_file("notes.txt").unwrap().unwrap(),
        b"from alpha"
    );
    assert_eq!(
        out.load_embedded_file("src2_notes.txt").unwrap().unwrap(),
        b"from beta"
    );
}

#[test]
fn stopping_reports_cancelled() {
    let dir = out_dir("merge-cancel");
    let (a, b) = sources(&dir);
    let srcs = [
        MergeSource::open(&a, None).unwrap(),
        MergeSource::open(&b, None).unwrap(),
    ];
    let picks = [
        PagePick {
            source: 0,
            page: 0,
            rotate: 0,
        },
        PagePick {
            source: 1,
            page: 0,
            rotate: 0,
        },
    ];
    let result = merge::merge(&srcs, &picks, MergeOptions::default(), &mut |done, _| {
        done < 1
    });
    assert!(matches!(result, Err(Error::Cancelled)));

    let twice = [picks[0], picks[0]];
    assert!(merge::merge(&srcs, &twice, MergeOptions::default(), &mut |_, _| true).is_err());
    let turned = [PagePick {
        rotate: 45,
        ..picks[0]
    }];
    assert!(merge::merge(&srcs, &turned, MergeOptions::default(), &mut |_, _| true).is_err());
}

#[test]
fn inserting_pages_is_one_undo_step_and_keeps_what_the_document_had() {
    let dir = out_dir("merge-insert");
    let (a, b) = sources(&dir);
    let (session, info) = Session::open(&a, None).unwrap();
    assert_eq!(info.pages.len(), 4);
    let op = Operation::InsertPages {
        source: InsertSource {
            path: b.clone(),
            password: None,
            pages: Vec::new(),
        },
        at: 2,
        options: InsertOptions::default(),
    };
    let change = session.apply(op).unwrap();
    assert_eq!(change.page_count, 7);
    assert_eq!(change.state.undo_name.as_deref(), Some("Insert pages"));
    let report = change.merge_report.unwrap();
    assert_eq!(report.renamed_destinations, 1);
    assert_eq!(report.renamed_fields, 1);
    // Beta has no labels, so the inserted pages continue Alpha's roman numbering.
    let labels_now = change.labels.unwrap().unwrap().labels;
    assert_eq!(labels_now, ["i", "ii", "iii", "iv", "v", "1", "2"]);

    let undone = session.undo().unwrap();
    assert_eq!(undone.page_count, 4);
    assert_eq!(
        undone.labels.unwrap().unwrap().labels,
        ["i", "ii", "1", "2"]
    );
    let redone = session.redo().unwrap();
    assert_eq!(redone.page_count, 7);

    let path = dir.join("inserted.pdf");
    session
        .save(SaveKind::Incremental, Some(path.clone()))
        .unwrap();
    session.close();
    qpdf_check(&path);
    let out = open(&path);
    let texts: Vec<String> = (0..7).map(|p| page_text(&out, p)).collect();
    for (i, text) in [
        "Alpha 1", "Alpha 2", "Beta 1", "Beta 2", "Beta 3", "Alpha 3", "Alpha 4",
    ]
    .iter()
    .enumerate()
    {
        assert!(texts[i].contains(text), "page {i}: {}", texts[i]);
    }
    // Alpha's links still lead to Alpha's pages, now further on.
    let links = annots(&out, 0);
    assert_eq!(link_page(&out, &links[0]), Some(5));
    assert_eq!(link_page(&out, &links[2]), Some(6));
    // Beta's link uses its renamed destination.
    let beta_link = annots(&out, 2)
        .into_iter()
        .find(|a| name(a, "Subtype").as_deref() == Some(b"Link"))
        .unwrap();
    assert_eq!(
        beta_link
            .get_dict("Dest")
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap(),
        b"inserted_chap2"
    );
    assert_eq!(link_page(&out, &beta_link), Some(3));
    assert_eq!(field_names(&out), ["Name", "inserted_Name", "Shared"]);
    // Beta's bookmark goes between Chapter 2 (page 2) and End (page 4, now page 7).
    let outline = read_bookmarks(&out).unwrap();
    assert_eq!(
        titles(&outline.items),
        [
            ("Chapter 2".to_string(), Some(1), 0),
            ("beta".to_string(), Some(2), 0),
            ("End".to_string(), Some(6), 0)
        ]
    );
}

#[test]
fn inserted_pages_keep_their_own_labels_when_they_have_some() {
    let dir = out_dir("merge-insert-labels");
    let (a, b) = sources(&dir);
    let (session, _) = Session::open(&b, None).unwrap();
    let insert = |pages: Vec<usize>, at, labels| Operation::InsertPages {
        source: InsertSource {
            path: a.clone(),
            password: None,
            pages,
        },
        at,
        options: InsertOptions {
            bookmarks: BookmarkMode::Drop,
            labels,
        },
    };
    let change = session
        .apply(insert(Vec::new(), 1, InsertLabels::KeepSource))
        .unwrap();
    assert_eq!(
        change.labels.unwrap().unwrap().labels,
        ["1", "i", "ii", "1", "2", "2", "3"]
    );
    session.undo().unwrap();
    let change = session
        .apply(insert(vec![3], 3, InsertLabels::FollowDocument))
        .unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Insert page"));
    assert_eq!(change.page_count, 4);
    // Beta has no labels, and following the document adds none.
    assert!(change.labels.is_none());
    session.close();
}
