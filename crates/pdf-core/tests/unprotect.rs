//! Removing a file's protection (the start screen's Unlock, ADR 0025): the password and
//! the restrictions go, the content stays, and the original is only read.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{open, out_dir, qpdf_check, writing};
use mupdf::TextPageFlags;
use mupdf::pdf::{Encryption, PdfWriteOptions, Permission};
use pdf_core::Error;
use pdf_core::testgen::{MARKER, SampleSpec, sample_document};
use pdf_core::unprotect::{Protection, inspect, remove_protection};

/// A three-page sample encrypted with AES-256 that allows annotating only (no printing,
/// no copying, no changes).
fn protected(dir: &Path, user: &str) -> PathBuf {
    let path = dir.join("protected.pdf");
    let doc = sample_document(&SampleSpec {
        pages: 3,
        ..SampleSpec::default()
    })
    .unwrap();
    let mut options = PdfWriteOptions::default();
    options
        .set_encryption(Encryption::Aes256)
        .set_permissions(Permission::ANNOTATE)
        .set_user_password(user)
        .set_owner_password("owner");
    writing(|| doc.save_with_options(path.to_str().unwrap(), options)).unwrap();
    path
}

/// Asserts `path` opens without a password, with no encryption and nothing forbidden, and
/// still has its three pages of text.
fn assert_unprotected(path: &Path) {
    qpdf_check(path);
    let doc = open(path);
    assert!(!doc.needs_password().unwrap());
    assert!(
        doc.trailer()
            .unwrap()
            .get_dict("Encrypt")
            .unwrap()
            .is_none()
    );
    assert!(doc.permissions().contains(Permission::all()));
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

#[test]
fn the_open_password_removes_the_password_and_the_restrictions() {
    let dir = out_dir("unprotect-password");
    let path = protected(&dir, "open sesame");
    let target = dir.join("unlocked.pdf");

    assert!(matches!(inspect(&path, None), Err(Error::PasswordRequired)));
    assert!(matches!(
        inspect(&path, Some("wrong")),
        Err(Error::WrongPassword)
    ));
    assert_eq!(
        inspect(&path, Some("open sesame")).unwrap(),
        Protection {
            encrypted: true,
            signed: false
        }
    );

    let original = fs::read(&path).unwrap();
    writing(|| remove_protection(&path, Some("open sesame"), &target)).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        original,
        "the original is only read"
    );
    assert_unprotected(&target);
    assert!(!inspect(&target, None).unwrap().encrypted);
}

#[test]
fn the_permissions_password_removes_it_too() {
    let dir = out_dir("unprotect-owner");
    let path = protected(&dir, "open sesame");
    let target = dir.join("unlocked.pdf");

    writing(|| remove_protection(&path, Some("owner"), &target)).unwrap();
    assert_unprotected(&target);
}

#[test]
fn restrictions_alone_need_no_password() {
    let dir = out_dir("unprotect-restrictions");
    let path = protected(&dir, "");
    let target = dir.join("unlocked.pdf");

    assert!(inspect(&path, None).unwrap().encrypted);
    writing(|| remove_protection(&path, None, &target)).unwrap();
    assert_unprotected(&target);
}

#[test]
fn the_original_can_be_replaced() {
    let dir = out_dir("unprotect-in-place");
    let path = protected(&dir, "open sesame");

    writing(|| remove_protection(&path, Some("open sesame"), &path)).unwrap();
    assert_unprotected(&path);
}

#[test]
fn a_wrong_password_writes_nothing() {
    let dir = out_dir("unprotect-wrong");
    let path = protected(&dir, "open sesame");
    let target = dir.join("unlocked.pdf");
    let _ = fs::remove_file(&target);

    assert!(matches!(
        remove_protection(&path, Some("wrong"), &target),
        Err(Error::WrongPassword)
    ));
    assert!(!target.exists());
}

#[test]
fn a_file_without_protection_says_so() {
    let dir = out_dir("unprotect-plain");
    let path = common::sample_file(&dir, "plain.pdf", SampleSpec::default());

    assert_eq!(
        inspect(&path, None).unwrap(),
        Protection {
            encrypted: false,
            signed: false
        }
    );
}
