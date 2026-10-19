//! Giving a file a password and restrictions (Protect, section 6.9, ADR 0026): as a tool on
//! a file, and as a change of a document's protection written by its next save. What is
//! written is checked with MuPDF and with qpdf, an independent reader.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{out_dir, qpdf_check_with, qpdf_encryption, sample_file, writing};
use mupdf::TextPageFlags;
use mupdf::pdf::{Encryption, PdfDocument, PdfWriteOptions, Permission};
use pdf_core::Error;
use pdf_core::docinfo::{self, PdfA};
use pdf_core::ffi::{crypt_info, open_pdf_shared};
use pdf_core::ops::Operation;
use pdf_core::protection::{
    Changes, Cipher, Printing, ProtectionSettings, Restrict, Restrictions, inspect, protect_file,
};
use pdf_core::save::SaveKind;
use pdf_core::session::{OpenOptions, PendingProtection, ProtectionChange, Session};
use pdf_core::testgen::{MARKER, SampleSpec, sample_document};

const STRICT: Restrictions = Restrictions {
    printing: Printing::LowResolution,
    changes: Changes::CommentsFormsAndSigning,
    copying: false,
};

fn settings(open: Option<&str>, permissions: Option<&str>) -> ProtectionSettings {
    ProtectionSettings {
        open_password: open.map(str::to_owned),
        restrict: permissions.map(|password| Restrict {
            password: password.to_owned(),
            restrictions: STRICT,
        }),
    }
}

fn plain(dir: &Path) -> PathBuf {
    sample_file(
        dir,
        "plain.pdf",
        SampleSpec {
            pages: 3,
            ..SampleSpec::default()
        },
    )
}

/// The permission bits with a meaning, as `doc` applies them now.
fn allowed(doc: &PdfDocument) -> u32 {
    doc.permissions().bits()
}

fn assert_text_kept(doc: &PdfDocument) {
    assert_eq!(doc.page_count().unwrap(), 3);
    for i in 0..3 {
        let text = doc
            .load_page(i)
            .unwrap()
            .to_text_page(TextPageFlags::empty())
            .unwrap()
            .to_text()
            .unwrap();
        assert!(text.contains(MARKER), "page {i} lost its text");
    }
}

fn assert_aes256(doc: &PdfDocument) {
    let crypt = crypt_info(doc).unwrap().expect("encrypted");
    assert_eq!(
        (crypt.revision, crypt.length, crypt.method.as_str()),
        (6, 256, "AES")
    );
}

#[test]
fn a_password_to_open_alone() {
    let dir = out_dir("protect-open");
    let path = plain(&dir);
    let target = dir.join("protected.pdf");
    let original = fs::read(&path).unwrap();

    let removed =
        writing(|| protect_file(&path, None, &settings(Some("open sesame"), None), &target))
            .unwrap();
    assert_eq!(removed, None);
    assert_eq!(
        fs::read(&path).unwrap(),
        original,
        "the original is only read"
    );

    let mut doc = open_pdf_shared(&target).unwrap();
    assert_aes256(&doc);
    assert!(doc.needs_password().unwrap(), "an empty password opens it");
    assert!(!doc.authenticate("wrong").unwrap());
    assert!(doc.authenticate("open sesame").unwrap());
    // Nothing is restricted.
    assert_eq!(allowed(&doc), Permission::all().bits());
    assert_text_kept(&doc);
    qpdf_check_with(&target, Some("open sesame"));

    // qpdf, which tries the empty password as the permissions password too, can't open it
    // without the password: the permissions password is a random one, not an empty one.
    if let Some(shown) = qpdf_encryption(&target, None) {
        let shown = shown.unwrap_or_else(|e| e);
        assert!(
            shown.contains("Incorrect password supplied"),
            "qpdf opened it without a password: {shown}"
        );
        let shown = qpdf_encryption(&target, Some("open sesame"))
            .unwrap()
            .unwrap();
        assert!(shown.contains("R = 6"), "{shown}");
        assert!(
            shown.contains("Supplied password is user password"),
            "{shown}"
        );
        assert!(!shown.contains("is owner password"), "{shown}");
    }
}

