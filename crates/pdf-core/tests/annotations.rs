//! Annotations (AGENTS.md sections 5 and 6.5): every type created, saved and reopened with
//! MuPDF, with every key the write profile requires checked; edits, deletes and repair,
//! also on annotations "another app" wrote; undo and redo through the session.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{assemble, open, out_dir, qpdf_check, sample_file, save_and_reopen, writing};
use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf::text_page::TextPageFlags;
use pdf_core::annot::quads::{Quad, is_acrobat_order, quads_from_array};
use pdf_core::annot::read::{Problem, read_page};
use pdf_core::annot::repair::{repair, scan};
use pdf_core::annot::{
    self, AnnotationEdit, Body, INK_TOLERANCE, Kind, MarkupKind, NewAnnotation, Rgb,
};
use pdf_core::geometry::{PageGeometry, Point, Rect, read_page_boxes};
use pdf_core::objects;
use pdf_core::ops::Operation;
use pdf_core::session::Session;
use pdf_core::testgen::SampleSpec;

const BLUE: Rgb = Rgb {
    r: 0.1,
    g: 0.3,
    b: 0.9,
};

fn geometry_cases() -> Vec<(&'static str, SampleSpec)> {
    let spec = |rotate, crop: Option<Rect>, unit| SampleSpec {
        pages: 2,
        rotate,
        crop_box: crop,
        user_unit: unit,
        ..SampleSpec::default()
    };
    let crop = Some(Rect::new(100.0, 150.0, 500.0, 700.0));
    vec![
        ("normal", spec(0, None, None)),
        ("r90", spec(90, None, None)),
        ("r180", spec(180, None, None)),
        ("r270", spec(270, None, None)),
        ("crop", spec(0, crop, None)),
        ("crop-r90", spec(90, crop, None)),
        ("userunit", spec(0, None, Some(2.0))),
    ]
}

fn get(obj: &PdfObject, key: &str) -> PdfObject {
    obj.get_dict(key)
        .unwrap()
        .unwrap_or_else(|| panic!("missing /{key}"))
}

fn rect_of(obj: &PdfObject) -> Rect {
    objects::rect(&get(obj, "Rect")).unwrap().unwrap()
}

fn contains(outer: Rect, inner: Rect) -> bool {
    const EPS: f64 = 1e-3;
    outer.x0 <= inner.x0 + EPS
        && outer.y0 <= inner.y0 + EPS
        && outer.x1 >= inner.x1 - EPS
        && outer.y1 >= inner.y1 - EPS
}

fn marker_quads(doc: &PdfDocument, page: i32) -> Vec<Quad> {
    let page = doc.load_page(page).unwrap();
    let text = page.to_text_page(TextPageFlags::empty()).unwrap();
    text.search("quick brown fox")
        .unwrap()
        .iter()
        .map(Quad::from)
        .collect()
}

fn wave() -> Vec<Point> {
    (0..=200)
        .map(|i| {
            let x = f64::from(i);
            Point::new(100.0 + x, 260.0 + 20.0 * (x / 15.0).sin())
        })
        .collect()
}

/// One of each type Lectrix creates, on `page` of `doc`.
fn every_kind(doc: &PdfDocument, page: usize) -> Vec<NewAnnotation> {
    let quads = marker_quads(doc, i32::try_from(page).unwrap());
    assert!(!quads.is_empty(), "marker text not found");
    let new = |body| NewAnnotation {
        page,
        body,
        color: BLUE,
        opacity: 0.6,
        author: "Tëster".into(),
    };
    let mut out: Vec<NewAnnotation> = [
        MarkupKind::Highlight,
        MarkupKind::Underline,
        MarkupKind::StrikeOut,
        MarkupKind::Squiggly,
    ]
    .into_iter()
    .map(|kind| {
        new(Body::Markup {
            kind,
            quads: quads.clone(),
            note: Some("A note — ünïcödé".into()),
        })
    })
    .collect();
    out.push(new(Body::Note {
        at: Point::new(150.0, 160.0),
        text: "Sticky note".into(),
    }));
    out.push(new(Body::Ink {
        strokes: vec![wave(), vec![Point::new(320.0, 300.0)]],
        width: 2.0,
    }));
    out.push(new(Body::FreeText {
        rect: Rect::new(72.0, 330.0, 272.0, 340.0),
        text: "Text box from Lectrix\nwith a second line that is long enough to wrap".into(),
        font_size: 14.0,
    }));
    out
}

/// Rule 6, for every type.
fn check_metadata(obj: &PdfObject, name: &str) {
    assert!(
        get(obj, "NM").as_string().unwrap().len() == 36,
        "{name}: /NM"
    );
    assert_eq!(get(obj, "T").as_string().unwrap(), "Tëster", "{name}: /T");
    for key in ["CreationDate", "M"] {
        let date = get(obj, key).as_string().unwrap();
        assert!(
            date.starts_with("D:") && date.ends_with("+00'00'"),
            "{name}: /{key} {date}"
        );
    }
    assert_eq!(get(obj, "F").as_int().unwrap(), 4, "{name}: /F");
    assert!(get(obj, "P").is_dict().unwrap(), "{name}: /P");
}

fn check_popup(obj: &PdfObject, name: &str) {
    let popup = get(obj, "Popup");
    assert_eq!(
        get(&popup, "Subtype").as_name().unwrap(),
        b"Popup",
        "{name}"
    );
    assert_eq!(
        get(&popup, "Parent").as_indirect().unwrap(),
        obj.as_indirect().unwrap(),
        "{name}: popup /Parent"
    );
    assert!(get(&popup, "P").is_dict().unwrap(), "{name}: popup /P");
}

fn normal_appearance(obj: &PdfObject, name: &str) -> PdfObject {
    let normal = get(&get(obj, "AP"), "N");
    assert!(
        normal.is_stream().unwrap(),
        "{name}: /AP /N is not a stream"
    );
    normal
}

