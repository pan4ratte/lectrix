//! Real-world files from the local corpus (AGENTS.md section 9): `tests/local-corpus/`,
//! git-ignored, built by `tests/local-corpus/select.py`. Run on demand with
//! `cargo test --release -p pdf-core --test local_corpus -- --ignored --nocapture`;
//! it passes trivially when the corpus is absent (CI).
//!
//! Every file is opened, rendered, read and searched as the viewer does. Then a copy in
//! `target/test-output/` gets one page rotated and is saved in place, and the reopened
//! copy must keep everything the user did not touch: page count, labels (their number tree
//! is not even written again), bookmarks and every annotation made by other apps (section
//! 5.1 rule 10). Labels are also checked against MuPDF's own reading of them. The corpus
//! files themselves are only read.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use common::{objects_written_after, out_dir, qpdf_severity, writing};
use mupdf::pdf::{PdfDocument, PdfObject};
use pdf_core::Error;
use pdf_core::geometry::{PageGeometry, read_page_boxes};
use pdf_core::labels::{LabelRule, LabelStyle, labels_for_pages, normalize_rules, read_rules};
use pdf_core::ops::Operation;
use pdf_core::outline;
use pdf_core::save::SaveKind;
use pdf_core::session::Session;
use serde_json::Value;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/local-corpus")
}

/// Annotation count per page, by reading the page dictionaries.
fn annotation_counts(doc: &PdfDocument) -> Vec<usize> {
    let count = doc.page_count().unwrap();
    (0..count)
        .map(|i| {
            let page = doc.find_page(i).unwrap();
            match page.get_dict("Annots").unwrap() {
                Some(a) if a.is_array().unwrap() => a.len().unwrap(),
                _ => 0,
            }
        })
        .collect()
}

fn outline_size(items: &[outline::ReadOutlineItem]) -> usize {
    items.iter().map(|i| 1 + outline_size(&i.children)).sum()
}