#[test]
fn restrictions_alone_open_without_a_password() {
    let dir = out_dir("protect-restrictions");
    let path = plain(&dir);
    let target = dir.join("protected.pdf");

    writing(|| protect_file(&path, None, &settings(None, Some("owner")), &target)).unwrap();

    let mut doc = open_pdf_shared(&target).unwrap();
    assert_aes256(&doc);
    assert!(!doc.needs_password().unwrap());
    assert_eq!(allowed(&doc), STRICT.permissions().bits());
    assert!(doc.authenticate("owner").unwrap());
    assert_eq!(allowed(&doc), Permission::all().bits());
    qpdf_check_with(&target, None);

    // An independent reader sees the same restrictions.
    if let Some(shown) = qpdf_encryption(&target, None) {
        let shown = shown.unwrap();
        for line in [
            "print low resolution: allowed",
            "print high resolution: not allowed",
            "modify annotations: allowed",
            "modify forms: allowed",
            "modify document assembly: not allowed",
            "modify other: not allowed",
            "extract for any purpose: not allowed",
            "extract for accessibility: allowed",
        ] {
            assert!(shown.contains(line), "missing {line:?} in:\n{shown}");
        }
    }
}

#[test]
fn both_passwords_each_do_their_part() {
    let dir = out_dir("protect-both");
    let path = plain(&dir);
    let target = dir.join("protected.pdf");

    writing(|| protect_file(&path, None, &settings(Some("user"), Some("owner")), &target)).unwrap();

    let mut doc = open_pdf_shared(&target).unwrap();
    assert!(doc.needs_password().unwrap());
    assert!(doc.authenticate("user").unwrap());
    assert_eq!(allowed(&doc), STRICT.permissions().bits());
    let mut doc = open_pdf_shared(&target).unwrap();
    assert!(doc.authenticate("owner").unwrap());
    assert_eq!(allowed(&doc), Permission::all().bits());
    assert_text_kept(&doc);
    qpdf_check_with(&target, Some("user"));
}

#[test]
fn unicode_passwords_open_the_file() {
    let dir = out_dir("protect-unicode");
    let path = plain(&dir);
    let target = dir.join("protected.pdf");

    writing(|| {
        protect_file(
            &path,
            None,
            &settings(Some("пароль ключ"), Some("所有者")),
            &target,
        )
    })
    .unwrap();
    let mut doc = open_pdf_shared(&target).unwrap();
    assert!(doc.authenticate("пароль ключ").unwrap());
    let mut doc = open_pdf_shared(&target).unwrap();
    assert!(doc.authenticate("所有者").unwrap());
    assert_eq!(allowed(&doc), Permission::all().bits());
    qpdf_check_with(&target, Some("пароль ключ"));
}

#[test]
fn new_protection_replaces_the_old_and_the_file_can_be_replaced() {
    let dir = out_dir("protect-replace");
    let path = plain(&dir);
    writing(|| protect_file(&path, None, &settings(Some("old"), None), &path)).unwrap();

    let before = inspect(&path, Some("old")).unwrap();
    let protection = before.protection.unwrap();
    assert!(protection.open_password);
    assert!(!protection.restricted);

    writing(|| protect_file(&path, Some("old"), &settings(None, Some("owner")), &path)).unwrap();
    let mut doc = open_pdf_shared(&path).unwrap();
    assert!(!doc.needs_password().unwrap(), "the old password is gone");
    assert!(!doc.authenticate("old").unwrap() || allowed(&doc) != Permission::all().bits());
    assert_eq!(allowed(&doc), STRICT.permissions().bits());
}