#[test]
fn every_type_has_every_profile_key_on_every_page_geometry() {
    let dir = out_dir("annotations-profile");
    for (case, spec) in geometry_cases() {
        let rotation = spec.rotate;
        let src = sample_file(&dir, &format!("{case}-src.pdf"), spec);
        let mut doc = open(&src);
        let specs = every_kind(&doc, 0);
        let mut ids = Vec::new();
        for s in &specs {
            ids.push(annot::create(&mut doc, s).unwrap());
        }
        let out = dir.join(format!("{case}.pdf"));
        let doc = save_and_reopen(&doc, &src, &out);
        qpdf_check(&out);

        let page_obj = doc.find_page(0).unwrap();
        let geometry = PageGeometry::new(&read_page_boxes(&page_obj).unwrap());
        for (s, created) in specs.iter().zip(&ids) {
            let kind = s.body.kind();
            let name = format!("{case} {}", kind.label());
            let obj = doc.new_indirect(created.xref, 0).unwrap();
            assert_eq!(
                get(&obj, "Subtype").as_name().unwrap(),
                kind.subtype().as_bytes(),
                "{name}"
            );
            assert_eq!(get(&obj, "NM").as_string().unwrap(), created.name);
            check_metadata(&obj, &name);
            let normal = normal_appearance(&obj, &name);
            let rect = rect_of(&obj);
            let opacity = get(&obj, "CA").as_float().unwrap();
            assert!((opacity - 0.6).abs() < 1e-6, "{name}: /CA {opacity}");

            match &s.body {
                Body::Markup { kind, quads, .. } => {
                    check_popup(&obj, &name);
                    assert_eq!(
                        get(&obj, "Contents").as_string().unwrap(),
                        "A note — ünïcödé"
                    );
                    assert_eq!(objects::numbers(&get(&obj, "C")).unwrap().unwrap().len(), 3);
                    // Rule 3.
                    let qp = objects::numbers(&get(&obj, "QuadPoints")).unwrap().unwrap();
                    let written = quads_from_array(&qp).unwrap();
                    assert_eq!(written.len(), quads.len(), "{name}");
                    for (w, q) in written.iter().zip(quads) {
                        assert!(is_acrobat_order(w), "{name}: {w:?}");
                        let e = q.view_to_user(&geometry);
                        for (a, b) in [(w.ul, e.ul), (w.ur, e.ur), (w.ll, e.ll), (w.lr, e.lr)] {
                            assert!((a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01);
                        }
                    }
                    // Rule 4.
                    let content = written
                        .iter()
                        .map(Quad::bounds)
                        .reduce(|a, b| a.union(&b))
                        .unwrap()
                        .expand(1.0);
                    assert!(contains(rect, content), "{name}: {rect:?} vs {content:?}");
                    // Rule 5: Multiply blend for highlights, opacity in the appearance.
                    let gs = get(&get(&normal, "Resources"), "ExtGState");
                    let mut multiply = false;
                    for entry in gs.dict_iter().unwrap() {
                        let (_, state) = entry.unwrap();
                        assert!((get(&state, "CA").as_float().unwrap() - 0.6).abs() < 1e-6);
                        if let Some(bm) = state.get_dict("BM").unwrap() {
                            multiply |= bm.as_name().unwrap() == b"Multiply";
                        }
                    }
                    assert_eq!(multiply, *kind == MarkupKind::Highlight, "{name}: blend");
                }
                Body::TextMarkup { .. } => unreachable!("every_kind makes no text ranges"),
                Body::Note { at, .. } => {
                    check_popup(&obj, &name);
                    assert_eq!(get(&obj, "Contents").as_string().unwrap(), "Sticky note");
                    assert_eq!(get(&obj, "Name").as_name().unwrap(), b"Comment");
                    // Acrobat and MuPDF show a note upright, from the upper-left corner
                    // of /Rect in user space (NoRotate): that corner is under `at`.
                    let corner = geometry.user_to_view(Point::new(rect.x0, rect.y1));
                    assert!(
                        (corner.x - at.x).abs() < 0.01 && (corner.y - at.y).abs() < 0.01,
                        "{name}: {corner:?}"
                    );
                    // And at the size of the appearance box (NoZoom), which is the size of
                    // /Rect, as PDFium and pdf.js draw it; /Matrix is the identity.
                    let bbox = objects::rect(&get(&normal, "BBox")).unwrap().unwrap();
                    assert!(
                        (bbox.width() - rect.width()).abs() < 0.01,
                        "{name}: {bbox:?}"
                    );
                    assert!((bbox.height() - rect.height()).abs() < 0.01, "{name}");
                    let identity = match normal.get_dict("Matrix").unwrap() {
                        Some(m) => objects::numbers(&m).unwrap().unwrap(),
                        None => vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    };
                    assert_eq!(identity, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], "{name}");
                }
                Body::Ink { strokes, width } => {
                    let bs = get(&get(&obj, "BS"), "W").as_float().unwrap();
                    assert!((f64::from(bs) - width).abs() < 1e-6);
                    // Rule 9: simplified, and every input point is within the tolerance
                    // of the written stroke (compared in view space, where the tolerance is
                    // measured).
                    let list = objects::array_items(&get(&obj, "InkList")).unwrap();
                    assert_eq!(list.len(), 2, "{name}");
                    let written: Vec<Point> = objects::numbers(&list[0])
                        .unwrap()
                        .unwrap()
                        .chunks(2)
                        .map(|c| geometry.user_to_view(Point::new(c[0], c[1])))
                        .collect();
                    assert!(
                        written.len() < strokes[0].len() / 3,
                        "{name}: {}",
                        written.len()
                    );
                    for p in &strokes[0] {
                        let d = written
                            .windows(2)
                            .map(|w| segment_distance(*p, w[0], w[1]))
                            .fold(f64::INFINITY, f64::min);
                        assert!(d <= INK_TOLERANCE + 1e-3, "{name}: {p:?} {d}");
                    }
                    let points: Vec<Point> = strokes
                        .iter()
                        .flatten()
                        .map(|p| geometry.view_to_user(*p))
                        .collect();
                    let content = Rect::bounding(points).expand(width + 1.0);
                    assert!(contains(rect, content), "{name}: {rect:?} vs {content:?}");
                }
                Body::FreeText {
                    rect: asked, text, ..
                } => {
                    // Rule 7.
                    let da = get(&obj, "DA").as_string().unwrap();
                    assert!(da.contains("/Helv 14 Tf"), "{name}: /DA {da}");
                    assert!(da.contains("rg"), "{name}: /DA {da}");
                    assert!(obj.get_dict("RC").unwrap().is_none(), "{name}: /RC");
                    assert!(obj.get_dict("CL").unwrap().is_none(), "{name}: /CL");

                    assert_eq!(get(&obj, "Contents").as_string().unwrap(), *text);
                    assert!(objects::array_items(&get(&obj, "C")).unwrap().is_empty());
                    let font = get(&get(&get(&normal, "Resources"), "Font"), "Helv");
                    assert_eq!(get(&font, "BaseFont").as_name().unwrap(), b"Helvetica");
                    // Rule 4: the box grew to three lines of text, plus the margin.
                    let rd = objects::numbers(&get(&obj, "RD")).unwrap().unwrap();
                    assert_eq!(rd, [1.0, 1.0, 1.0, 1.0], "{name}");
                    let inner = geometry.user_rect_to_view(rect.expand(-1.0));
                    assert!(
                        (inner.x0 - asked.x0).abs() < 0.01 && (inner.y0 - asked.y0).abs() < 0.01
                    );
                    assert!((inner.width() - asked.width()).abs() < 0.01);
                    assert!(
                        inner.height() >= 3.0 * 1.2 * 14.0 - 0.01,
                        "{name}: {inner:?}"
                    );
                    let rotate = obj.get_dict("Rotate").unwrap().map(|r| r.as_int().unwrap());
                    assert_eq!(rotate.unwrap_or(0), rotation, "{name}: /Rotate");
                }
            }
        }

        // Read back as the annotation list shows them: nothing needs repair.
        let listed = read_page(&doc, 0).unwrap();
        assert_eq!(listed.len(), specs.len());
        for a in &listed {
            assert!(
                a.problems.is_empty(),
                "{case}: {:?} {:?}",
                a.kind,
                a.problems
            );
            assert_eq!(a.author, "Tëster");
        }
    }
}

fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

/// A one-page file with annotations "another app" wrote, by hand:
/// 4: a Square with an appearance, rich text and a private key;
/// 5: its appearance; 6: a highlight with quads in the spec's counter-clockwise order, no
/// appearance, no metadata, and a /Rect smaller than its quads; 7: an ink drawing whose
/// appearance BBox is not its Rect and whose Rect is too small; 8: a sticky note without
/// an appearance, /NM, /M or /P; 9: a text box without an appearance; 10: an underline
/// whose QuadPoints can't be read; 11: a reply to the Square; 12: the drawing's appearance.
fn other_app_file(dir: &Path) -> PathBuf {
    let ap = "q 1 0 0 RG 2 w 1 1 98 98 re S Q";
    let ink_ap = "q 0 0 1 RG 1 w 0 0 m 50 50 l S Q";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> \
         /Annots [4 0 R 6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R] >>"
            .to_owned(),
        "<< /Type /Annot /Subtype /Square /Rect [100 600 200 700] /C [1 0 0] \
         /T (Other App) /Contents (Theirs) /RC (<body><p>Theirs</p></body>) /NM (other-1) \
         /F 4 /M (D:20200101000000Z) /P 3 0 R /OtherAppPrivate (keep me) \
         /AP << /N 5 0 R >> >>"
            .to_owned(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Length {} >>\nstream\n{ap}\nendstream",
            ap.len()
        ),
        "<< /Type /Annot /Subtype /Highlight /Rect [72 682 150 690] /C [1 1 0] \
         /T (Other App) /Contents (Old highlight) \
         /QuadPoints [72 680 200 680 200 692 72 692] >>"
            .to_owned(),
        "<< /Type /Annot /Subtype /Ink /Rect [300 300 350 350] /C [0 0 1] /NM (ink-1) /F 4 \
         /M (D:20200101000000Z) /P 3 0 R /T (Other App) /BS << /W 2 >> \
         /InkList [[300 300 380 380]] /AP << /N 12 0 R >> >>"
            .to_owned(),
        "<< /Type /Annot /Subtype /Text /Rect [400 700 420 720] /C [1 1 0] /F 4 \
         /T (Other App) /Contents (A sticky note) >>"
            .to_owned(),
        "<< /Type /Annot /Subtype /FreeText /Rect [72 500 272 540] /F 4 /NM (ft-1) \
         /M (D:20200101000000Z) /P 3 0 R /T (Other App) /DA (/Helv 12 Tf 0 0 1 rg) \
         /Contents (Typed by another app) >>"
            .to_owned(),
        "<< /Type /Annot /Subtype /Underline /Rect [72 400 200 420] /C [0 1 0] /F 4 \
         /NM (u-1) /M (D:20200101000000Z) /P 3 0 R /QuadPoints [72 400 200 400 200] >>"
            .to_owned(),
        "<< /Type /Annot /Subtype /Text /Rect [210 690 230 710] /F 4 /NM (reply-1) \
         /M (D:20200101000000Z) /P 3 0 R /T (Someone) /Contents (A reply) /IRT 4 0 R \
         /AP << /N 5 0 R >> >>"
            .to_owned(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 50 50] /Length {} >>\nstream\n{ink_ap}\nendstream",
            ink_ap.len()
        ),
    ];
    let path = dir.join("other-app.pdf");
    std::fs::write(&path, assemble(&objects)).unwrap();
    path
}

fn keys(obj: &PdfObject) -> BTreeSet<String> {
    objects::dict_entries(obj)
        .unwrap()
        .into_iter()
        .map(|(k, _)| String::from_utf8_lossy(&k.as_name().unwrap()).into_owned())
        .collect()
}

