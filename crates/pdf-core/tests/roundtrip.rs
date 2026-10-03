//! Round-trip tests: write with pdf-core, save, reopen with MuPDF, check every key the
//! write profile requires (AGENTS.md section 9).

mod common;

use common::{open, out_dir, sample_file, save_and_reopen};
use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf::text_page::TextPageFlags;
use pdf_core::annot::quads::{Quad, is_acrobat_order, quads_from_array};
use pdf_core::annot::{MarkupKind, MarkupSpec, Rgb, add_text_markup};
use pdf_core::geometry::{PageGeometry, Point, Rect, read_page_boxes};
use pdf_core::labels::{self, LabelRule, LabelStyle};
use pdf_core::merge::{MergeOptions, MergeSource, merge_all};
use pdf_core::objects;
use pdf_core::outline::{self, OutlineItem};
use pdf_core::testgen::{MARKER, SampleSpec};

fn spec(rotate: i32, crop: Option<Rect>, unit: Option<f64>) -> SampleSpec {
    SampleSpec {
        pages: 2,
        rotate,
        crop_box: crop,
        user_unit: unit,
        ..SampleSpec::default()
    }
}

fn geometry_cases() -> Vec<(String, SampleSpec)> {
    let mut cases = Vec::new();
    for rotate in [0, 90, 180, 270] {
        for (crop_name, crop) in [
            ("full", None),
            ("crop", Some(Rect::new(100.0, 150.0, 500.0, 700.0))),
        ] {
            cases.push((format!("r{rotate}-{crop_name}"), spec(rotate, crop, None)));
        }
    }
    cases.push(("userunit".into(), spec(0, None, Some(2.0))));
    cases.push(("userunit-r90".into(), spec(90, None, Some(2.0))));
    cases
}

#[test]
fn geometry_matches_mupdf_page_transform() {
    let dir = out_dir("geometry");
    for (name, spec) in geometry_cases() {
        let path = sample_file(&dir, &format!("{name}.pdf"), spec);
        let doc = open(&path);
        let page = doc.load_pdf_page(0).unwrap();
        let ours = PageGeometry::new(&read_page_boxes(&page.object()).unwrap());
        let m = page.ctm().unwrap();
        for p in [
            Point::new(0.0, 0.0),
            Point::new(123.5, 456.25),
            Point::new(612.0, 792.0),
        ] {
            let theirs = Point::new(
                f64::from(m.a) * p.x + f64::from(m.c) * p.y + f64::from(m.e),
                f64::from(m.b) * p.x + f64::from(m.d) * p.y + f64::from(m.f),
            );
            let mine = ours.user_to_view(p);
            assert!(
                (mine.x - theirs.x).abs() < 0.01 && (mine.y - theirs.y).abs() < 0.01,
                "{name}: user {p:?} -> ours {mine:?}, MuPDF {theirs:?}"
            );
        }
        let bounds = page.bounds().unwrap();
        assert!(
            (ours.width - f64::from(bounds.x1 - bounds.x0)).abs() < 0.01,
            "{name}: width"
        );
        assert!(
            (ours.height - f64::from(bounds.y1 - bounds.y0)).abs() < 0.01,
            "{name}: height"
        );
    }
}

#[test]
fn labels_round_trip_and_match_mupdf() {
    let dir = out_dir("labels");
    let src = sample_file(
        &dir,
        "src.pdf",
        SampleSpec {
            pages: 30,
            ..SampleSpec::default()
        },
    );
    let rules = vec![
        LabelRule {
            start_page: 0,
            style: LabelStyle::LowerRoman,
            prefix: String::new(),
            first_number: 1,
        },
        LabelRule::decimal_from_one(4),
        LabelRule {
            start_page: 20,
            style: LabelStyle::UpperLetters,
            prefix: "App-".into(),
            first_number: 25,
        },
        LabelRule {
            start_page: 28,
            style: LabelStyle::None,
            prefix: "Índice".into(),
            first_number: 1,
        },
    ];
    let mut doc = open(&src);
    labels::write_rules(&mut doc, rules.clone()).unwrap();
    let doc = save_and_reopen(&doc, &src, &dir.join("labels.pdf"));

    assert_eq!(labels::read_rules(&doc).unwrap(), rules);
    for page in 0..30 {
        assert_eq!(
            doc.page_label(page).unwrap(),
            labels::label_for_page(&rules, page),
            "page {page}"
        );
    }
    assert_eq!(labels::label_for_page(&rules, 21), "App-Z");
    assert_eq!(labels::label_for_page(&rules, 22), "App-AA");
}

