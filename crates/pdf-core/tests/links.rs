//! Links on a page (AGENTS.md section 6.1): where each is in view space, and what it
//! does, read as a bookmark's target is.

mod common;

use common::{assemble, open, out_dir};
use pdf_core::Error;
use pdf_core::links::{PageLink, page_links};
use pdf_core::outline::Target;
use pdf_core::session::Session;

/// A PDF written by hand: three pages (the second turned a quarter), links of every kind
/// on the first, and annotations that are no links for the reader to click.
fn fixture() -> Vec<u8> {
    let objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 3 0 R >> >>".into(),
        "<< /Type /Pages /Kids [4 0 R 5 0 R 6 0 R] /Count 3 /MediaBox [0 0 612 792] >>".into(),
        "<< /Names [(chap) [6 0 R /FitH 500]] >>".into(),
        // 4: the first page
        "<< /Type /Page /Parent 2 0 R /Resources << >> /Annots [7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 15 0 R 16 0 R] >>".into(),
        // 5: turned a quarter clockwise
        "<< /Type /Page /Parent 2 0 R /Resources << >> /Rotate 90 /Annots [14 0 R] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Resources << >> >>".into(),
        // 7: an explicit destination
        "<< /Type /Annot /Subtype /Link /Rect [72 700 300 720] /Border [0 0 0] /Dest [6 0 R /XYZ 0 792 null] >>".into(),
        // 8: a GoTo action to a named destination
        "<< /Type /Annot /Subtype /Link /Rect [72 660 300 680] /A << /S /GoTo /D (chap) >> >>".into(),
        // 9: a web address
        "<< /Type /Annot /Subtype /Link /Rect [72 620 300 640] /A << /S /URI /URI (https://example.org/a) >> >>".into(),
        // 10: corners the other way round
        "<< /Type /Annot /Subtype /Link /Rect [300 600 72 580] /Dest [5 0 R /Fit] >>".into(),
        // 11: hidden
        "<< /Type /Annot /Subtype /Link /Rect [72 540 300 560] /F 2 /Dest [5 0 R /Fit] >>".into(),
        // 12: leads nowhere
        "<< /Type /Annot /Subtype /Link /Rect [72 500 300 520] >>".into(),
        // 13: not a link
        "<< /Type /Annot /Subtype /Text /Rect [72 460 92 480] /Contents (note) /Dest [5 0 R /Fit] >>".into(),
        // 14: on the turned page
        "<< /Type /Annot /Subtype /Link /Rect [100 100 200 150] /Dest [4 0 R /XYZ null null null] >>".into(),
        // 15: off the page
        "<< /Type /Annot /Subtype /Link /Rect [-90 -90 -10 -10] /Dest [5 0 R /Fit] >>".into(),
        // 16: half off the page, to a page that is none of this document's
        "<< /Type /Annot /Subtype /Link /Rect [500 20 700 40] /Dest [3 0 R /Fit] >>".into(),
    ];
    assemble(&objects)
}

fn page(page: usize, x: Option<f32>, y: Option<f32>, named: Option<&str>) -> Target {
    Target::Page {
        page,
        x,
        y,
        named: named.map(str::to_owned),
    }
}

#[test]
fn links_are_read_with_their_place_and_target() {
    let dir = out_dir("links");
    let path = dir.join("links.pdf");
    std::fs::write(&path, fixture()).unwrap();
    let doc = open(&path);

    assert_eq!(
        page_links(&doc, 0).unwrap(),
        vec![
            PageLink {
                rect: [72.0, 72.0, 300.0, 92.0],
                target: page(2, Some(0.0), Some(0.0), None),
            },
            PageLink {
                rect: [72.0, 112.0, 300.0, 132.0],
                target: page(2, None, Some(292.0), Some("chap")),
            },
            PageLink {
                rect: [72.0, 152.0, 300.0, 172.0],
                target: Target::Uri("https://example.org/a".into()),
            },
            PageLink {
                rect: [72.0, 192.0, 300.0, 212.0],
                target: page(1, None, None, None),
            },
            // Kept to the page; its destination is not a page.
            PageLink {
                rect: [500.0, 752.0, 612.0, 772.0],
                target: Target::Broken { named: None },
            },
        ]
    );

    // On a turned page, the place is where the link shows.
    let turned = page_links(&doc, 1).unwrap();
    assert_eq!(turned.len(), 1);
    assert_eq!(turned[0].rect, [100.0, 100.0, 150.0, 200.0]);
    assert_eq!(turned[0].target, page(0, None, None, None));

    assert!(page_links(&doc, 2).unwrap().is_empty());
    assert!(matches!(page_links(&doc, 3), Err(Error::PageOutOfRange(3))));
}

#[test]
fn push_buttons_are_links_and_other_fields_are_not() {
    let objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 9 0 R 10 0 R 11 0 R] >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612 792] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Resources << >> /Annots [5 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Resources << >> >>".into(),
        // 5: a push button whose widget is its field, with a GoTo action
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (next) /Rect [72 700 300 720] /A << /S /GoTo /D [4 0 R /Fit] >> >>".into(),
        // 6: a push button field with two widgets, which inherit its type and flags
        "<< /FT /Btn /Ff 65536 /T (contents) /Kids [7 0 R 8 0 R] >>".into(),
        // 7: with a mouse-up action
        "<< /Type /Annot /Subtype /Widget /Parent 6 0 R /Rect [72 660 300 680] /AA << /U << /S /Launch /F (../Start.pdf) >> >> >>".into(),
        // 8: with no action
        "<< /Type /Annot /Subtype /Widget /Parent 6 0 R /Rect [72 620 300 640] >>".into(),
        // 9: a check box with an action is filled in, not followed
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (box) /Rect [72 580 92 600] /A << /S /GoTo /D [4 0 R /Fit] >> >>".into(),
        // 10: a text field
        "<< /Type /Annot /Subtype /Widget /FT /Tx /Ff 65536 /T (name) /Rect [72 540 300 560] /A << /S /GoTo /D [4 0 R /Fit] >> >>".into(),
        // 11: a hidden push button
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /F 2 /T (hidden) /Rect [72 500 300 520] /A << /S /GoTo /D [4 0 R /Fit] >> >>".into(),
    ];
    let dir = out_dir("links-buttons");
    let path = dir.join("buttons.pdf");
    std::fs::write(&path, assemble(&objects)).unwrap();
    let doc = open(&path);

    assert_eq!(
        page_links(&doc, 0).unwrap(),
        vec![
            PageLink {
                rect: [72.0, 72.0, 300.0, 92.0],
                target: page(1, None, None, None),
            },
            PageLink {
                rect: [72.0, 112.0, 300.0, 132.0],
                target: Target::File("../Start.pdf".into()),
            },
        ]
    );
}

#[test]
fn a_session_reads_links_with_the_revision() {
    let dir = out_dir("links-session");
    let path = dir.join("links.pdf");
    std::fs::write(&path, fixture()).unwrap();
    let (session, info) = Session::open(&path, None).unwrap();
    let (links, revision) = session.page_links(0).unwrap();
    assert_eq!(links.len(), 5);
    assert_eq!(revision, info.state.revision);
    assert!(matches!(
        session.page_links(9),
        Err(Error::PageOutOfRange(9))
    ));
    session.close();
}