#[test]
fn inspect_reads_what_the_file_has() {
    let dir = out_dir("protect-inspect");
    let path = plain(&dir);
    assert_eq!(inspect(&path, None).unwrap().protection, None);

    let target = dir.join("protected.pdf");
    writing(|| protect_file(&path, None, &settings(Some("user"), Some("owner")), &target)).unwrap();
    assert!(matches!(
        inspect(&target, None),
        Err(Error::PasswordRequired)
    ));

    let as_user = inspect(&target, Some("user")).unwrap().protection.unwrap();
    assert!(as_user.open_password);
    assert!(as_user.restricted);
    assert_eq!(as_user.restrictions, STRICT);
    assert_eq!((as_user.cipher, as_user.key_bits), (Cipher::Aes, 256));
    assert!(!as_user.unrestricted);
    assert!(as_user.allowed.print && !as_user.allowed.print_high_resolution);
    assert!(as_user.allowed.annotate && as_user.allowed.fill_forms);
    assert!(!as_user.allowed.copy && !as_user.allowed.modify && !as_user.allowed.assemble);

    // The permissions password lifts the restrictions, but the file still has them, and
    // still needs a password to open.
    let as_owner = inspect(&target, Some("owner")).unwrap().protection.unwrap();
    assert!(as_owner.open_password);
    assert_eq!(as_owner.restrictions, STRICT);
    assert!(as_owner.unrestricted);
}

#[test]
fn an_older_apps_rc4_file_reads_as_such() {
    let dir = out_dir("protect-rc4");
    let path = dir.join("rc4.pdf");
    let doc = sample_document(&SampleSpec::default()).unwrap();
    let mut options = PdfWriteOptions::default();
    options
        .set_encryption(Encryption::Rc4_128)
        .set_permissions(Permission::PRINT)
        .set_user_password("")
        .set_owner_password("owner");
    writing(|| doc.save_with_options(path.to_str().unwrap(), options)).unwrap();

    let protection = inspect(&path, None).unwrap().protection.unwrap();
    assert_eq!((protection.cipher, protection.key_bits), (Cipher::Rc4, 128));
    assert!(!protection.open_password);
    assert_eq!(protection.restrictions.printing, Printing::LowResolution);
    assert_eq!(protection.restrictions.changes, Changes::None);
}

#[test]
fn a_pdfa_claim_goes_with_new_protection() {
    let dir = out_dir("protect-pdfa");
    let level = PdfA {
        part: 2,
        conformance: Some('B'),
    };
    let path = sample_file(
        &dir,
        "pdfa.pdf",
        SampleSpec {
            pdfa: Some(level),
            ..SampleSpec::default()
        },
    );
    let target = dir.join("protected.pdf");
    assert_eq!(inspect(&path, None).unwrap().pdfa, Some(level));

    let removed =
        writing(|| protect_file(&path, None, &settings(None, Some("owner")), &target)).unwrap();
    assert_eq!(removed, Some(level));
    let doc = open_pdf_shared(&target).unwrap();
    assert_eq!(docinfo::read_pdfa(&doc), None);
}

#[test]
fn bad_settings_write_nothing() {
    let dir = out_dir("protect-bad");
    let path = plain(&dir);
    let target = dir.join("protected.pdf");
    let _ = fs::remove_file(&target);

    for bad in [
        settings(None, None),
        settings(Some(""), None),
        settings(Some("same"), Some("same")),
        settings(Some(&"x".repeat(128)), None),
    ] {
        assert!(matches!(
            protect_file(&path, None, &bad, &target),
            Err(Error::InvalidArgument(_))
        ));
        assert!(!target.exists());
    }
    // A wrong password for a protected file writes nothing either.
    let locked = dir.join("locked.pdf");
    writing(|| protect_file(&path, None, &settings(Some("user"), None), &locked)).unwrap();
    assert!(matches!(
        protect_file(
            &locked,
            Some("wrong"),
            &settings(Some("new"), None),
            &target
        ),
        Err(Error::WrongPassword)
    ));
    assert!(!target.exists());
}