#[test]
fn untouched_labels_survive_an_unrelated_edit() {
    let dir = out_dir("labels-untouched");
    let src = sample_file(
        &dir,
        "src.pdf",
        SampleSpec {
            pages: 6,
            ..SampleSpec::default()
        },
    );
    let mut doc = open(&src);
    labels::write_rules(
        &mut doc,
        vec![LabelRule {
            start_page: 0,
            style: LabelStyle::UpperRoman,
            prefix: "P".into(),
            first_number: 3,
        }],
    )
    .unwrap();
    let doc = save_and_reopen(&doc, &src, &dir.join("labelled.pdf"));
    let before = labels::read_rules(&doc).unwrap();

    let mut doc = doc;
    outline::write_outline(&mut doc, &[OutlineItem::new("Only", 0)]).unwrap();
    let after = save_and_reopen(&doc, &dir.join("labelled.pdf"), &dir.join("edited.pdf"));
    assert_eq!(labels::read_rules(&after).unwrap(), before);
}

#[test]
fn outline_round_trip_with_unicode_and_open_state() {
    let dir = out_dir("outline");
    let src = sample_file(
        &dir,
        "src.pdf",
        SampleSpec {
            pages: 10,
            ..SampleSpec::default()
        },
    );
    let tree = vec![
        OutlineItem {
            open: true,
            children: vec![OutlineItem {
                children: vec![OutlineItem::new("Deep", 3)],
                ..OutlineItem::new("Préface — 日本", 1)
            }],
            ..OutlineItem::new("Part I", 0)
        },
        OutlineItem::new("Plain ASCII", 9),
    ];
    let mut doc = open(&src);
    outline::write_outline(&mut doc, &tree).unwrap();
    let doc = save_and_reopen(&doc, &src, &dir.join("outline.pdf"));

    let read = outline::read_outline(&doc).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].title, "Part I");
    assert!(read[0].open);
    assert_eq!(read[0].page, Some(0));
    assert_eq!(read[0].children[0].title, "Préface — 日本");
    assert!(!read[0].children[0].open);
    assert_eq!(read[0].children[0].children[0].page, Some(3));
    assert_eq!(read[1].page, Some(9));

    // Non-ASCII titles are UTF-16BE with a BOM; ASCII stays a plain string.
    let first = doc
        .catalog()
        .unwrap()
        .get_dict("Outlines")
        .unwrap()
        .unwrap()
        .get_dict("First")
        .unwrap()
        .unwrap();
    let child = first.get_dict("First").unwrap().unwrap();
    let raw = child
        .get_dict("Title")
        .unwrap()
        .unwrap()
        .as_bytes()
        .unwrap();
    assert_eq!(&raw[..2], &[0xFE, 0xFF]);
    let ascii = first
        .get_dict("Title")
        .unwrap()
        .unwrap()
        .as_bytes()
        .unwrap();
    assert_eq!(ascii, b"Part I");

    // /Count: open "Part I" shows its one child; closed "Préface" hides one.
    assert_eq!(
        first.get_dict("Count").unwrap().unwrap().as_int().unwrap(),
        1
    );
    assert_eq!(
        child.get_dict("Count").unwrap().unwrap().as_int().unwrap(),
        -1
    );
    // Explicit destination: [page /XYZ null null null].
    let dest = outline::dest_numbers(&first).unwrap().unwrap();
    assert_eq!(dest, vec![None, None, None]);
}

