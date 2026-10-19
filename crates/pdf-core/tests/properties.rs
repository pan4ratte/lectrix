//! A document's properties (section 6.10): what its file declares, its description changed
//! as one undo step in the information dictionary and the XMP alike, and its fonts.

mod common;

use common::{out_dir, qpdf_check, sample_file, writing};
use pdf_core::docinfo::{self, PdfA};
use pdf_core::ffi::open_pdf_shared;
use pdf_core::ops::Operation;
use pdf_core::properties::{self, Description, FontKind};
use pdf_core::save::SaveKind;
use pdf_core::session::Session;
use pdf_core::testgen::SampleSpec;

fn describe(title: &str, author: &str, subject: &str, keywords: &str) -> Operation {
    Operation::SetDescription {
        description: Description {
            title: title.into(),
            author: author.into(),
            subject: subject.into(),
            keywords: keywords.into(),
        },
    }
}

#[test]
fn reads_what_the_file_declares() {
    let dir = out_dir("properties-read");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let (session, _) = Session::open(&path, None).unwrap();
    let p = session.properties().unwrap();
    assert_eq!(p.description.title, "Lectrix sample");
    assert_eq!(p.description.author, "");
    assert_eq!(p.producer.as_deref(), Some("Lectrix testgen"));
    assert_eq!(p.creator, None);
    assert!(p.version.starts_with("1."), "{}", p.version);
    assert!(!p.tagged);
    assert_eq!(p.signatures, 0);
}

#[test]
fn the_description_is_one_undo_step_written_by_the_next_save() {
    let dir = out_dir("properties-write");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let (session, _) = Session::open(&path, None).unwrap();

    let change = session
        .apply(describe(
            "Annual report",
            "Zoë Ångström",
            "Figures",
            "money, 2026",
        ))
        .unwrap();
    assert!(change.state.dirty);
    assert_eq!(change.state.undo_name.as_deref(), Some("Change properties"));
    assert_eq!(
        session.properties().unwrap().description.author,
        "Zoë Ångström"
    );

    session.undo().unwrap();
    assert_eq!(
        session.properties().unwrap().description.title,
        "Lectrix sample"
    );
    session.redo().unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    qpdf_check(&path);

    let doc = open_pdf_shared(&path).unwrap();
    let p = properties::read(&doc).unwrap();
    assert_eq!(
        p.description,
        Description {
            title: "Annual report".into(),
            author: "Zoë Ångström".into(),
            subject: "Figures".into(),
            keywords: "money, 2026".into(),
        }
    );
    // An empty value takes the entry out.
    session.apply(describe("", "", "", "")).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    let doc = open_pdf_shared(&path).unwrap();
    let info = doc.trailer().unwrap().get_dict("Info").unwrap().unwrap();
    assert!(info.get_dict("Title").unwrap().is_none());
    assert!(info.get_dict("Author").unwrap().is_none());
    // What the description does not cover stays.
    assert!(info.get_dict("Producer").unwrap().is_some());
}

#[test]
fn a_pdfa_files_xmp_follows_and_its_claim_stays() {
    let dir = out_dir("properties-pdfa");
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
    let (session, _) = Session::open(&path, None).unwrap();
    session
        .apply(describe("Title & more", "Ann", "", "x, y"))
        .unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(saved.pdfa_removed, None);
    qpdf_check(&path);

    let doc = open_pdf_shared(&path).unwrap();
    assert_eq!(docinfo::read_pdfa(&doc), Some(level));
    let xmp = String::from_utf8(docinfo::metadata_bytes(&doc).unwrap()).unwrap();
    assert!(xmp.contains("Title &amp; more"), "{xmp}");
    assert!(xmp.contains("<rdf:li>Ann</rdf:li>"), "{xmp}");
    assert!(xmp.contains("x, y"), "{xmp}");
    assert!(xmp.contains("MetadataDate"), "{xmp}");
}

#[test]
fn a_file_without_an_information_dictionary_gets_one() {
    let dir = out_dir("properties-no-info");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    // The sample with its information dictionary taken out.
    let (session, _) = Session::open(&path, None).unwrap();
    drop(session);
    let doc = open_pdf_shared(&path).unwrap();
    doc.trailer().unwrap().dict_delete("Info").unwrap();
    let bare = dir.join("bare.pdf");
    writing(|| pdf_core::save::save_atomic(&doc, SaveKind::Full, None, &bare)).unwrap();

    let (session, _) = Session::open(&bare, None).unwrap();
    assert_eq!(session.properties().unwrap().description.title, "");
    session.apply(describe("Found", "", "", "")).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    qpdf_check(&bare);
    let doc = open_pdf_shared(&bare).unwrap();
    assert_eq!(properties::read(&doc).unwrap().description.title, "Found");
}

#[test]
fn fonts_are_listed_once_each() {
    let dir = out_dir("properties-fonts");
    let path = sample_file(
        &dir,
        "doc.pdf",
        SampleSpec {
            pages: 4,
            ..SampleSpec::default()
        },
    );
    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.pages.len(), 4);
    let fonts = session.fonts(0..4).unwrap();
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    let font = &fonts[0];
    assert_eq!(font.name, "Helvetica");
    assert_eq!(font.kind, FontKind::Type1);
    assert!(!font.embedded && font.standard && !font.subset);
    assert_eq!(font.first_page, 0);
    // A range past the end is cut to the pages there are.
    assert_eq!(session.fonts(2..100).unwrap().len(), 1);
    assert!(session.fonts(10..20).unwrap().is_empty());
}
