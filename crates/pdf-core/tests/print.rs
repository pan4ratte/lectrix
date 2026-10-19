//! Printing (section 6.12): a job through the PDF target, as CUPS gets it on Linux and
//! macOS: one page a sheet, the size of the paper, with the document's page placed on it.

mod common;

use common::{out_dir, qpdf_check, sample_file, writing};
use mupdf::{Document, TextPageFlags};
use pdf_core::Error;
use pdf_core::print::pdf::PdfTarget;
use pdf_core::print::{Duplex, Job, Orientation, Sheet, Sizing, print};
use pdf_core::session::Session;
use pdf_core::testgen::SampleSpec;

fn job(pages: Vec<usize>) -> Job {
    Job {
        pages,
        sizing: Sizing::Fit,
        orientation: Orientation::Auto,
        annotations: true,
        grey: false,
        copies: 1,
        collate: true,
        duplex: Duplex::OneSided,
    }
}

fn text(doc: &Document, page: i32) -> String {
    doc.load_page(page)
        .unwrap()
        .to_text_page(TextPageFlags::empty())
        .unwrap()
        .to_text()
        .unwrap()
}

#[test]
fn a_job_becomes_a_pdf_of_its_sheets() {
    let dir = out_dir("print-pdf");
    let source = sample_file(&dir, "source.pdf", SampleSpec::default());
    let (session, _) = Session::open(&source, None).unwrap();
    let out = dir.join("printed.pdf");
    let _ = std::fs::remove_file(&out);
    let sheet = Sheet::with_margin(595.0, 842.0, 12.0);
    let mut job = job(vec![2, 0]);
    job.copies = 2;
    job.duplex = Duplex::LongEdge;
    let mut seen = Vec::new();
    writing(|| {
        print(
            &session,
            &job,
            Box::new(PdfTarget::new(&out, sheet).unwrap()),
            &mut |done, total| {
                seen.push((done, total));
                true
            },
        )
    })
    .unwrap();
    session.close();
    // CUPS makes the copies: one sheet a page, each the size of the paper.
    assert_eq!(seen.last(), Some(&(2, 2)));
    qpdf_check(&out);
    let printed = Document::open(out.to_str().unwrap()).unwrap();
    assert_eq!(printed.page_count().unwrap(), 2);
    let bounds = printed.load_page(0).unwrap().bounds().unwrap();
    assert!((bounds.width() - 595.0).abs() < 0.5, "{bounds:?}");
    assert!((bounds.height() - 842.0).abs() < 0.5, "{bounds:?}");
    // In the order asked: the third page first. The text stays text.
    let source_doc = Document::open(source.to_str().unwrap()).unwrap();
    let first = text(&printed, 0);
    assert!(!first.trim().is_empty());
    assert_eq!(first.trim(), text(&source_doc, 2).trim());
    assert_eq!(text(&printed, 1).trim(), text(&source_doc, 0).trim());
}

#[test]
fn a_stopped_job_leaves_no_file() {
    let dir = out_dir("print-stop");
    let source = sample_file(&dir, "source.pdf", SampleSpec::default());
    let (session, _) = Session::open(&source, None).unwrap();
    let out = dir.join("printed.pdf");
    let _ = std::fs::remove_file(&out);
    let target = PdfTarget::new(&out, Sheet::with_margin(595.0, 842.0, 0.0)).unwrap();
    let result = writing(|| {
        print(&session, &job(vec![0, 1]), Box::new(target), &mut |_, _| {
            false
        })
    });
    session.close();
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(!out.exists());
}