fn raw(obj: &PdfObject, key: &str) -> Option<String> {
    obj.get_dict(key).unwrap().map(|v| v.to_string())
}

#[test]
fn editing_another_apps_annotation_changes_only_the_edited_keys() {
    let dir = out_dir("annotations-edit-other");
    let src = other_app_file(&dir);
    let mut doc = open(&src);
    let square = doc.new_indirect(4, 0).unwrap();
    let before: Vec<(String, Option<String>)> = keys(&square)
        .into_iter()
        .map(|k| (k.clone(), raw(&square, &k)))
        .collect();

    annot::edit(
        &mut doc,
        0,
        4,
        &AnnotationEdit {
            color: Some(BLUE),
            ..AnnotationEdit::default()
        },
    )
    .unwrap();
    let square = doc.new_indirect(4, 0).unwrap();
    // Changed: the colour, the modification date, the appearance. MuPDF's synthesis may
    // also set the geometry keys it draws from (/Rect, /RD, /BS).
    let allowed: BTreeSet<&str> = ["C", "M", "AP", "Rect", "RD", "BS"].into();
    for (key, value) in &before {
        if allowed.contains(key.as_str()) {
            continue;
        }
        assert_eq!(&raw(&square, key), value, "/{key} changed");
    }
    for key in keys(&square) {
        assert!(
            before.iter().any(|(k, _)| *k == key) || allowed.contains(key.as_str()),
            "/{key} added"
        );
    }
    assert_eq!(
        get(&square, "OtherAppPrivate").as_string().unwrap(),
        "keep me"
    );
    assert_ne!(raw(&square, "M").unwrap(), "(D:20200101000000Z)");

    // Editing the note text replaces the rich text with it (or readers would still show
    // the old text); everything else stays.
    annot::edit(
        &mut doc,
        0,
        4,
        &AnnotationEdit {
            contents: Some("Mine now".into()),
            ..AnnotationEdit::default()
        },
    )
    .unwrap();
    let square = doc.new_indirect(4, 0).unwrap();
    assert_eq!(get(&square, "Contents").as_string().unwrap(), "Mine now");
    assert!(square.get_dict("RC").unwrap().is_none());
    assert_eq!(get(&square, "T").as_string().unwrap(), "Other App");
    assert_eq!(get(&square, "NM").as_string().unwrap(), "other-1");
    check_popup(&square, "edited square");

    // Annotations nobody touched keep every byte: an incremental save rewrites only the
    // square, its new popup, its appearance and the page (whose /Annots gained the popup).
    let out = dir.join("edited.pdf");
    writing(|| {
        pdf_core::save::save_atomic(
            &doc,
            pdf_core::save::SaveKind::Incremental,
            Some(&src),
            &out,
        )
    })
    .unwrap();
    let original = std::fs::read(&src).unwrap();
    let saved = std::fs::read(&out).unwrap();
    assert_eq!(&saved[..original.len()], &original[..]);
    let written = common::objects_written_after(&saved, original.len());
    for untouched in [6, 7, 8, 9, 10, 11] {
        assert!(
            !written.contains(&untouched),
            "object {untouched} was rewritten: {written:?}"
        );
    }
    qpdf_check(&out);
}

#[test]
fn notes_move_drawings_resize_and_text_boxes_refit() {
    let dir = out_dir("annotations-geometry-edits");
    for (case, spec) in geometry_cases() {
        let src = sample_file(&dir, &format!("{case}-src.pdf"), spec);
        let mut doc = open(&src);
        let specs = every_kind(&doc, 0);
        let note = annot::create(&mut doc, &specs[4]).unwrap();
        let ink = annot::create(&mut doc, &specs[5]).unwrap();
        let text = annot::create(&mut doc, &specs[6]).unwrap();
        let id = |r: &annot::AnnotationRef| u32::try_from(r.xref).unwrap();
        let find = |doc: &PdfDocument, id: u32| {
            read_page(doc, 0)
                .unwrap()
                .into_iter()
                .find(|a| a.id == id)
                .unwrap()
        };

        // Move the note by (30, 40): the icon and its popup move, the size stays.
        let before = find(&doc, id(&note));
        let popup_before = rect_of(&get(&doc.new_indirect(note.xref, 0).unwrap(), "Popup"));
        let moved = Rect::new(
            before.bounds.x0 + 30.0,
            before.bounds.y0 + 40.0,
            before.bounds.x1 + 30.0,
            before.bounds.y1 + 40.0,
        );
        annot::edit(
            &mut doc,
            0,
            id(&note),
            &AnnotationEdit {
                bounds: Some(moved),
                ..AnnotationEdit::default()
            },
        )
        .unwrap();
        let after = find(&doc, id(&note));
        assert!(
            (after.bounds.x0 - moved.x0).abs() < 0.01,
            "{case}: {after:?}"
        );
        assert!((after.bounds.y0 - moved.y0).abs() < 0.01, "{case}");
        assert!((after.bounds.width() - before.bounds.width()).abs() < 0.01);
        let obj = doc.new_indirect(note.xref, 0).unwrap();
        let page_obj = doc.find_page(0).unwrap();
        let g = PageGeometry::new(&read_page_boxes(&page_obj).unwrap());
        let popup_after = g.user_rect_to_view(rect_of(&get(&obj, "Popup")));
        let popup_before = g.user_rect_to_view(popup_before);
        assert!(
            (popup_after.x0 - popup_before.x0 - 30.0).abs() < 0.01,
            "{case}: popup"
        );
        assert!(
            (popup_after.y0 - popup_before.y0 - 40.0).abs() < 0.01,
            "{case}: popup"
        );

        // Resize the drawing to a new box: its strokes fill the new box.
        let target = Rect::new(50.0, 400.0, 150.0, 450.0);
        annot::edit(
            &mut doc,
            0,
            id(&ink),
            &AnnotationEdit {
                bounds: Some(target),
                width: Some(4.0),
                ..AnnotationEdit::default()
            },
        )
        .unwrap();
        let after = find(&doc, id(&ink));
        for (a, b) in [
            (after.bounds.x0, target.x0),
            (after.bounds.y0, target.y0),
            (after.bounds.x1, target.x1),
            (after.bounds.y1, target.y1),
        ] {
            assert!(
                (a - b).abs() < 0.01,
                "{case}: {:?} vs {target:?}",
                after.bounds
            );
        }
        assert_eq!(after.width, Some(4.0));
        assert!(
            contains(after.rect, after.bounds.expand(5.0)),
            "{case}: {:?}",
            after.rect
        );
        assert!(after.problems.is_empty(), "{case}: {:?}", after.problems);

        // A shorter text shrinks the box to one line; a wider box keeps its new width.
        annot::edit(
            &mut doc,
            0,
            id(&text),
            &AnnotationEdit {
                contents: Some("Short".into()),
                color: Some(Rgb::BLACK),
                ..AnnotationEdit::default()
            },
        )
        .unwrap();
        let after = find(&doc, id(&text));
        assert_eq!(after.contents, "Short");
        assert_eq!(after.color, Some(Rgb::BLACK));
        assert!(
            after.bounds.height() < 2.0 * 1.2 * 14.0,
            "{case}: {:?}",
            after.bounds
        );
        let wide = Rect::new(60.0, 500.0, 400.0, 600.0);
        annot::edit(
            &mut doc,
            0,
            id(&text),
            &AnnotationEdit {
                bounds: Some(wide),
                font_size: Some(20.0),
                ..AnnotationEdit::default()
            },
        )
        .unwrap();
        let after = find(&doc, id(&text));
        for (a, b) in [
            (after.bounds.x0, wide.x0),
            (after.bounds.y0, wide.y0),
            (after.bounds.x1, wide.x1),
            (after.bounds.y1, wide.y1),
        ] {
            assert!((a - b).abs() < 0.01, "{case}: {:?}", after.bounds);
        }
        assert_eq!(after.font_size, Some(20.0));

        let out = dir.join(format!("{case}.pdf"));
        let doc = save_and_reopen(&doc, &src, &out);
        qpdf_check(&out);
        for a in read_page(&doc, 0).unwrap() {
            assert!(
                a.problems.is_empty(),
                "{case}: {:?} {:?}",
                a.kind,
                a.problems
            );
        }
    }
}