/// Object numbers of the `/PageLabels` number tree (root and `/Kids`, where indirect).
fn label_tree_objects(doc: &PdfDocument) -> BTreeSet<u32> {
    fn walk(node: &PdfObject, out: &mut BTreeSet<u32>, depth: u32) {
        if depth > 32 {
            return;
        }
        if node.is_indirect().unwrap() {
            let num = u32::try_from(node.as_indirect().unwrap()).unwrap();
            if !out.insert(num) {
                return;
            }
        }
        if let Some(kids) = node.get_dict("Kids").unwrap() {
            for i in 0..kids.len().unwrap() as i32 {
                if let Some(kid) = kids.get_array(i).unwrap() {
                    walk(&kid, out, depth + 1);
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    if let Some(tree) = doc.catalog().unwrap().get_dict("PageLabels").unwrap() {
        walk(&tree, &mut out, 0);
    }
    out
}

fn rotation(doc: &PdfDocument, page: i32) -> i32 {
    PageGeometry::new(&read_page_boxes(&doc.find_page(page).unwrap()).unwrap()).rotation
}

/// A word of at least four letters from the page's text, to search for.
fn some_word(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_alphabetic())
        .find(|w| w.chars().count() >= 4)
        .map(str::to_owned)
}

fn check_file(file: &Path, survey: &Value, out: &Path) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let fail = |what: &str, e: Error| format!("{what}: {e}");

    // --- view, as the app does ---
    let (session, info) = Session::open(file, None).map_err(|e| fail("open", e))?;
    let pages = info.pages.len();
    if Some(pages as u64) != survey["pages"].as_u64() {
        return Err(format!("{pages} pages, survey says {}", survey["pages"]));
    }
    if let Some(labels) = &info.labels {
        if labels.labels.len() != pages {
            return Err(format!("{} labels for {pages} pages", labels.labels.len()));
        }
        // MuPDF's own reading (fz_page_label) must agree on every page.
        let doc = PdfDocument::open(file.to_str().unwrap()).map_err(|e| format!("{e}"))?;
        for (page, label) in labels.labels.iter().enumerate() {
            let theirs = doc.page_label(page).map_err(|e| format!("{e}"))?;
            if &theirs != label {
                return Err(format!(
                    "page {}: label {label:?}, MuPDF reads {theirs:?}",
                    page + 1
                ));
            }
        }
        notes.push(format!("{} label rules", labels.rules.len()));
    }
    for page in [0, pages / 2, pages - 1] {
        session
            .render_rgba(page, 0.5, None)
            .map_err(|e| fail(&format!("render page {}", page + 1), e))?;
    }
    let (text, _) = session.page_text(0).map_err(|e| fail("text", e))?;
    if let Some(word) = some_word(&text.plain_text()) {
        let (hits, _) = session
            .search(&word, 0..pages.min(5))
            .map_err(|e| fail("search", e))?;
        if !hits.iter().any(|h| h.page == 0) {
            return Err(format!("search for {word:?} found nothing on page 1"));
        }
    }
    session.close();

    // --- edit a copy and save in place ---
    let copy = out.join(file.file_name().unwrap());
    fs::copy(file, &copy).map_err(|e| format!("copy: {e}"))?;
    let before =
        mupdf::pdf::PdfDocument::open(copy.to_str().unwrap()).map_err(|e| format!("{e}"))?;
    let annots_before = annotation_counts(&before);
    let outline_before =
        outline_size(&outline::read_outline(&before).map_err(|e| fail("outline", e))?);
    let rotation_before = rotation(&before, 0);
    let label_objects = label_tree_objects(&before);
    let size_before = fs::metadata(&copy).map_err(|e| format!("{e}"))?.len() as usize;
    drop(before);

    let (session, info) = Session::open(&copy, None).map_err(|e| fail("open copy", e))?;
    let rotate = Operation::RotatePages {
        pages: vec![0],
        degrees: 90,
    };
    if !info.flags.can_assemble {
        match session.apply(rotate) {
            Err(Error::NotPermitted) => notes.push("rotation refused: document permissions".into()),
            other => return Err(format!("expected NotPermitted, got {other:?}")),
        }
        return Ok(notes);
    }
    session.apply(rotate).map_err(|e| fail("rotate", e))?;
    let saved =
        writing(|| session.save(SaveKind::Incremental, None)).map_err(|e| fail("save", e))?;
    if saved.outcome.fell_back_to_full {
        notes.push("saved in full (file had been repaired)".into());
    }
    session.close();
    if !saved.outcome.fell_back_to_full {
        let bytes = fs::read(&copy).map_err(|e| format!("read saved copy: {e}"))?;
        let written = objects_written_after(&bytes, size_before);
        let rewritten: Vec<_> = label_objects.intersection(&written).collect();
        if !rewritten.is_empty() {
            return Err(format!("label tree objects written again: {rewritten:?}"));
        }
    }

    let after = mupdf::pdf::PdfDocument::open(copy.to_str().unwrap())
        .map_err(|e| format!("reopen: {e}"))?;
    if after.page_count().unwrap() as usize != pages {
        return Err("page count changed".into());
    }
    if rotation(&after, 0) != (rotation_before + 90) % 360 {
        return Err("page 1 was not rotated".into());
    }
    if annotation_counts(&after) != annots_before {
        return Err("annotations changed".into());
    }
    if outline_size(&outline::read_outline(&after).map_err(|e| fail("outline after", e))?)
        != outline_before
    {
        return Err("bookmarks changed".into());
    }
    let (reopened_session, reopened) =
        Session::open(&copy, None).map_err(|e| fail("reopen session", e))?;
    if reopened.labels != info.labels {
        return Err("page labels changed".into());
    }
    drop(after);

    // --- edit the labels of a file that has them, as the panel does ---
    if let Some(labels) = &reopened.labels {
        // The panel sends the rules it shows (those within the document) plus the change:
        // here, a prefixed range on the last page.
        let mut rules: Vec<LabelRule> = labels
            .rules
            .iter()
            .filter(|r| r.start_page < pages)
            .cloned()
            .collect();
        rules.retain(|r| r.start_page != pages - 1);
        rules.push(LabelRule {
            start_page: pages - 1,
            style: LabelStyle::UpperLetters,
            prefix: "Folio-".into(),
            first_number: 27,
        });
        let expected = normalize_rules(rules.clone(), pages).map_err(|e| fail("rules", e))?;
        reopened_session
            .apply(Operation::SetPageLabels { rules })
            .map_err(|e| fail("set labels", e))?;
        writing(|| reopened_session.save(SaveKind::Incremental, None))
            .map_err(|e| fail("save labels", e))?;
        reopened_session.close();
        let doc = PdfDocument::open(copy.to_str().unwrap()).map_err(|e| format!("{e}"))?;
        if read_rules(&doc).map_err(|e| fail("read labels", e))? != expected {
            return Err("edited labels did not read back as written".into());
        }
        let last = doc.page_label(pages - 1).map_err(|e| format!("{e}"))?;
        if last != "Folio-AA" || last != labels_for_pages(&expected, pages)[pages - 1] {
            return Err(format!("last page label after the edit: {last:?}"));
        }
        notes.push("label edit ok".into());
    } else {
        reopened_session.close();
    }

    if let (Some(input), Some(output)) = (qpdf_severity(file), qpdf_severity(&copy)) {
        if output > input {
            return Err(format!("qpdf --check got worse: {input} -> {output}"));
        }
        if input > 0 {
            notes.push(format!(
                "qpdf finds problems in the original too (severity {input})"
            ));
        }
    }
    fs::remove_file(&copy).ok();
    Ok(notes)
}

#[test]
#[ignore = "real-world local corpus, minutes long: run with --ignored"]
fn local_corpus_opens_renders_and_saves_safely() {
    let manifest_path = corpus_dir().join("manifest.json");
    let Ok(manifest) = fs::read_to_string(&manifest_path) else {
        eprintln!("no local corpus at {}; skipped", manifest_path.display());
        return;
    };
    let manifest: Vec<Value> = serde_json::from_str(&manifest).unwrap();
    let out = out_dir("local-corpus");
    let mut failures = Vec::new();
    for entry in &manifest {
        let name = entry["file"].as_str().unwrap();
        let file = corpus_dir().join("files").join(name);
        let started = std::time::Instant::now();
        match check_file(&file, &entry["survey"], &out) {
            Ok(notes) => eprintln!(
                "ok   {name} ({:.1} s){}",
                started.elapsed().as_secs_f64(),
                if notes.is_empty() {
                    String::new()
                } else {
                    format!(": {}", notes.join("; "))
                }
            ),
            Err(e) => {
                eprintln!("FAIL {name}: {e}");
                failures.push(format!("{name}: {e}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} file(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