fn find_marker_quads(doc: &PdfDocument, page: i32) -> Vec<Quad> {
    let page = doc.load_page(page).unwrap();
    let text = page.to_text_page(TextPageFlags::empty()).unwrap();
    text.search("quick brown fox")
        .unwrap()
        .iter()
        .map(Quad::from)
        .collect()
}

fn get(obj: &PdfObject, key: &str) -> PdfObject {
    obj.get_dict(key)
        .unwrap()
        .unwrap_or_else(|| panic!("missing /{key}"))
}

#[test]
fn highlight_has_every_profile_key() {
    let dir = out_dir("highlight");
    for (name, spec) in geometry_cases() {
        let src = sample_file(&dir, &format!("{name}-src.pdf"), spec);
        let mut doc = open(&src);
        let quads = find_marker_quads(&doc, 0);
        assert!(!quads.is_empty(), "{name}: marker text not found");
        let created = add_text_markup(
            &mut doc,
            &MarkupSpec {
                kind: MarkupKind::Highlight,
                page: 0,
                quads: quads.clone(),
                color: Rgb::YELLOW,
                opacity: 0.5,
                author: "Tëster".into(),
                note: Some("A note".into()),
            },
        )
        .unwrap();
        let doc = save_and_reopen(&doc, &src, &dir.join(format!("{name}.pdf")));

        let page = doc.load_pdf_page(0).unwrap();
        let annot = page
            .annotations()
            .next()
            .unwrap_or_else(|| panic!("{name}: no annotation"));
        let obj = annot.object();
        let geometry = PageGeometry::new(&read_page_boxes(&page.object()).unwrap());

        assert_eq!(get(&obj, "Subtype").as_name().unwrap(), b"Highlight");
        assert_eq!(get(&obj, "NM").as_string().unwrap(), created.name);
        assert_eq!(get(&obj, "T").as_string().unwrap(), "Tëster");
        assert_eq!(&get(&obj, "T").as_bytes().unwrap()[..2], &[0xFE, 0xFF]);
        for key in ["CreationDate", "M"] {
            let date = get(&obj, key).as_string().unwrap();
            assert!(
                date.starts_with("D:") && date.ends_with("+00'00'"),
                "{name}: /{key} {date}"
            );
        }
        assert_eq!(get(&obj, "F").as_int().unwrap(), 4);
        assert_eq!(objects::numbers(&get(&obj, "C")).unwrap().unwrap().len(), 3);
        assert!((get(&obj, "CA").as_float().unwrap() - 0.5).abs() < 1e-6);
        assert!(get(&obj, "P").is_dict().unwrap());
        assert_eq!(get(&obj, "Contents").as_string().unwrap(), "A note");

        // Popup linked both ways.
        let popup = get(&obj, "Popup");
        assert_eq!(get(&popup, "Subtype").as_name().unwrap(), b"Popup");
        assert_eq!(
            get(&popup, "Parent").as_indirect().unwrap(),
            obj.as_indirect().unwrap()
        );

        // QuadPoints in Acrobat order and matching the selected text.
        let qp = objects::numbers(&get(&obj, "QuadPoints")).unwrap().unwrap();
        let written = quads_from_array(&qp).unwrap();
        assert_eq!(written.len(), quads.len());
        for (w, q) in written.iter().zip(&quads) {
            assert!(is_acrobat_order(w), "{name}: {w:?}");
            let expected = q.view_to_user(&geometry);
            for (a, b) in [
                (w.ul, expected.ul),
                (w.ur, expected.ur),
                (w.ll, expected.ll),
                (w.lr, expected.lr),
            ] {
                assert!(
                    (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01,
                    "{name}: {a:?} vs {b:?}"
                );
            }
        }

        // /Rect contains all quads plus the 1 pt margin.
        let rect = objects::rect(&get(&obj, "Rect")).unwrap().unwrap();
        let content = written
            .iter()
            .map(Quad::bounds)
            .reduce(|a, b| a.union(&b))
            .unwrap()
            .expand(1.0);
        assert!(
            rect.x0 <= content.x0 + 1e-3
                && rect.y0 <= content.y0 + 1e-3
                && rect.x1 >= content.x1 - 1e-3
                && rect.y1 >= content.y1 - 1e-3,
            "{name}: Rect {rect:?} does not contain {content:?}"
        );

        // Appearance stream with Multiply blend and the opacity.
        let normal = get(&get(&obj, "AP"), "N");
        assert!(
            normal.is_stream().unwrap(),
            "{name}: /AP /N is not a stream"
        );
        let bbox = objects::rect(&get(&normal, "BBox")).unwrap().unwrap();
        assert!(
            (bbox.x0 - rect.x0).abs() < 1e-3 && (bbox.y1 - rect.y1).abs() < 1e-3,
            "{name}: BBox != Rect"
        );
        let gs = get(&get(&normal, "Resources"), "ExtGState");
        let mut found_multiply = false;
        for entry in gs.dict_iter().unwrap() {
            let (_, state) = entry.unwrap();
            if let Some(bm) = state.get_dict("BM").unwrap() {
                found_multiply |= bm.as_name().unwrap() == b"Multiply";
                assert!((get(&state, "CA").as_float().unwrap() - 0.5).abs() < 1e-6);
            }
        }
        assert!(found_multiply, "{name}: no Multiply blend in appearance");
    }
}

#[test]
fn merge_keeps_pages_outlines_and_labels() {
    let dir = out_dir("merge");
    let a = sample_file(
        &dir,
        "a.pdf",
        SampleSpec {
            pages: 3,
            title: "Alpha".into(),
            ..SampleSpec::default()
        },
    );
    let b = sample_file(
        &dir,
        "b.pdf",
        SampleSpec {
            pages: 4,
            title: "Beta".into(),
            ..SampleSpec::default()
        },
    );
    let mut doc_b = open(&b);
    labels::write_rules(
        &mut doc_b,
        vec![LabelRule {
            start_page: 0,
            style: LabelStyle::LowerRoman,
            prefix: String::new(),
            first_number: 1,
        }],
    )
    .unwrap();
    outline::write_outline(&mut doc_b, &[OutlineItem::new("B chapter", 2)]).unwrap();
    let doc_b = save_and_reopen(&doc_b, &b, &dir.join("b2.pdf"));

    let (merged, _) = merge_all(
        &[
            MergeSource {
                doc: open(&a),
                name: "Alpha".into(),
            },
            MergeSource {
                doc: doc_b,
                name: "Beta".into(),
            },
        ],
        MergeOptions::default(),
    )
    .unwrap();
    let path = dir.join("merged.pdf");
    pdf_core::save::save_atomic(&merged, pdf_core::save::SaveKind::Optimized, None, &path).unwrap();
    let merged = open(&path);

    assert_eq!(merged.page_count().unwrap(), 7);
    let labels: Vec<String> = (0..7).map(|p| merged.page_label(p).unwrap()).collect();
    assert_eq!(labels, ["1", "2", "3", "i", "ii", "iii", "iv"]);
    let tree = outline::read_outline(&merged).unwrap();
    assert_eq!(
        tree.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        ["Alpha", "Beta"]
    );
    assert_eq!(tree[1].page, Some(3));
    assert_eq!(tree[1].children[0].title, "B chapter");
    assert_eq!(tree[1].children[0].page, Some(5));
    // Text survived on a copied page.
    let text = merged
        .load_page(5)
        .unwrap()
        .to_text_page(TextPageFlags::empty())
        .unwrap();
    assert!(!text.search(MARKER).unwrap().is_empty());
}