#[test]
fn edits_refuse_what_a_type_does_not_have() {
    let dir = out_dir("annotations-refuse");
    let src = sample_file(&dir, "src.pdf", SampleSpec::default());
    let mut doc = open(&src);
    let specs = every_kind(&doc, 0);
    let highlight = annot::create(&mut doc, &specs[0]).unwrap();
    let id = u32::try_from(highlight.xref).unwrap();
    for edit in [
        AnnotationEdit {
            bounds: Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
            ..AnnotationEdit::default()
        },
        AnnotationEdit {
            width: Some(2.0),
            ..AnnotationEdit::default()
        },
        AnnotationEdit {
            font_size: Some(12.0),
            ..AnnotationEdit::default()
        },
        AnnotationEdit {
            opacity: Some(1.5),
            ..AnnotationEdit::default()
        },
        AnnotationEdit::default(),
    ] {
        assert!(annot::edit(&mut doc, 0, id, &edit).is_err(), "{edit:?}");
    }
    assert!(
        annot::edit(
            &mut doc,
            0,
            99_999,
            &AnnotationEdit {
                author: Some("x".into()),
                ..AnnotationEdit::default()
            }
        )
        .is_err()
    );
}

/// The ExtGState blend modes of the normal appearance include Multiply.
fn blends_multiply(obj: &PdfObject) -> bool {
    let normal = get(&get(obj, "AP"), "N");
    let Some(gs) = get(&normal, "Resources").get_dict("ExtGState").unwrap() else {
        return false;
    };
    gs.dict_iter().unwrap().any(|entry| {
        let (_, state) = entry.unwrap();
        state
            .get_dict("BM")
            .unwrap()
            .is_some_and(|bm| bm.as_name().unwrap() == b"Multiply")
    })
}

