//! Files that claim PDF/A (AGENTS.md section 5.4): the claim is reported, such a file can be
//! opened for reading only, and a save that may have broken the standard takes the claim out
//! of the file, while one that cannot have keeps it.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{open, out_dir, qpdf_check, sample_file, writing};
use pdf_core::Error;
use pdf_core::annot::{Body, NewAnnotation, Rgb};
use pdf_core::docinfo::{PdfA, metadata_bytes, read_pdfa};
use pdf_core::geometry::Point;
use pdf_core::labels::{LabelRule, LabelStyle};
use pdf_core::merge::InsertOptions;
use pdf_core::ops::{InsertSource, Operation};
use pdf_core::outline::{ViewDest, read_bookmarks};
use pdf_core::save::SaveKind;
use pdf_core::session::{OpenOptions, Session};
use pdf_core::testgen::SampleSpec;

const CLAIM: Option<PdfA> = Some(PdfA {
    part: 1,
    conformance: Some('B'),
});

const READING: OpenOptions = OpenOptions {
    pdfa_read_only: true,
};

fn pdfa_file(dir: &Path, name: &str) -> PathBuf {
    sample_file(
        dir,
        name,
        SampleSpec {
            pages: 4,
            pdfa: CLAIM,
            ..SampleSpec::default()
        },
    )
}

fn rotate(page: usize) -> Operation {
    Operation::RotatePages {
        pages: vec![page],
        degrees: 90,
    }
}

fn bookmark(title: &str, parent: Option<u32>) -> Operation {
    Operation::AddBookmark {
        parent,
        index: 0,
        title: title.into(),
        dest: ViewDest {
            page: 1,
            x: 0.0,
            y: 0.0,
        },
    }
}

fn note() -> Operation {
    Operation::AddAnnotation {
        annotation: NewAnnotation {
            page: 0,
            body: Body::Note {
                at: Point::new(100.0, 100.0),
                text: "A note".into(),
            },
            color: Rgb::YELLOW,
            opacity: 1.0,
            author: "Tester".into(),
        },
    }
}

fn claim_on_disk(path: &Path) -> Option<PdfA> {
    read_pdfa(&open(path))
}

#[test]
fn open_reports_the_claim() {
    let dir = out_dir("pdfa-open");
    let plain = sample_file(&dir, "plain.pdf", SampleSpec::default());
    let (session, info) = Session::open(&plain, None).unwrap();
    assert_eq!(info.flags.pdfa, None);
    session.close();

    // The claim alone changes nothing: a session opens ready to edit unless it is asked
    // to open such files for reading only.
    let path = pdfa_file(&dir, "pdfa.pdf");
    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.flags.pdfa, CLAIM);
    assert!(!info.read_only);
    assert!(info.flags.can_annotate && info.flags.can_assemble);
    session.apply(rotate(0)).unwrap();
    session.close();

    // Only files that claim PDF/A are opened for reading only.
    let (session, info) = Session::open_with(&plain, None, READING).unwrap();
    assert!(!info.read_only);
    session.apply(rotate(0)).unwrap();
    session.close();
}

#[test]
fn a_file_open_for_reading_only_is_not_changed_until_editing_is_enabled() {
    let dir = out_dir("pdfa-read-only");
    let path = pdfa_file(&dir, "doc.pdf");
    // Bookmarks to expand and collapse: a parent with a child.
    let parent = {
        let (session, _) = Session::open(&path, None).unwrap();
        let parent = session
            .apply(bookmark("Parent", None))
            .unwrap()
            .created
            .unwrap();
        session.apply(bookmark("Child", Some(parent))).unwrap();
        writing(|| session.save(SaveKind::Incremental, None)).unwrap();
        session.close();
        parent
    };
    let stored_open = || read_bookmarks(&open(&path)).unwrap().items[0].open;
    let was_open = stored_open();

    let (session, info) = Session::open_with(&path, None, READING).unwrap();
    assert!(info.read_only);
    assert_eq!(info.flags.pdfa, CLAIM);
    // What the file itself allows is still reported.
    assert!(info.flags.can_annotate && info.flags.can_assemble);

    for op in [rotate(0), bookmark("New", None), note()] {
        assert!(matches!(session.apply(op), Err(Error::ReadOnly)));
    }
    let info = session.info().unwrap();
    assert!(!info.state.dirty);
    assert_eq!(info.state.undo_name, None);
    assert!(info.annotations.iter().all(Vec::is_empty));

    // The panel shows the bookmark collapsed, but a save does not write that.
    session.set_bookmark_open(parent, !was_open).unwrap();
    assert_eq!(session.info().unwrap().outline.items[0].open, !was_open);
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(stored_open(), was_open);
    assert!(session.info().unwrap().read_only, "saving keeps it so");

    session.enable_editing().unwrap();
    let info = session.info().unwrap();
    assert!(!info.read_only);
    assert_eq!(info.outline.items[0].open, !was_open);
    // Now the expanded state is written, and operations apply.
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(stored_open(), !was_open);
    session.apply(rotate(0)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    session.close();
    qpdf_check(&path);
    assert_eq!(claim_on_disk(&path), CLAIM);
}

#[test]
fn changes_that_cannot_break_the_standard_keep_the_claim() {
    let dir = out_dir("pdfa-kept");
    let path = pdfa_file(&dir, "doc.pdf");
    let (session, _) = Session::open(&path, None).unwrap();
    session.apply(rotate(0)).unwrap();
    session.apply(bookmark("Chapter", None)).unwrap();
    session
        .apply(Operation::SetPageLabels {
            rules: vec![LabelRule {
                start_page: 0,
                style: LabelStyle::LowerRoman,
                prefix: String::new(),
                first_number: 1,
            }],
        })
        .unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.pdfa_removed, None);
    assert_eq!(session.info().unwrap().flags.pdfa, CLAIM);
    assert_eq!(claim_on_disk(&path), CLAIM);

    // A copy, plain or optimized, keeps it too.
    for (name, kind) in [
        ("copy.pdf", SaveKind::Full),
        ("optimized.pdf", SaveKind::Optimized),
    ] {
        let target = dir.join(name);
        let saved = writing(|| session.save(kind, Some(target.clone()))).unwrap();
        assert_eq!(saved.pdfa_removed, None, "{name}");
        assert_eq!(claim_on_disk(&target), CLAIM, "{name}");
        qpdf_check(&target);
    }
    session.close();
}

