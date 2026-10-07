//! Document sessions: operations with undo/redo and dirty state, saving in place while the
//! file is open, Save As, encrypted and repaired files, reload, text and search.

mod common;

use std::fs;
use std::path::Path;

use common::{open, out_dir, qpdf_check, sample_file, writing};
use mupdf::pdf::{Encryption, PdfWriteOptions};
use pdf_core::Error;
use pdf_core::geometry::{PageGeometry, read_page_boxes};
use pdf_core::labels::{LabelRule, LabelStyle, write_rules};
use pdf_core::ops::Operation;
use pdf_core::save::SaveKind;
use pdf_core::session::{PageSize, Session};
use pdf_core::testgen::{MARKER, SampleSpec, sample_document};

fn rotate(pages: Vec<usize>, degrees: i32) -> Operation {
    Operation::RotatePages { pages, degrees }
}

fn rotation_on_disk(path: &Path, page: i32) -> i32 {
    let doc = open(path);
    PageGeometry::new(&read_page_boxes(&doc.find_page(page).unwrap()).unwrap()).rotation
}

#[test]
fn open_reports_pages_flags_and_clean_state() {
    let dir = out_dir("session-open");
    let path = sample_file(
        &dir,
        "doc.pdf",
        SampleSpec {
            pages: 4,
            ..SampleSpec::default()
        },
    );
    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.path, path);
    assert_eq!(info.pages.len(), 4);
    assert_eq!(
        info.pages[0],
        PageSize {
            width: 612.0,
            height: 792.0,
            rotation: 0
        }
    );
    assert!(info.labels.is_none());
    assert!(info.flags.can_assemble && info.flags.can_copy && !info.flags.signed);
    assert_eq!(info.state.revision, 0);
    assert!(!info.state.dirty);
    assert_eq!(info.state.undo_name, None);
    session.close();
}

#[test]
fn rotate_undo_redo_tracks_revision_and_dirty_state() {
    let dir = out_dir("session-undo");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let (session, _) = Session::open(&path, None).unwrap();

    let change = session.apply(rotate(vec![1], 90)).unwrap();
    assert_eq!(
        change.changed_pages,
        vec![(
            1,
            PageSize {
                width: 792.0,
                height: 612.0,
                rotation: 90
            }
        )]
    );
    assert_eq!(change.state.revision, 1);
    assert!(change.state.dirty);
    assert_eq!(change.state.undo_name.as_deref(), Some("Rotate page"));
    assert_eq!(change.labels, None);

    let undone = session.undo().unwrap();
    assert_eq!(undone.state.revision, 0);
    assert!(!undone.state.dirty, "back at the opened state");
    assert_eq!(undone.state.redo_name.as_deref(), Some("Rotate page"));
    assert_eq!(undone.changed_pages[0].1.width, 612.0);
    assert!(matches!(session.undo(), Err(Error::NothingToUndo)));

    let redone = session.redo().unwrap();
    assert_eq!(redone.state.revision, 1, "redo returns to the same content");
    assert!(redone.state.dirty);
    assert!(matches!(session.redo(), Err(Error::NothingToRedo)));

    // A new step after an undo gets a new revision and drops the redo branch.
    session.undo().unwrap();
    let branched = session.apply(rotate(vec![0, 2], 180)).unwrap();
    assert_eq!(branched.state.revision, 2);
    assert_eq!(branched.state.undo_name.as_deref(), Some("Rotate pages"));
    assert_eq!(branched.state.redo_name, None);
    // 180 degrees keeps the size, but the view needs the new rotation.
    assert_eq!(
        branched.changed_pages,
        vec![
            (
                0,
                PageSize {
                    width: 612.0,
                    height: 792.0,
                    rotation: 180
                }
            ),
            (
                2,
                PageSize {
                    width: 612.0,
                    height: 792.0,
                    rotation: 180
                }
            )
        ]
    );
}

