//! Real-world files from the local corpus (AGENTS.md section 9): `tests/local-corpus/`,
//! git-ignored, built by `tests/local-corpus/select.py`. Run on demand with
//! `cargo test --release -p pdf-core --test local_corpus -- --ignored --nocapture`;
//! it passes trivially when the corpus is absent (CI).
//!
//! Every file is opened, rendered, read and searched as the viewer does. Then a copy in
//! `target/test-output/` gets one page rotated and is saved in place, and the reopened
//! copy must keep everything the user did not touch: page count, labels (their number tree
//! is not even written again), bookmarks and every annotation made by other apps (section
//! 5.1 rule 10). Labels are also checked against MuPDF's own reading of them. Two more
//! tests combine each file with another, and move and delete its pages. The corpus files
//! themselves are only read.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use common::{objects_written_after, out_dir, qpdf_severity, writing};
use mupdf::pdf::{PdfDocument, PdfObject};
use pdf_core::Error;
use pdf_core::geometry::{PageGeometry, read_page_boxes};
use pdf_core::labels::{LabelRule, LabelStyle, labels_for_pages, normalize_rules, read_rules};
use pdf_core::merge::{self, MergeOptions, MergeSource};
use pdf_core::ops::Operation;
use pdf_core::outline;
use pdf_core::save::SaveKind;
use pdf_core::session::Session;
use pdf_core::testgen::{SampleSpec, sample_document};
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
            .render_rgba(page, 0.5, None, &[])
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
            prefix: "Lectrix-".into(),
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
        if last != "Lectrix-AA" || last != labels_for_pages(&expected, pages)[pages - 1] {
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

/// Bookmarks in the outline as the panel reads it.
fn bookmark_count(doc: &PdfDocument) -> usize {
    let mut n = 0;
    outline::read_bookmarks(doc).unwrap().for_each(|_| n += 1);
    n
}

/// Combines a generated two-page file with `file` (every page, default options) and checks
/// that everything came along: pages, every annotation (links and widgets included), every
/// page's label, the whole outline nested under the file, and a structure no worse than
/// the original's.
fn combine_file(file: &Path, out: &Path) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let source = match MergeSource::open(file, None) {
        Ok(source) => source,
        Err(Error::CopyNotPermitted(_)) => return Ok(vec!["refused: copying not allowed".into()]),
        Err(e) => return Err(format!("open: {e}")),
    };
    let pages = source.doc.page_count().unwrap() as usize;
    let annots_before = annotation_counts(&source.doc);
    // Combined, the pages of a file without labels are numbered 1, 2, 3... (MuPDF reads
    // them as ""; an empty label stored in a file stays empty).
    let unlabeled = read_rules(&source.doc).unwrap().is_empty();
    let labels_before: Vec<String> = (0..pages)
        .map(|p| {
            if unlabeled {
                (p + 1).to_string()
            } else {
                source.doc.page_label(p).unwrap()
            }
        })
        .collect();
    let bookmarks_before = bookmark_count(&source.doc);
    let sample = MergeSource {
        doc: sample_document(&SampleSpec {
            pages: 2,
            ..SampleSpec::default()
        })
        .unwrap(),
        name: "Sample".into(),
    };
    let started = std::time::Instant::now();
    let (merged, report) = merge::merge_all(&[sample, source], MergeOptions::default())
        .map_err(|e| format!("merge: {e}"))?;
    let target = out.join(format!(
        "combined-{}",
        file.file_name().unwrap().to_string_lossy()
    ));
    writing(|| pdf_core::save::save_atomic(&merged, SaveKind::Full, None, &target))
        .map_err(|e| format!("save: {e}"))?;
    notes.push(format!("{:.1} s", started.elapsed().as_secs_f64()));
    drop(merged);

    let after = PdfDocument::open(target.to_str().unwrap()).map_err(|e| format!("reopen: {e}"))?;
    if after.page_count().unwrap() as usize != pages + 2 {
        return Err("page count".into());
    }
    if annotation_counts(&after)[2..] != annots_before[..] {
        return Err("annotations did not all come along".into());
    }
    let labels_after: Vec<String> = (2..pages + 2)
        .map(|p| after.page_label(p).unwrap())
        .collect();
    if labels_after != labels_before {
        let first = labels_after
            .iter()
            .zip(&labels_before)
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(format!(
            "page {} label {:?}, was {:?}",
            first + 1,
            labels_after.get(first),
            labels_before.get(first)
        ));
    }
    // One top-level bookmark per file, the file's own below its.
    if bookmark_count(&after) != bookmarks_before + 2 {
        return Err(format!(
            "{} bookmarks, expected {}",
            bookmark_count(&after),
            bookmarks_before + 2
        ));
    }
    if report.renamed_destinations + report.renamed_fields > 0 {
        notes.push(format!(
            "renamed {} names, {} fields",
            report.renamed_destinations, report.renamed_fields
        ));
    }
    if let (Some(input), Some(output)) = (qpdf_severity(file), qpdf_severity(&target))
        && output > input
    {
        return Err(format!("qpdf --check got worse: {input} -> {output}"));
    }
    fs::remove_file(&target).ok();
    Ok(notes)
}