#[test]
fn text_markup_changes_type_and_keeps_everything_else() {
    let dir = out_dir("annotations-change-type");
    let kinds = [
        MarkupKind::Highlight,
        MarkupKind::Underline,
        MarkupKind::StrikeOut,
        MarkupKind::Squiggly,
    ];
    for (case, spec) in geometry_cases() {
        let src = sample_file(&dir, &format!("{case}.pdf"), spec);
        let mut doc = open(&src);
        let specs = every_kind(&doc, 0);
        let mut ids = Vec::new();
        // Every type into every other: create one of each `from`, turn it into each `to` in
        // turn, ending where it started.
        for (i, from) in kinds.iter().enumerate() {
            let made = annot::create(&mut doc, &specs[i]).unwrap();
            let id = u32::try_from(made.xref).unwrap();
            let obj = doc.new_indirect(made.xref, 0).unwrap();
            let kept: Vec<(&str, Option<String>)> = [
                "QuadPoints",
                "C",
                "CA",
                "NM",
                "T",
                "Contents",
                "CreationDate",
                "F",
                "Popup",
            ]
            .into_iter()
            .map(|k| (k, raw(&obj, k)))
            .collect();
            let route: Vec<MarkupKind> = kinds
                .iter()
                .copied()
                .filter(|k| k != from)
                .chain([*from])
                .collect();
            for to in route {
                let name = format!("{case}: {from:?} -> {to:?}");
                annot::edit(
                    &mut doc,
                    0,
                    id,
                    &AnnotationEdit {
                        kind: Some(to),
                        ..AnnotationEdit::default()
                    },
                )
                .unwrap();
                let obj = doc.new_indirect(made.xref, 0).unwrap();
                assert_eq!(
                    get(&obj, "Subtype").as_name().unwrap(),
                    to.kind().subtype().as_bytes(),
                    "{name}"
                );
                for (key, value) in &kept {
                    assert_eq!(&raw(&obj, key), value, "{name}: /{key} changed");
                }
                // Rule 2 and rule 5: a fresh appearance, blending only for highlights.
                normal_appearance(&obj, &name);
                assert!(
                    obj.get_dict("AP")
                        .unwrap()
                        .unwrap()
                        .get_dict("D")
                        .unwrap()
                        .is_none(),
                    "{name}"
                );
                assert_eq!(
                    blends_multiply(&obj),
                    to == MarkupKind::Highlight,
                    "{name}: blend"
                );
                let quads =
                    quads_from_array(&objects::numbers(&get(&obj, "QuadPoints")).unwrap().unwrap())
                        .unwrap();
                let content = quads
                    .iter()
                    .map(Quad::bounds)
                    .reduce(|a, b| a.union(&b))
                    .unwrap()
                    .expand(1.0);
                assert!(contains(rect_of(&obj), content), "{name}: /Rect");
            }
            ids.push((id, *from));
        }
        let reopened = save_and_reopen(&doc, &src, &dir.join(format!("{case}-changed.pdf")));
        let listed = read_page(&reopened, 0).unwrap();
        for (id, kind) in ids {
            let a = listed.iter().find(|a| a.id == id).unwrap();
            assert_eq!(a.kind, Some(kind.kind()), "{case}");
            assert!(a.problems.is_empty(), "{case}: {:?}", a.problems);
        }
    }

    // Another app's highlight: only /Subtype, /M and the appearance (with the /Rect it
    // needs) change; its unknown look is replaced.
    let src = other_app_file(&dir);
    let mut doc = open(&src);
    let before = doc.new_indirect(6, 0).unwrap();
    let before: Vec<(String, Option<String>)> = keys(&before)
        .into_iter()
        .map(|k| (k.clone(), raw(&before, &k)))
        .collect();
    annot::edit(
        &mut doc,
        0,
        6,
        &AnnotationEdit {
            kind: Some(MarkupKind::Underline),
            ..AnnotationEdit::default()
        },
    )
    .unwrap();
    let underline = doc.new_indirect(6, 0).unwrap();
    assert_eq!(get(&underline, "Subtype").as_name().unwrap(), b"Underline");
    let allowed: BTreeSet<&str> = ["Subtype", "M", "AP", "Rect"].into();
    for (key, value) in &before {
        if !allowed.contains(key.as_str()) {
            assert_eq!(&raw(&underline, key), value, "/{key} changed");
        }
    }
    for key in keys(&underline) {
        assert!(
            before.iter().any(|(k, _)| *k == key) || allowed.contains(key.as_str()),
            "/{key} added"
        );
    }

    // Only text markup changes type.
    let src = sample_file(&dir, "refuse.pdf", SampleSpec::default());
    let mut doc = open(&src);
    let specs = every_kind(&doc, 0);
    for spec in &specs[4..] {
        let made = annot::create(&mut doc, spec).unwrap();
        let edit = AnnotationEdit {
            kind: Some(MarkupKind::Highlight),
            ..AnnotationEdit::default()
        };
        assert!(
            annot::edit(&mut doc, 0, u32::try_from(made.xref).unwrap(), &edit).is_err(),
            "{:?}",
            spec.body.kind()
        );
    }
}

#[test]
fn deleting_removes_the_popup_and_replies_only() {
    let dir = out_dir("annotations-delete");
    let src = other_app_file(&dir);
    let mut doc = open(&src);
    let annots_of = |doc: &PdfDocument| -> Vec<i32> {
        objects::array_items(&get(&doc.find_page(0).unwrap(), "Annots"))
            .unwrap()
            .iter()
            .map(|a| a.as_indirect().unwrap())
            .collect()
    };
    // Give the square a popup first, so there is one to remove.
    annot::edit(
        &mut doc,
        0,
        4,
        &AnnotationEdit {
            contents: Some("With popup".into()),
            ..AnnotationEdit::default()
        },
    )
    .unwrap();
    let popup = get(&doc.new_indirect(4, 0).unwrap(), "Popup")
        .as_indirect()
        .unwrap();
    assert!(annots_of(&doc).contains(&popup));

    annot::delete(&mut doc, 0, 4).unwrap();
    assert_eq!(
        annots_of(&doc),
        [6, 7, 8, 9, 10],
        "square, its popup and its reply go"
    );
    assert!(annot::delete(&mut doc, 0, 4).is_err());
}