#[test]
fn invalid_operations_change_nothing() {
    let dir = out_dir("session-invalid");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let (session, _) = Session::open(&path, None).unwrap();
    assert!(session.apply(rotate(vec![7], 90)).is_err());
    assert!(session.apply(rotate(vec![0], 30)).is_err());
    let state = session.info().unwrap().state;
    assert_eq!(state.revision, 0);
    assert!(!state.dirty);
}

#[test]
fn saves_in_place_repeatedly_while_open() {
    let dir = out_dir("session-save-in-place");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let original = fs::read(&path).unwrap();
    let (session, _) = Session::open(&path, None).unwrap();

    session.apply(rotate(vec![0], 90)).unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert!(!saved.outcome.fell_back_to_full);
    assert!(!saved.state.dirty);
    assert_eq!(
        saved.state.undo_name, None,
        "history starts again after a save"
    );
    assert_eq!(saved.path, path);
    let after_first = fs::read(&path).unwrap();
    assert!(
        after_first.starts_with(&original),
        "incremental: original bytes kept"
    );
    assert_eq!(rotation_on_disk(&path, 0), 90);

    // A second incremental save must append to the saved file, not the original.
    session.apply(rotate(vec![1], 270)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    let after_second = fs::read(&path).unwrap();
    assert!(after_second.starts_with(&after_first));
    assert_eq!(rotation_on_disk(&path, 0), 90);
    assert_eq!(rotation_on_disk(&path, 1), 270);
    qpdf_check(&path);

    // Undo after the save works on the reopened document.
    session.apply(rotate(vec![2], 90)).unwrap();
    session.undo().unwrap();
    assert!(!session.info().unwrap().state.dirty);
    session.close();
}

#[test]
fn save_as_moves_the_session_and_leaves_the_source_alone() {
    let dir = out_dir("session-save-as");
    let path = sample_file(&dir, "source.pdf", SampleSpec::default());
    let original = fs::read(&path).unwrap();
    let target = dir.join("copy.pdf");
    let _ = fs::remove_file(&target);

    let (session, _) = Session::open(&path, None).unwrap();
    session.apply(rotate(vec![0], 90)).unwrap();
    let saved = writing(|| session.save(SaveKind::Optimized, Some(target.clone()))).unwrap();
    assert_eq!(saved.path, target);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(rotation_on_disk(&target, 0), 90);
    qpdf_check(&target);
    assert_eq!(session.info().unwrap().path, target);

    // Later saves go to the new file.
    session.apply(rotate(vec![1], 90)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert_eq!(rotation_on_disk(&target, 1), 90);
    assert_eq!(fs::read(&path).unwrap(), original);
}

fn encrypted_file(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("encrypted.pdf");
    let doc = sample_document(&SampleSpec::default()).unwrap();
    let mut options = PdfWriteOptions::default();
    options
        .set_encryption(Encryption::Aes256)
        .set_user_password("open sesame")
        .set_owner_password("owner");
    writing(|| doc.save_with_options(path.to_str().unwrap(), options)).unwrap();
    path
}

#[test]
fn encrypted_documents_need_the_password_and_stay_encrypted() {
    let dir = out_dir("session-encrypted");
    let path = encrypted_file(&dir);

    assert!(matches!(
        Session::open(&path, None),
        Err(Error::PasswordRequired)
    ));
    assert!(matches!(
        Session::open(&path, Some("wrong")),
        Err(Error::WrongPassword)
    ));
    let (session, info) = Session::open(&path, Some("open sesame")).unwrap();
    assert!(info.flags.encrypted);

    session.apply(rotate(vec![0], 90)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    // Still encrypted with the same password, and the session reopened it fine.
    assert!(matches!(
        Session::open(&path, None),
        Err(Error::PasswordRequired)
    ));
    let (check, _) = Session::open(&path, Some("open sesame")).unwrap();
    assert_eq!(check.info().unwrap().pages[0].width, 792.0);
    session.apply(rotate(vec![1], 90)).unwrap();
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
}

#[test]
fn repaired_file_falls_back_to_a_full_save() {
    let dir = out_dir("session-repaired");
    let good = sample_file(&dir, "good.pdf", SampleSpec::default());
    // Break the cross-reference offset so MuPDF must repair the file on open.
    let mut bytes = fs::read(&good).unwrap();
    let pos = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
    let digits_start = pos + 10;
    for b in bytes[digits_start..].iter_mut() {
        if b.is_ascii_digit() {
            *b = b'9';
        } else if *b == b'%' {
            break;
        }
    }
    let path = dir.join("damaged.pdf");
    fs::write(&path, &bytes).unwrap();

    let (session, info) = Session::open(&path, None).unwrap();
    assert!(info.flags.repaired);
    session.apply(rotate(vec![0], 90)).unwrap();
    let saved = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert!(saved.outcome.fell_back_to_full);
    assert_eq!(saved.outcome.kind, SaveKind::Full);
    qpdf_check(&path);
    assert_eq!(rotation_on_disk(&path, 0), 90);
    // The reopened file is healthy now, so the next save can be incremental.
    session.apply(rotate(vec![1], 90)).unwrap();
    let again = writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    assert!(!again.outcome.fell_back_to_full);
}

#[test]
fn reload_picks_up_external_changes() {
    let dir = out_dir("session-reload");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.pages.len(), 3);

    // Another program replaces the file while it is open.
    let replacement = sample_file(
        &dir,
        "replacement.pdf",
        SampleSpec {
            pages: 6,
            ..SampleSpec::default()
        },
    );
    fs::copy(&replacement, dir.join("staging.tmp")).unwrap();
    fs::rename(dir.join("staging.tmp"), &path).unwrap();

    let reloaded = session.reload().unwrap();
    assert_eq!(reloaded.pages.len(), 6);
    assert!(!reloaded.state.dirty);
    assert!(
        reloaded.state.revision > info.state.revision,
        "new content gets a new revision"
    );
}

#[test]
fn reports_labels_per_page() {
    let dir = out_dir("session-labels");
    let path = dir.join("labels.pdf");
    let mut doc = sample_document(&SampleSpec {
        pages: 5,
        ..SampleSpec::default()
    })
    .unwrap();
    write_rules(
        &mut doc,
        vec![
            LabelRule {
                start_page: 0,
                style: LabelStyle::LowerRoman,
                prefix: String::new(),
                first_number: 1,
            },
            LabelRule::decimal_from_one(2),
        ],
    )
    .unwrap();
    writing(|| pdf_core::save::save_atomic(&doc, SaveKind::Full, None, &path)).unwrap();
    let (_, info) = Session::open(&path, None).unwrap();
    assert_eq!(
        info.labels.unwrap().labels,
        vec!["i", "ii", "1", "2", "3"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn text_render_and_search_through_the_session() {
    let dir = out_dir("session-text");
    let path = sample_file(
        &dir,
        "doc.pdf",
        SampleSpec {
            pages: 5,
            ..SampleSpec::default()
        },
    );
    let (session, _) = Session::open(&path, None).unwrap();

    let (text, revision) = session.page_text(2).unwrap();
    assert_eq!(revision, 0);
    assert_eq!(text.lines[0].text, "Page 3");

    let (image, _) = session.render_rgba(0, 1.0, None).unwrap();
    assert_eq!((image.width, image.height), (612, 792));

    let (hits, _) = session.search(MARKER, 0..5).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.page).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4]
    );
    // A range running past the last page stops at the last page.
    let (hits, _) = session.search(MARKER, 3..64).unwrap();
    assert_eq!(hits.iter().map(|h| h.page).collect::<Vec<_>>(), vec![3, 4]);
    let (hits, _) = session.search("on page 4.", 0..5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page, 3);

    // Rendering reflects operations at once (the display list cache is cleared).
    session.apply(rotate(vec![0], 90)).unwrap();
    let (image, revision) = session.render_rgba(0, 1.0, None).unwrap();
    assert_eq!((image.width, image.height, revision), (792, 612, 1));
}

fn labels_of(info: &Option<pdf_core::session::PageLabels>) -> Option<Vec<String>> {
    info.as_ref().map(|l| l.labels.clone())
}

#[test]
fn page_labels_are_one_undo_step_each_and_survive_saving() {
    let dir = out_dir("session-set-labels");
    let path = sample_file(
        &dir,
        "doc.pdf",
        SampleSpec {
            pages: 6,
            ..SampleSpec::default()
        },
    );
    let (session, info) = Session::open(&path, None).unwrap();
    assert!(info.labels.is_none());

    // "Roman front matter, then arabic from page 3."
    let rules = vec![
        LabelRule {
            style: LabelStyle::LowerRoman,
            ..LabelRule::decimal_from_one(0)
        },
        LabelRule::decimal_from_one(2),
    ];
    let change = session
        .apply(Operation::SetPageLabels {
            rules: rules.clone(),
        })
        .unwrap();
    let labels = change.labels.clone().expect("labels changed");
    assert_eq!(labels.as_ref().unwrap().rules, rules);
    assert_eq!(labels_of(&labels).unwrap(), ["i", "ii", "1", "2", "3", "4"]);
    assert_eq!(
        change.state.undo_name.as_deref(),
        Some("Change page labels")
    );
    assert!(change.state.dirty);

    // Committing the same rules again (in any order) is not a step.
    let same = session
        .apply(Operation::SetPageLabels {
            rules: rules.iter().rev().cloned().collect(),
        })
        .unwrap();
    assert_eq!(same.state.revision, change.state.revision);
    assert_eq!(same.labels, None);

    // A prefixed appendix, then undo and redo it.
    let mut with_appendix = rules.clone();
    with_appendix.push(LabelRule {
        start_page: 4,
        style: LabelStyle::UpperLetters,
        prefix: "Anhang ".into(),
        first_number: 1,
    });
    let appendix = session
        .apply(Operation::SetPageLabels {
            rules: with_appendix.clone(),
        })
        .unwrap();
    assert_eq!(
        labels_of(&appendix.labels.unwrap()).unwrap(),
        ["i", "ii", "1", "2", "Anhang A", "Anhang B"]
    );
    let undone = session.undo().unwrap();
    assert_eq!(undone.state.revision, change.state.revision);
    assert_eq!(undone.labels.unwrap().unwrap().rules, rules);
    let redone = session.redo().unwrap();
    assert_eq!(redone.labels.unwrap().unwrap().rules, with_appendix);

    // Saved and reopened: the rules exactly as written.
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    session.close();
    qpdf_check(&path);
    let (session, info) = Session::open(&path, None).unwrap();
    assert_eq!(info.labels.as_ref().unwrap().rules, with_appendix);

    // "Remove all labels" is its own named step.
    let removed = session
        .apply(Operation::SetPageLabels { rules: Vec::new() })
        .unwrap();
    assert_eq!(removed.labels, Some(None));
    assert_eq!(
        removed.state.undo_name.as_deref(),
        Some("Remove page labels")
    );
    writing(|| session.save(SaveKind::Incremental, None)).unwrap();
    session.close();
    let (_, info) = Session::open(&path, None).unwrap();
    assert!(info.labels.is_none());
}

#[test]
fn invalid_label_rules_change_nothing() {
    let dir = out_dir("session-invalid-labels");
    let path = sample_file(&dir, "doc.pdf", SampleSpec::default());
    let (session, _) = Session::open(&path, None).unwrap();
    let invalid = [
        vec![LabelRule::decimal_from_one(3)],
        vec![
            LabelRule::decimal_from_one(1),
            LabelRule::decimal_from_one(1),
        ],
        vec![LabelRule {
            first_number: 0,
            ..LabelRule::decimal_from_one(0)
        }],
    ];
    for rules in invalid {
        assert!(matches!(
            session.apply(Operation::SetPageLabels { rules }),
            Err(Error::InvalidArgument(_))
        ));
    }
    let state = session.info().unwrap().state;
    assert_eq!(state.revision, 0);
    assert_eq!(state.undo_name, None);
}