#[test]
#[ignore = "real-world local corpus, minutes long: run with --ignored"]
fn local_corpus_files_combine_with_everything_they_hold() {
    let manifest_path = corpus_dir().join("manifest.json");
    let Ok(manifest) = fs::read_to_string(&manifest_path) else {
        eprintln!("no local corpus at {}; skipped", manifest_path.display());
        return;
    };
    let manifest: Vec<Value> = serde_json::from_str(&manifest).unwrap();
    let out = out_dir("local-corpus-combine");
    let mut failures = Vec::new();
    for entry in &manifest {
        let name = entry["file"].as_str().unwrap();
        let file = corpus_dir().join("files").join(name);
        match combine_file(&file, &out) {
            Ok(notes) => eprintln!("ok   {name}: {}", notes.join("; ")),
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

/// What tells a page from another without reading its content: its size and rotation, and
/// how many annotations it has.
type PageMark = (u32, u32, u16, usize);

fn page_marks(session: &Session) -> Result<Vec<PageMark>, String> {
    let info = session.info().map_err(|e| format!("info: {e}"))?;
    Ok(info
        .pages
        .iter()
        .zip(&info.annotations)
        .map(|(p, a)| {
            (
                (p.width * 100.0).round() as u32,
                (p.height * 100.0).round() as u32,
                p.rotation,
                a.len(),
            )
        })
        .collect())
}

/// Moves pages of `file` (the last one to the front, then the first half to the end),
/// extracts three into a new file, deletes one, and checks after each step that every page looks as it did, wherever it is
/// now: pages take their size and rotation from the nodes above them, and real files have
/// trees of every shape. Undo must bring the order back, and the saved files must be no
/// worse for `qpdf --check` than the original. Nothing is written to `file`.
fn rearrange_file(file: &Path, out: &Path) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let (session, info) = Session::open(file, None).map_err(|e| format!("open: {e}"))?;
    let n = info.pages.len();
    if n < 3 {
        session.close();
        return Ok(vec!["fewer than three pages".into()]);
    }
    let before = page_marks(&session)?;
    let half: Vec<usize> = (0..n / 2).collect();
    let last_to_front = Operation::MovePages {
        pages: vec![n - 1],
        to: 0,
    };
    let half_to_end = Operation::MovePages {
        pages: half.clone(),
        to: n,
    };
    let started = std::time::Instant::now();
    let moved = match session.apply(last_to_front) {
        Ok(change) => change,
        Err(e @ (Error::NotPermitted | Error::DamagedPageTree)) => {
            session.close();
            return Ok(vec![format!("refused: {e}")]);
        }
        Err(e) => return Err(format!("move: {e}")),
    };
    let one_ms = started.elapsed().as_secs_f64() * 1000.0;
    // The order now, as indexes into the original one.
    let mut order: Vec<usize> = std::iter::once(n - 1).chain(0..n - 1).collect();
    let expected_map: Vec<Option<usize>> = order.iter().map(|&p| Some(p)).collect();
    if moved.page_map.as_ref() != Some(&expected_map) {
        return Err("the page map does not say the last page is first".into());
    }
    if moved.labels.is_some() {
        return Err("moving a page changed the labels".into());
    }
    let marks = |order: &[usize]| -> Vec<PageMark> { order.iter().map(|&p| before[p]).collect() };
    if page_marks(&session)? != marks(&order) {
        return Err("a page looks different after the first move".into());
    }

    // Three pages into a new file, from the document as it is now (the move is not saved).
    let name = file.file_name().unwrap().to_string_lossy();
    let picked = [0, n / 2, n - 1];
    let extract = out.join(format!("extract-{name}"));
    let started = std::time::Instant::now();
    match writing(|| session.extract(picked.to_vec(), &extract, |_| true)) {
        Ok(report) => {
            notes.push(format!(
                "extract {:.0} ms",
                started.elapsed().as_secs_f64() * 1000.0
            ));
            if report.pages != 3 {
                return Err(format!("{} pages extracted", report.pages));
            }
            let (copy, _) = Session::open(&extract, None).map_err(|e| format!("extract: {e}"))?;
            let size = |m: &PageMark| (m.0, m.1, m.2);
            let got: Vec<_> = page_marks(&copy)?.iter().map(size).collect();
            copy.close();
            let want: Vec<_> = picked.iter().map(|&p| size(&before[order[p]])).collect();
            if got != want {
                return Err("an extracted page looks different".into());
            }
            if let (Some(input), Some(output)) = (qpdf_severity(file), qpdf_severity(&extract))
                && output > input
            {
                return Err(format!(
                    "qpdf --check is worse for the extract: {input} -> {output}"
                ));
            }
            fs::remove_file(&extract).ok();
        }
        Err(Error::CopyNotPermitted(_)) => {
            notes.push("extract refused: copying not allowed".into())
        }
        Err(e) => return Err(format!("extract: {e}")),
    }

    let started = std::time::Instant::now();
    session
        .apply(half_to_end)
        .map_err(|e| format!("move half: {e}"))?;
    let half_ms = started.elapsed().as_secs_f64() * 1000.0;
    order = order[n / 2..]
        .iter()
        .chain(&order[..n / 2])
        .copied()
        .collect();
    if page_marks(&session)? != marks(&order) {
        return Err("a page looks different after the second move".into());
    }
    notes.push(format!(
        "{n} pages; one page {one_ms:.0} ms, {} pages {half_ms:.0} ms",
        n / 2
    ));

    session.undo().map_err(|e| format!("undo: {e}"))?;
    session.undo().map_err(|e| format!("undo: {e}"))?;
    if page_marks(&session)? != before {
        return Err("undo did not bring the pages back".into());
    }
    session.redo().map_err(|e| format!("redo: {e}"))?;
    session.redo().map_err(|e| format!("redo: {e}"))?;

    let target = out.join(format!("moved-{name}"));
    let saved = writing(|| session.save(SaveKind::Incremental, Some(target.clone())))
        .map_err(|e| format!("save: {e}"))?;
    if saved.outcome.fell_back_to_full {
        notes.push("saved in full".into());
    }
    if page_marks(&session)? != marks(&order) {
        return Err("a page looks different after saving".into());
    }
    let input = qpdf_severity(file);
    if let (Some(input), Some(output)) = (input, qpdf_severity(&target))
        && output > input
    {
        return Err(format!(
            "qpdf --check got worse after moving: {input} -> {output}"
        ));
    }

    // One page from the middle.
    let gone = n / 2;
    let started = std::time::Instant::now();
    let change = session
        .apply(Operation::DeletePages { pages: vec![gone] })
        .map_err(|e| format!("delete: {e}"))?;
    let delete_ms = started.elapsed().as_secs_f64() * 1000.0;
    order.remove(gone);
    if change.page_count != n - 1 || page_marks(&session)? != marks(&order) {
        return Err("a page looks different after deleting one".into());
    }
    let report = change.deleted.ok_or("no report of the deletion")?;
    notes.push(format!(
        "delete {delete_ms:.0} ms ({} links, {} bookmarks, {} fields)",
        report.links, report.bookmarks, report.fields
    ));
    let deleted = out.join(format!("deleted-{name}"));
    writing(|| session.save(SaveKind::Incremental, Some(deleted.clone())))
        .map_err(|e| format!("save after deleting: {e}"))?;
    session.close();
    if let (Some(input), Some(output)) = (input, qpdf_severity(&deleted))
        && output > input
    {
        return Err(format!(
            "qpdf --check got worse after deleting: {input} -> {output}"
        ));
    }
    let reopened =
        PdfDocument::open(deleted.to_str().unwrap()).map_err(|e| format!("reopen: {e}"))?;
    if reopened.page_count().unwrap() as usize != n - 1 {
        return Err("page count after reopening".into());
    }
    drop(reopened);
    fs::remove_file(&target).ok();
    fs::remove_file(&deleted).ok();
    Ok(notes)
}

#[test]
#[ignore = "real-world local corpus, minutes long: run with --ignored"]
fn local_corpus_pages_move_and_are_deleted_as_they_are() {
    let manifest_path = corpus_dir().join("manifest.json");
    let Ok(manifest) = fs::read_to_string(&manifest_path) else {
        eprintln!("no local corpus at {}; skipped", manifest_path.display());
        return;
    };
    let manifest: Vec<Value> = serde_json::from_str(&manifest).unwrap();
    let out = out_dir("local-corpus-pages");
    let mut failures = Vec::new();
    for entry in &manifest {
        let name = entry["file"].as_str().unwrap();
        let file = corpus_dir().join("files").join(name);
        match rearrange_file(&file, &out) {
            Ok(notes) => eprintln!("ok   {name}: {}", notes.join("; ")),
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