#[test]
fn a_documents_protection_is_written_by_its_next_save() {
    let dir = out_dir("protect-session");
    let path = plain(&dir);
    let (session, info) = Session::open(&path, None).unwrap();
    assert!(!info.state.dirty);
    assert_eq!(session.protection().unwrap(), None);

    // A change of protection makes the document unsaved, but is no undo step.
    let state = session
        .set_protection(Some(ProtectionChange::Set(settings(
            Some("user"),
            Some("owner"),
        ))))
        .unwrap();
    assert!(state.dirty);
    assert_eq!(state.undo_name, None);
    assert_eq!(
        state.protection,
        Some(PendingProtection::Set {
            open_password: true,
            restrictions: Some(STRICT),
        })
    );
    // An edit and its undo leave it unsaved still.
    session
        .apply(Operation::RotatePages {
            pages: vec![0],
            degrees: 90,
        })
        .unwrap();
    let state = session.undo().unwrap().state;
    assert!(state.dirty);
    assert!(state.protection.is_some());

    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.outcome.kind, SaveKind::Full);
    assert!(!saved.outcome.fell_back_to_full);
    assert!(!saved.state.dirty);
    assert_eq!(saved.state.protection, None);
    // It goes on with the permissions password, so nothing is refused to its author.
    assert_eq!(saved.password, Some(Some("owner".to_owned())));
    let info = session.info().unwrap();
    assert!(info.flags.encrypted);
    assert!(info.flags.can_annotate && info.flags.can_assemble && info.flags.can_copy);
    let protection = session.protection().unwrap().unwrap();
    assert!(protection.open_password && protection.restricted && protection.unrestricted);

    let mut doc = open_pdf_shared(&path).unwrap();
    assert!(doc.needs_password().unwrap());
    assert!(doc.authenticate("user").unwrap());
    assert_eq!(allowed(&doc), STRICT.permissions().bits());
    qpdf_check_with(&path, Some("user"));

    // Later saves keep it, incrementally.
    session
        .apply(Operation::RotatePages {
            pages: vec![1],
            degrees: 90,
        })
        .unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.outcome.kind, SaveKind::Incremental);
    assert_eq!(saved.password, None);
    let mut doc = open_pdf_shared(&path).unwrap();
    assert!(doc.authenticate("user").unwrap());
    qpdf_check_with(&path, Some("user"));

    // Removing it writes the file without encryption.
    let state = session
        .set_protection(Some(ProtectionChange::Remove))
        .unwrap();
    assert_eq!(state.protection, Some(PendingProtection::Remove));
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.password, Some(None));
    assert!(!session.info().unwrap().flags.encrypted);
    let doc = open_pdf_shared(&path).unwrap();
    assert!(crypt_info(&doc).unwrap().is_none());
    assert_text_kept(&doc);
}

#[test]
fn a_documents_protection_change_can_be_taken_back() {
    let dir = out_dir("protect-session-back");
    let path = plain(&dir);
    let original = fs::read(&path).unwrap();
    let (session, _) = Session::open(&path, None).unwrap();

    // Removing protection the file doesn't have changes nothing.
    let state = session
        .set_protection(Some(ProtectionChange::Remove))
        .unwrap();
    assert!(!state.dirty);
    assert_eq!(state.protection, None);

    session
        .set_protection(Some(ProtectionChange::Set(settings(Some("user"), None))))
        .unwrap();
    let state = session.set_protection(None).unwrap();
    assert!(!state.dirty);
    assert_eq!(state.protection, None);

    // Bad settings are refused, and reloading forgets a change.
    assert!(
        session
            .set_protection(Some(ProtectionChange::Set(settings(None, None))))
            .is_err()
    );
    session
        .set_protection(Some(ProtectionChange::Set(settings(Some("user"), None))))
        .unwrap();
    let info = session.reload().unwrap();
    assert!(!info.state.dirty);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn a_reload_that_fails_leaves_the_document_with_its_password() {
    // Protect in place on a file open in a tab: the file gets a new password, and the
    // tab reads it again with it. If that fails, the document in memory still opens with
    // the old one.
    let dir = out_dir("protect-session-reload");
    let source = plain(&dir);
    let path = dir.join("protected.pdf");
    writing(|| protect_file(&source, None, &settings(Some("old"), None), &path)).unwrap();
    let (session, _) = Session::open(&path, Some("old")).unwrap();
    writing(|| protect_file(&path, Some("old"), &settings(Some("new"), None), &path)).unwrap();

    assert!(matches!(
        session.reload_with(Some("wrong".to_owned())),
        Err(Error::WrongPassword)
    ));
    let protection = session.protection().unwrap().unwrap();
    assert!(protection.open_password);
    // Its streams still decrypt.
    for page in 0..3 {
        let (text, _) = session.page_text(page).unwrap();
        assert!(
            text.plain_text().contains(MARKER),
            "page {page} lost its text"
        );
    }

    // The right password opens the file as it is now.
    session.reload_with(Some("new".to_owned())).unwrap();
}

#[test]
fn a_file_protected_under_unsaved_changes_stays_protected_when_they_are_saved() {
    // Protect as a tool on a file open in a tab with unsaved changes: the tab keeps them,
    // and is told what its file has now.
    let dir = out_dir("protect-session-dirty");
    let path = plain(&dir);
    let (session, _) = Session::open(&path, None).unwrap();
    session
        .apply(Operation::RotatePages {
            pages: vec![0],
            degrees: 90,
        })
        .unwrap();
    let new = settings(Some("user"), Some("owner"));
    writing(|| protect_file(&path, None, &new, &path)).unwrap();
    let state = session
        .protection_written(ProtectionChange::Set(new))
        .unwrap();
    assert!(state.dirty);
    assert!(state.protection.is_some());

    // Saved, its changes are in the file, and so is the protection.
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.outcome.kind, SaveKind::Full);
    let mut doc = open_pdf_shared(&path).unwrap();
    assert!(doc.needs_password().unwrap());
    assert!(doc.authenticate("user").unwrap());
    assert_aes256(&doc);
    assert_eq!(
        doc.find_page(0)
            .unwrap()
            .get_dict("Rotate")
            .unwrap()
            .unwrap()
            .as_int()
            .unwrap(),
        90
    );
    assert_text_kept(&doc);
    qpdf_check_with(&path, Some("user"));
}

