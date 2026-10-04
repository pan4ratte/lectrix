//! Crash recovery (AGENTS.md section 7): recovery copies written from a session leave it
//! untouched, and a restored session belongs to the original file, is dirty, and saves
//! onto it incrementally.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{open, out_dir, qpdf_check, sample_file, writing};
use mupdf::pdf::{Encryption, PdfWriteOptions};
use pdf_core::Error;
use pdf_core::geometry::{PageGeometry, read_page_boxes};
use pdf_core::ops::Operation;
use pdf_core::save::SaveKind;
use pdf_core::session::{RecoveryWrite, Session};
use pdf_core::testgen::{SampleSpec, sample_document};

fn rotate(page: usize) -> Operation {
    Operation::RotatePages {
        pages: vec![page],
        degrees: 90,
    }
}

fn rotation_on_disk(path: &Path, page: i32) -> i32 {
    let doc = open(path);
    PageGeometry::new(&read_page_boxes(&doc.find_page(page).unwrap()).unwrap()).rotation
}

fn fresh_dir(name: &str) -> PathBuf {
    let dir = out_dir(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn written(write: RecoveryWrite) -> u64 {
    match write {
        RecoveryWrite::Written { revision } => revision,
        other => panic!("expected a written copy, got {other:?}"),
    }
}

#[test]
fn copies_are_written_only_for_new_unsaved_revisions_and_leave_the_session_alone() {
    let dir = fresh_dir("recovery-write");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let copy = dir.join("copy.pdf");
    let (session, _) = Session::open(&path, None).unwrap();

    assert_eq!(
        session.write_recovery(&copy, None).unwrap(),
        RecoveryWrite::Clean
    );
    assert!(!copy.exists());

    session.apply(rotate(0)).unwrap();
    session.apply(rotate(1)).unwrap();
    let undone = session.undo().unwrap().state;
    let revision = writing(|| session.write_recovery(&copy, None))
        .map(written)
        .unwrap();
    assert_eq!(revision, undone.revision);
    assert_eq!(
        session.write_recovery(&copy, Some(revision)).unwrap(),
        RecoveryWrite::Unchanged
    );
    // The session's state, history and file are as they were.
    assert_eq!(session.info().unwrap().state, undone);
    assert_eq!(rotation_on_disk(&path, 0), 0);
    assert_eq!(session.redo().unwrap().state.redo_name, None);
    session.undo().unwrap();
    session.undo().unwrap();
    assert_eq!(
        session.write_recovery(&copy, Some(revision)).unwrap(),
        RecoveryWrite::Clean
    );

    // A saved document has nothing to recover either.
    session.apply(rotate(2)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(
        session.write_recovery(&copy, None).unwrap(),
        RecoveryWrite::Clean
    );
}

#[test]
fn a_restored_session_is_dirty_and_saves_onto_the_original_incrementally() {
    let dir = fresh_dir("recovery-restore");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let original = fs::read(&path).unwrap();
    let copy = dir.join("copy.pdf");
    {
        let (session, _) = Session::open(&path, None).unwrap();
        session.apply(rotate(0)).unwrap();
        writing(|| session.write_recovery(&copy, None))
            .map(written)
            .unwrap();
        // The app crashes: the session goes without saving.
    }
    assert_eq!(fs::read(&path).unwrap(), original);

    let (session, info) = Session::restore(&copy, &path, None).unwrap();
    assert_eq!(info.path, path);
    assert!(info.state.dirty);
    assert_eq!(
        info.state.undo_name, None,
        "the history starts at the restore"
    );
    assert_eq!(info.pages[0].width, 792.0, "page 1 is turned");

    // Undoing a new edit returns to the restored content, which is still unsaved.
    session.apply(rotate(1)).unwrap();
    assert!(session.undo().unwrap().state.dirty);
    session.redo().unwrap();

    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert!(!saved.state.dirty);
    assert_eq!(saved.path, path);
    assert!(!saved.outcome.fell_back_to_full);
    let bytes = fs::read(&path).unwrap();
    assert!(
        bytes.starts_with(&original),
        "the original bytes stay intact"
    );
    qpdf_check(&path);
    assert_eq!(
        (rotation_on_disk(&path, 0), rotation_on_disk(&path, 1)),
        (90, 90)
    );

    // The session now reads the saved file: the copy can go, and saving still works.
    fs::remove_file(&copy).unwrap();
    session.apply(rotate(2)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(rotation_on_disk(&path, 2), 90);
    qpdf_check(&path);
}

#[test]
fn a_restored_session_writes_recovery_copies_of_its_own() {
    let dir = fresh_dir("recovery-again");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let original = fs::read(&path).unwrap();
    let first = dir.join("first.pdf");
    let second = dir.join("second.pdf");
    {
        let (session, _) = Session::open(&path, None).unwrap();
        session.apply(rotate(0)).unwrap();
        writing(|| session.write_recovery(&first, None)).unwrap();
    }
    {
        // Restored, edited, and crashed again before saving.
        let (session, _) = Session::restore(&first, &path, None).unwrap();
        let revision = session.info().unwrap().state.revision;
        assert_eq!(
            writing(|| session.write_recovery(&second, None))
                .map(written)
                .unwrap(),
            revision,
            "a restored document is unsaved, so it has something to recover"
        );
        session.apply(rotate(1)).unwrap();
        writing(|| session.write_recovery(&second, None)).unwrap();
    }
    fs::remove_file(&first).unwrap();
    let (session, info) = Session::restore(&second, &path, None).unwrap();
    assert_eq!((info.pages[0].width, info.pages[1].width), (792.0, 792.0));
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert!(fs::read(&path).unwrap().starts_with(&original));
    qpdf_check(&path);
}

#[test]
fn reloading_a_restored_session_reads_the_original_file() {
    let dir = fresh_dir("recovery-reload");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let copy = dir.join("copy.pdf");
    {
        let (session, _) = Session::open(&path, None).unwrap();
        session.apply(rotate(0)).unwrap();
        writing(|| session.write_recovery(&copy, None)).unwrap();
    }
    let (session, _) = Session::restore(&copy, &path, None).unwrap();
    let info = session.reload().unwrap();
    assert!(!info.state.dirty);
    assert_eq!(info.pages[0].width, 612.0, "the changes are discarded");
    fs::remove_file(&copy).unwrap();
    session.apply(rotate(1)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(rotation_on_disk(&path, 1), 90);
}

#[test]
fn a_repaired_document_is_copied_in_full_without_disturbing_it() {
    let dir = fresh_dir("recovery-repaired");
    let good = sample_file(&dir, "good.pdf", SampleSpec::default());
    let mut bytes = fs::read(&good).unwrap();
    let pos = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
    for b in bytes[pos + 10..].iter_mut() {
        if b.is_ascii_digit() {
            *b = b'9';
        } else if *b == b'%' {
            break;
        }
    }
    let path = dir.join("damaged.pdf");
    fs::write(&path, &bytes).unwrap();
    let copy = dir.join("copy.pdf");

    let (session, info) = Session::open(&path, None).unwrap();
    assert!(info.flags.repaired);
    session.apply(rotate(0)).unwrap();
    session.apply(rotate(1)).unwrap();
    let state = session.undo().unwrap().state;
    writing(|| session.write_recovery(&copy, None))
        .map(written)
        .unwrap();
    qpdf_check(&copy);
    assert_eq!(session.info().unwrap().state, state);
    assert_eq!(rotation_on_disk(&copy, 0), 90);
    assert_eq!(rotation_on_disk(&copy, 1), 0);

    // The session itself still saves as before (in full, as the file is damaged).
    session.redo().unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert!(saved.outcome.fell_back_to_full);
    qpdf_check(&path);
    assert_eq!(
        (rotation_on_disk(&path, 0), rotation_on_disk(&path, 1)),
        (90, 90)
    );

    // The copy restores like any other.
    let restored = dir.join("restored-target.pdf");
    fs::copy(&good, &restored).unwrap();
    let (session, info) = Session::restore(&copy, &restored, None).unwrap();
    assert!(info.state.dirty);
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(rotation_on_disk(&restored, 0), 90);
    qpdf_check(&restored);
}

#[test]
fn an_encrypted_document_stays_encrypted_in_its_copy() {
    let dir = fresh_dir("recovery-encrypted");
    let path = dir.join("encrypted.pdf");
    let doc = sample_document(&SampleSpec::default()).unwrap();
    let mut options = PdfWriteOptions::default();
    options
        .set_encryption(Encryption::Aes256)
        .set_user_password("open sesame")
        .set_owner_password("owner");
    writing(|| doc.save_with_options(path.to_str().unwrap(), options)).unwrap();
    let copy = dir.join("copy.pdf");
    {
        let (session, _) = Session::open(&path, Some("open sesame")).unwrap();
        session.apply(rotate(0)).unwrap();
        writing(|| session.write_recovery(&copy, None)).unwrap();
    }

    assert!(matches!(
        Session::restore(&copy, &path, None),
        Err(Error::PasswordRequired)
    ));
    let (session, info) = Session::restore(&copy, &path, Some("open sesame")).unwrap();
    assert!(info.flags.encrypted);
    assert_eq!(info.pages[0].width, 792.0);
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert!(matches!(
        Session::open(&path, None),
        Err(Error::PasswordRequired)
    ));
    let (check, info) = Session::open(&path, Some("open sesame")).unwrap();
    assert_eq!(info.pages[0].width, 792.0);
    check.close();
}
