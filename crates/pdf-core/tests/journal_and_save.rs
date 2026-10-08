//! Journalling (undo/redo through the FFI wrapper) and atomic-save failure paths.

mod common;

use std::fs;

use common::{open, out_dir, qpdf_check, sample_file};
use pdf_core::annot::quads::Quad;
use pdf_core::annot::{MarkupKind, MarkupSpec, Rgb, add_text_markup};
use pdf_core::ffi::Journal;
use pdf_core::geometry::Rect;
use pdf_core::save::{SaveKind, save_atomic};
use pdf_core::testgen::SampleSpec;

fn annotation_count(doc: &mupdf::pdf::PdfDocument) -> usize {
    doc.load_pdf_page(0).unwrap().annotations().count()
}

#[test]
fn undo_and_redo_an_annotation() {
    let dir = out_dir("journal");
    let src = sample_file(&dir, "src.pdf", SampleSpec::default());
    let mut doc = open(&src);
    Journal::new(&mut doc).enable().unwrap();
    assert_eq!(Journal::new(&mut doc).state().unwrap().steps, 0);

    doc.begin_operation("Add highlight").unwrap();
    add_text_markup(
        &mut doc,
        &MarkupSpec {
            kind: MarkupKind::Highlight,
            page: 0,
            quads: vec![Quad::from_view_rect(Rect::new(72.0, 100.0, 200.0, 112.0))],
            color: Rgb::YELLOW,
            opacity: 1.0,
            author: "Lectrix".into(),
            note: None,
        },
    )
    .unwrap();
    doc.end_operation().unwrap();
    assert_eq!(annotation_count(&doc), 1);

    let state = Journal::new(&mut doc).state().unwrap();
    assert_eq!((state.current, state.steps), (1, 1));
    assert_eq!(state.undo_name.as_deref(), Some("Add highlight"));
    assert_eq!(state.redo_name, None);

    Journal::new(&mut doc).undo().unwrap();
    assert_eq!(annotation_count(&doc), 0);
    let state = Journal::new(&mut doc).state().unwrap();
    assert_eq!(state.current, 0);
    assert_eq!(state.redo_name.as_deref(), Some("Add highlight"));

    Journal::new(&mut doc).redo().unwrap();
    assert_eq!(annotation_count(&doc), 1);

    // Errors from MuPDF come back as values, not aborts.
    Journal::new(&mut doc).redo().unwrap_err();
}

/// A highlight with a note: an annotation, a popup and an appearance stream, all new
/// objects.
fn add_highlight(doc: &mut mupdf::pdf::PdfDocument, y: f64) {
    doc.begin_operation("Add highlight").unwrap();
    add_text_markup(
        doc,
        &MarkupSpec {
            kind: MarkupKind::Highlight,
            page: 0,
            quads: vec![Quad::from_view_rect(Rect::new(72.0, y, 200.0, y + 12.0))],
            color: Rgb::YELLOW,
            opacity: 1.0,
            author: "Lectrix".into(),
            note: Some("note".into()),
        },
    )
    .unwrap();
    doc.end_operation().unwrap();
}

#[test]
fn saving_after_undoing_new_objects_writes_a_consistent_trailer() {
    let dir = out_dir("save-after-undo");
    for kind in [SaveKind::Incremental, SaveKind::Full] {
        // Undone on its own, and undone after a step that is kept.
        for keep_one in [false, true] {
            let name = format!("{kind:?}-{keep_one}");
            let src = sample_file(&dir, &format!("{name}-src.pdf"), SampleSpec::default());
            let mut doc = open(&src);
            Journal::new(&mut doc).enable().unwrap();
            if keep_one {
                add_highlight(&mut doc, 100.0);
            }
            add_highlight(&mut doc, 200.0);
            let with_both = annotation_count(&doc);
            Journal::new(&mut doc).undo().unwrap();
            let kept = annotation_count(&doc);
            assert_eq!(kept, usize::from(keep_one), "{name}");
            // A recovery copy is an incremental update too.
            let snapshot = dir.join(format!("{name}-snapshot.pdf"));
            pdf_core::ffi::save_snapshot(&mut doc, &snapshot).unwrap();
            qpdf_check(&snapshot);
            let out = dir.join(format!("{name}.pdf"));
            save_atomic(&doc, kind, Some(&src), &out).unwrap();
            qpdf_check(&out);
            assert_eq!(annotation_count(&open(&out)), kept, "{name}");
            // The undone step can still be redone.
            Journal::new(&mut doc).redo().unwrap();
            assert_eq!(annotation_count(&doc), with_both, "{name}");
        }
    }
}

#[test]
fn failed_save_leaves_no_temp_and_reports_missing_folder() {
    let dir = out_dir("save-fail");
    let src = sample_file(&dir, "src.pdf", SampleSpec::default());
    let doc = open(&src);
    let target = dir.join("missing-folder").join("out.pdf");
    assert!(save_atomic(&doc, SaveKind::Full, None, &target).is_err());
    assert!(!target.exists());
}

#[test]
fn incremental_save_keeps_original_bytes_as_prefix() {
    let dir = out_dir("save-incremental");
    let src = sample_file(&dir, "src.pdf", SampleSpec::default());
    let original = fs::read(&src).unwrap();
    let mut doc = open(&src);
    pdf_core::outline::write_outline(&mut doc, &[pdf_core::outline::OutlineItem::new("x", 0)])
        .unwrap();
    let out = dir.join("out.pdf");
    let outcome = save_atomic(&doc, SaveKind::Incremental, Some(&src), &out).unwrap();
    assert_eq!(outcome.kind, SaveKind::Incremental);
    let saved = fs::read(&out).unwrap();
    assert!(saved.len() > original.len());
    assert_eq!(&saved[..original.len()], &original[..]);
    // No temp files left behind.
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".lectrix-tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[cfg(windows)]
#[test]
fn locked_target_is_reported_and_left_untouched() {
    use std::os::windows::fs::OpenOptionsExt;

    use pdf_core::Error;

    let dir = out_dir("save-locked");
    let src = sample_file(&dir, "src.pdf", SampleSpec::default());
    let target = dir.join("locked.pdf");
    fs::write(&target, b"original contents").unwrap();
    // Open with no sharing, as Acrobat does with files it has open.
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&target)
        .unwrap();

    let doc = open(&src);
    match save_atomic(&doc, SaveKind::Full, None, &target) {
        Err(Error::TargetLocked(path)) => assert_eq!(path, target),
        other => panic!("expected TargetLocked, got {other:?}"),
    }
    drop(lock);
    assert_eq!(fs::read(&target).unwrap(), b"original contents");
    let leftovers = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".lectrix-tmp"))
        .count();
    assert_eq!(leftovers, 0);
}