#[test]
fn repair_fixes_other_apps_problems_without_changing_content() {
    let dir = out_dir("annotations-repair");
    let src = other_app_file(&dir);
    let mut doc = open(&src);

    let found = scan(&doc).unwrap();
    let count = |p| found.counts.get(&p).copied().unwrap_or(0);
    assert_eq!(count(Problem::MissingAppearance), 3, "{found:?}"); // highlight, note, text box
    assert_eq!(count(Problem::QuadOrder), 1);
    assert_eq!(count(Problem::RectTooSmall), 2); // highlight, drawing
    assert_eq!(count(Problem::MissingName), 2); // highlight, note
    assert_eq!(count(Problem::MissingFlags), 1);
    assert_eq!(count(Problem::MissingModified), 2);
    assert_eq!(count(Problem::MissingPage), 2);
    assert_eq!(count(Problem::MalformedQuads), 1);
    assert_eq!(found.fixable, 4);
    assert_eq!(found.unfixable, 1, "the underline's quads can't be read");

    let snapshot = |doc: &PdfDocument| -> Vec<(i32, Vec<Option<String>>)> {
        [4, 6, 7, 8, 9, 10, 11]
            .into_iter()
            .map(|n| {
                let o = doc.new_indirect(n, 0).unwrap();
                (
                    n,
                    ["Contents", "C", "T", "DA", "InkList", "IRT"]
                        .iter()
                        .map(|k| raw(&o, k))
                        .collect(),
                )
            })
            .collect()
    };
    let corners = |doc: &PdfDocument| -> Vec<(i64, i64)> {
        let o = doc.new_indirect(6, 0).unwrap();
        let mut v: Vec<(i64, i64)> = objects::numbers(&get(&o, "QuadPoints"))
            .unwrap()
            .unwrap()
            .chunks(2)
            .map(|c| (c[0] as i64, c[1] as i64))
            .collect();
        v.sort_unstable();
        v
    };
    let note_corner = |doc: &PdfDocument| {
        let r = rect_of(&doc.new_indirect(8, 0).unwrap());
        (r.x0, r.y1)
    };
    let content_before = snapshot(&doc);
    let corners_before = corners(&doc);
    let note_before = note_corner(&doc);

    let changes = repair(&mut doc).unwrap();
    for c in &changes {
        println!("{c}");
    }
    assert!(
        changes
            .iter()
            .any(|c| c.id == 6 && c.fixed == Problem::QuadOrder)
    );
    assert!(
        changes.iter().all(|c| c.id != 4 && c.id != 11),
        "nothing wrong there"
    );

    // Content, colour, author and position unchanged.
    assert_eq!(snapshot(&doc), content_before);
    assert_eq!(corners(&doc), corners_before);
    assert_eq!(note_corner(&doc), note_before);

    // Only the problem repair can't fix is left.
    let after = scan(&doc).unwrap();
    let left: BTreeSet<Problem> = after.counts.keys().copied().collect();
    assert_eq!(left, [Problem::MalformedQuads].into(), "{after:?}");
    assert!(
        changes.iter().all(|c| c.id != 10),
        "the underline is left alone"
    );
    for a in read_page(&doc, 0).unwrap() {
        if a.id != 10 {
            assert!(a.problems.is_empty(), "{}: {:?}", a.id, a.problems);
        }
    }
    let highlight = doc.new_indirect(6, 0).unwrap();
    let quads = quads_from_array(
        &objects::numbers(&get(&highlight, "QuadPoints"))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(quads.iter().all(is_acrobat_order));
    // The drawing's own appearance was kept and its BBox grown with its Rect.
    let ink = doc.new_indirect(7, 0).unwrap();
    let ink_rect = rect_of(&ink);
    assert!(
        contains(ink_rect, Rect::new(299.0, 299.0, 381.0, 381.0)),
        "{ink_rect:?}"
    );
    let bbox = objects::rect(&get(&get(&get(&ink, "AP"), "N"), "BBox"))
        .unwrap()
        .unwrap();
    assert!((bbox.x0 - (ink_rect.x0 - 300.0)).abs() < 0.01, "{bbox:?}");
    assert!((bbox.x1 - (ink_rect.x1 - 300.0)).abs() < 0.01, "{bbox:?}");

    let out = dir.join("repaired.pdf");
    let reopened = save_and_reopen(&doc, &src, &out);
    qpdf_check(&out);
    assert_eq!(snapshot(&reopened), content_before);
}

#[test]
fn annotations_are_undoable_steps_through_the_session() {
    let dir = out_dir("annotations-session");
    let src = sample_file(&dir, "src.pdf", SampleSpec::default());
    // Find the text in a copy: a file MuPDF has open can be inherited by a qpdf another
    // test starts meanwhile, which would block saving in place (see `common::writing`).
    let probe = sample_file(&dir, "probe.pdf", SampleSpec::default());
    let specs = every_kind(&open(&probe), 1);
    let (session, info) = Session::open(&src, None).unwrap();
    assert!(info.annotations.iter().all(Vec::is_empty));

    let change = session
        .apply(Operation::AddAnnotation {
            annotation: specs[0].clone(),
        })
        .unwrap();
    let id = change.created.unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Add highlight"));
    assert!(change.state.dirty);
    assert_eq!(change.annotations.len(), 1);
    let (page, list) = &change.annotations[0];
    assert_eq!(*page, 1);
    assert_eq!(list[0].id, id);
    assert_eq!(list[0].kind, Some(Kind::Highlight));
    assert_eq!(list[0].contents, "A note — ünïcödé");

    let note = session
        .apply(Operation::AddAnnotation {
            annotation: specs[4].clone(),
        })
        .unwrap();
    let note_id = note.created.unwrap();
    let moved = session
        .apply(Operation::UpdateAnnotation {
            page: 1,
            id: note_id,
            edit: AnnotationEdit {
                bounds: Some(Rect::new(300.0, 300.0, 320.0, 320.0)),
                ..AnnotationEdit::default()
            },
        })
        .unwrap();
    assert_eq!(moved.state.undo_name.as_deref(), Some("Move note"));
    let recolored = session
        .apply(Operation::UpdateAnnotation {
            page: 1,
            id,
            edit: AnnotationEdit {
                color: Some(Rgb::YELLOW),
                ..AnnotationEdit::default()
            },
        })
        .unwrap();
    assert_eq!(
        recolored.state.undo_name.as_deref(),
        Some("Change highlight")
    );
    let retyped = session
        .apply(Operation::UpdateAnnotation {
            page: 1,
            id,
            edit: AnnotationEdit {
                kind: Some(MarkupKind::Underline),
                ..AnnotationEdit::default()
            },
        })
        .unwrap();
    assert_eq!(
        retyped.state.undo_name.as_deref(),
        Some("Change highlight to underline")
    );
    assert_eq!(retyped.annotations[0].1[0].kind, Some(Kind::Underline));
    let back = session.undo().unwrap();
    assert_eq!(back.annotations[0].1[0].kind, Some(Kind::Highlight));
    assert_eq!(
        back.state.redo_name.as_deref(),
        Some("Change highlight to underline")
    );
    // Redone and changed back, so the saved file ends on a step that was kept: undoing a
    // step that made objects and then saving leaves a trailer /Size qpdf warns about
    // (docs/status.md).
    let redone = session.redo().unwrap();
    assert_eq!(redone.annotations[0].1[0].kind, Some(Kind::Underline));
    let restored = session
        .apply(Operation::UpdateAnnotation {
            page: 1,
            id,
            edit: AnnotationEdit {
                kind: Some(MarkupKind::Highlight),
                ..AnnotationEdit::default()
            },
        })
        .unwrap();
    assert_eq!(
        restored.state.undo_name.as_deref(),
        Some("Change underline to highlight")
    );
    let deleted = session
        .apply(Operation::DeleteAnnotation { page: 1, id })
        .unwrap();
    assert_eq!(deleted.state.undo_name.as_deref(), Some("Delete highlight"));
    assert_eq!(deleted.annotations[0].1.len(), 1, "only the note is left");

    let undone = session.undo().unwrap();
    assert_eq!(undone.annotations[0].1.len(), 2);
    assert_eq!(undone.state.revision, restored.state.revision);
    let redone = session.redo().unwrap();
    assert_eq!(redone.annotations[0].1.len(), 1);

    // Invalid edits change nothing and add no step.
    assert!(
        session
            .apply(Operation::UpdateAnnotation {
                page: 1,
                id: note_id,
                edit: AnnotationEdit {
                    width: Some(3.0),
                    ..AnnotationEdit::default()
                },
            })
            .is_err()
    );
    assert_eq!(session.info().unwrap().state, redone.state);

    let saved = session
        .save(pdf_core::save::SaveKind::Incremental, None)
        .unwrap();
    assert!(!saved.state.dirty);
    qpdf_check(&src);
    let (_, reopened) = Session::open(&src, None).unwrap();
    assert_eq!(reopened.annotations[1].len(), 1);
    assert_eq!(reopened.annotations[1][0].kind, Some(Kind::Note));
}

#[test]
fn repair_is_one_undo_step() {
    let dir = out_dir("annotations-repair-session");
    let src = other_app_file(&dir);
    let (session, info) = Session::open(&src, None).unwrap();
    let needing = |list: &[pdf_core::annot::read::AnnotationInfo]| {
        list.iter().filter(|a| !a.problems.is_empty()).count()
    };
    assert_eq!(needing(&info.annotations[0]), 5);
    let found = session.scan_annotations().unwrap();
    assert_eq!(found.fixable, 4);

    let change = session.apply(Operation::RepairAnnotations).unwrap();
    assert_eq!(
        change.state.undo_name.as_deref(),
        Some("Repair annotations")
    );
    let repairs = change.repairs.unwrap();
    assert!(!repairs.is_empty());
    assert_eq!(needing(&change.annotations[0].1), 1);

    let undone = session.undo().unwrap();
    assert_eq!(needing(&undone.annotations[0].1), 5);

    assert_eq!(session.scan_annotations().unwrap(), found);
}

#[test]
fn selected_text_becomes_quads_in_the_text_direction() {
    use pdf_core::text::{TextRange, page_text};
    let dir = out_dir("annotations-text-ranges");
    for (case, spec) in geometry_cases() {
        let src = sample_file(&dir, &format!("{case}-src.pdf"), spec);
        let mut doc = open(&src);
        // What the viewer gets: the page's structured text, line by line.
        let list = pdf_core::render::display_list(&doc, 0).unwrap();
        let text = page_text(&list).unwrap();
        let (line, at) = text
            .lines
            .iter()
            .enumerate()
            .find_map(|(i, l)| {
                l.text
                    .find("quick brown fox")
                    .map(|b| (i, l.text[..b].chars().count()))
            })
            .unwrap();
        let ranges = vec![TextRange {
            line,
            start: at,
            end: at + "quick brown fox".chars().count(),
        }];
        let created = annot::create(
            &mut doc,
            &NewAnnotation {
                page: 0,
                body: Body::TextMarkup {
                    kind: MarkupKind::Underline,
                    ranges,
                    note: None,
                },
                color: BLUE,
                opacity: 1.0,
                author: "Tëster".into(),
            },
        )
        .unwrap();
        // The same quad MuPDF's search finds for those words, in Acrobat order.
        let found = marker_quads(&doc, 0);
        let obj = doc.new_indirect(created.xref, 0).unwrap();
        let qp = objects::numbers(&get(&obj, "QuadPoints")).unwrap().unwrap();
        let written = quads_from_array(&qp).unwrap();
        assert_eq!(written.len(), 1, "{case}");
        assert!(is_acrobat_order(&written[0]), "{case}");
        let g = PageGeometry::new(&read_page_boxes(&doc.find_page(0).unwrap()).unwrap());
        let expected = found[0].view_to_user(&g);
        for (a, b) in [
            (written[0].ul, expected.ul),
            (written[0].ur, expected.ur),
            (written[0].ll, expected.ll),
            (written[0].lr, expected.lr),
        ] {
            assert!(
                (a.x - b.x).abs() < 0.05 && (a.y - b.y).abs() < 0.05,
                "{case}: {a:?} {b:?}"
            );
        }
        // A range off the page's text is refused.
        let bad = annot::create(
            &mut doc,
            &NewAnnotation {
                page: 0,
                body: Body::TextMarkup {
                    kind: MarkupKind::Highlight,
                    ranges: vec![TextRange {
                        line: 9999,
                        start: 0,
                        end: 1,
                    }],
                    note: None,
                },
                color: BLUE,
                opacity: 1.0,
                author: "x".into(),
            },
        );
        assert!(bad.is_err(), "{case}");
    }
}

#[test]
fn text_selected_across_pages_is_one_undo_step() {
    let dir = out_dir("annotations-multi");
    let src = sample_file(&dir, "src.pdf", SampleSpec::default());
    let probe = sample_file(&dir, "probe.pdf", SampleSpec::default());
    let probe = open(&probe);
    let mut specs = every_kind(&probe, 0);
    specs.truncate(1);
    let mut second = every_kind(&probe, 1);
    specs.push(second.remove(0));
    let (session, _) = Session::open(&src, None).unwrap();
    let change = session
        .apply(Operation::AddAnnotations { annotations: specs })
        .unwrap();
    assert_eq!(change.state.undo_name.as_deref(), Some("Add highlight"));
    assert_eq!(change.annotations.len(), 2);
    let undone = session.undo().unwrap();
    assert!(undone.annotations.iter().all(|(_, l)| l.is_empty()));
    assert!(
        session
            .apply(Operation::AddAnnotations {
                annotations: Vec::new()
            })
            .is_err()
    );
}