#[test]
fn a_file_protected_under_unsaved_changes_is_read_again_with_its_new_password() {
    let dir = out_dir("protect-session-dirty-reload");
    let path = plain(&dir);
    let (session, _) = Session::open(&path, None).unwrap();
    session
        .apply(Operation::RotatePages {
            pages: vec![0],
            degrees: 90,
        })
        .unwrap();
    let new = settings(Some("user"), None);
    writing(|| protect_file(&path, None, &new, &path)).unwrap();
    session
        .protection_written(ProtectionChange::Set(new))
        .unwrap();
    // Its changes are given up: the file as it is now, with its password.
    let info = session.reload().unwrap();
    assert!(!info.state.dirty);
    assert!(info.flags.encrypted);
    assert!(session.protection().unwrap().unwrap().open_password);
}

#[test]
fn save_as_optimized_writes_the_protection_too() {
    let dir = out_dir("protect-session-optimized");
    let path = plain(&dir);
    let target = dir.join("optimized.pdf");
    let (session, _) = Session::open(&path, None).unwrap();
    session
        .set_protection(Some(ProtectionChange::Set(settings(None, Some("owner")))))
        .unwrap();
    let saved = writing(|| session.save(SaveKind::Optimized, Some(target.clone()))).unwrap();
    assert_eq!(saved.outcome.kind, SaveKind::Optimized);

    let doc = open_pdf_shared(&target).unwrap();
    assert_aes256(&doc);
    assert!(!doc.needs_password().unwrap());
    assert_eq!(allowed(&doc), STRICT.permissions().bits());
    assert_text_kept(&doc);
    qpdf_check_with(&target, None);
}

#[test]
fn a_documents_new_protection_removes_its_pdfa_claim_and_is_refused_while_read_only() {
    let dir = out_dir("protect-session-pdfa");
    let level = PdfA {
        part: 1,
        conformance: Some('B'),
    };
    let path = sample_file(
        &dir,
        "pdfa.pdf",
        SampleSpec {
            pdfa: Some(level),
            ..SampleSpec::default()
        },
    );
    let (session, _) = Session::open_with(
        &path,
        None,
        OpenOptions {
            pdfa_read_only: true,
        },
    )
    .unwrap();
    assert!(matches!(
        session.set_protection(Some(ProtectionChange::Set(settings(Some("user"), None)))),
        Err(Error::ReadOnly)
    ));
    session.enable_editing().unwrap();
    session
        .set_protection(Some(ProtectionChange::Set(settings(Some("user"), None))))
        .unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.pdfa_removed, Some(level));
    let mut doc = open_pdf_shared(&path).unwrap();
    assert!(doc.authenticate("user").unwrap());
    assert_eq!(docinfo::read_pdfa(&doc), None);
}