#[test]
fn saving_annotations_or_inserted_pages_takes_the_claim_out() {
    let dir = out_dir("pdfa-removed");
    let other = sample_file(&dir, "other.pdf", SampleSpec::default());
    let insert = || Operation::InsertPages {
        source: InsertSource {
            path: other.clone(),
            password: None,
            pages: vec![0],
        },
        at: 1,
        options: InsertOptions::default(),
    };
    for (name, op, kind) in [
        ("note.pdf", note(), SaveKind::Incremental),
        ("note-optimized.pdf", note(), SaveKind::Optimized),
        ("inserted.pdf", insert(), SaveKind::Incremental),
    ] {
        let path = pdfa_file(&dir, name);
        let before = String::from_utf8(metadata_bytes(&open(&path)).unwrap()).unwrap();
        let (session, _) = Session::open(&path, None).unwrap();
        session.apply(op).unwrap();
        // Until it is saved, the document is the file's, claim included.
        assert_eq!(session.info().unwrap().flags.pdfa, CLAIM, "{name}");
        let target = (kind == SaveKind::Optimized).then(|| path.clone());
        let saved = writing(|| session.save(kind, target)).unwrap();
        assert_eq!(saved.pdfa_removed, CLAIM, "{name}");
        assert_eq!(session.info().unwrap().flags.pdfa, None, "{name}");

        // A later save has nothing left to remove.
        session.apply(rotate(0)).unwrap();
        let again = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
        assert_eq!(again.pdfa_removed, None, "{name}");
        session.close();

        qpdf_check(&path);
        let doc = open(&path);
        assert_eq!(read_pdfa(&doc), None, "{name}");
        // The rest of the metadata is as it was, in a stream without a filter.
        let after = String::from_utf8(metadata_bytes(&doc).unwrap()).unwrap();
        let expected: String = before
            .lines()
            .filter(|line| !line.contains("<pdfaid:"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(after, expected, "{name}");
        let metadata = doc
            .catalog()
            .unwrap()
            .get_dict("Metadata")
            .unwrap()
            .unwrap();
        assert!(metadata.get_dict("Filter").unwrap().is_none(), "{name}");
    }
}

#[test]
fn only_the_changes_that_are_saved_count() {
    let dir = out_dir("pdfa-undone");
    // An annotation that was undone leaves the claim.
    let path = pdfa_file(&dir, "undone.pdf");
    let (session, _) = Session::open(&path, None).unwrap();
    session.apply(note()).unwrap();
    session.undo().unwrap();
    session.apply(bookmark("Chapter", None)).unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.pdfa_removed, None);
    session.close();
    assert_eq!(claim_on_disk(&path), CLAIM);

    // One that is still there does not, whatever was undone after it; redone, it counts
    // again.
    let path = pdfa_file(&dir, "kept.pdf");
    let (session, _) = Session::open(&path, None).unwrap();
    session.apply(note()).unwrap();
    session.apply(bookmark("Chapter", None)).unwrap();
    session.undo().unwrap();
    session.undo().unwrap();
    session.redo().unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.pdfa_removed, CLAIM);
    session.close();
    assert_eq!(claim_on_disk(&path), None);
}

#[test]
fn a_save_that_fails_leaves_the_claim_where_it_was() {
    let dir = out_dir("pdfa-failed-save");
    let path = pdfa_file(&dir, "doc.pdf");
    let taken = dir.join("taken.pdf");
    let _ = fs::remove_dir_all(&taken);
    fs::create_dir_all(&taken).unwrap();

    let (session, _) = Session::open(&path, None).unwrap();
    session.apply(note()).unwrap();
    assert!(writing(|| session.save(SaveKind::Full, Some(taken.clone()))).is_err());
    assert_eq!(session.info().unwrap().flags.pdfa, CLAIM);
    assert_eq!(claim_on_disk(&path), CLAIM);

    // Without the annotation, the next save keeps the claim: the failed one did not
    // leave the document without it.
    session.undo().unwrap();
    session.apply(bookmark("Chapter", None)).unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.pdfa_removed, None);
    session.close();
    qpdf_check(&path);
    assert_eq!(claim_on_disk(&path), CLAIM);
}

#[test]
fn a_restored_document_is_edited_and_loses_the_claim_when_saved() {
    let dir = out_dir("pdfa-restored");
    let path = pdfa_file(&dir, "doc.pdf");
    let copy = dir.join("copy.pdf");
    {
        let (session, _) = Session::open(&path, None).unwrap();
        session.apply(bookmark("Chapter", None)).unwrap();
        writing(|| session.write_recovery(&copy, None)).unwrap();
        // The app crashes: the session goes without saving.
    }
    let (session, info) = Session::restore(&copy, &path, None).unwrap();
    assert_eq!(info.flags.pdfa, CLAIM);
    assert!(!info.read_only && info.state.dirty);
    // What the restored changes were is not known, so the claim does not survive them.
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.pdfa_removed, CLAIM);
    session.close();
    qpdf_check(&path);
    assert_eq!(claim_on_disk(&path), None);
}
