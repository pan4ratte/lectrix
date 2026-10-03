//! `pdf-cli survey`: a read-only profile of a PDF, as one JSON line, used to pick test
//! files that cover the categories in AGENTS.md section 9 (rotation, CropBox offsets,
//! damage, encryption, signatures, size, scans, scripts, outlines, labels, forms, and
//! annotations made by other apps).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use mupdf::pdf::PdfObject;
use mupdf::text_page::TextPageFlags;
use serde::Serialize;

use pdf_core::geometry::{PageGeometry, read_page_boxes};
use pdf_core::{Result, docinfo, labels, outline, render};

#[derive(Debug, Default, Serialize)]
pub struct Survey {
    pub path: String,
    pub bytes: u64,
    pub error: Option<String>,
    pub needs_password: bool,
    pub encrypted: bool,
    pub repaired: bool,
    pub signed: bool,
    pub pages: usize,
    /// Pages per effective rotation other than 0 ("90", "180", "270").
    pub rotated_pages: BTreeMap<String, usize>,
    /// Pages whose visible box does not start at (0, 0).
    pub offset_crop_pages: usize,
    pub user_unit_pages: usize,
    pub label_rules: usize,
    pub outline_items: usize,
    pub outline_depth: usize,
    pub form_fields: usize,
    /// Annotation count per subtype.
    pub annotations: BTreeMap<String, usize>,
    /// Annotations (other than links and popups) without a normal appearance stream.
    pub annotations_without_ap: usize,
    /// Text characters on the sampled pages (first, middle, last).
    pub sample_text_chars: usize,
    pub sample_cjk_chars: usize,
    pub sample_rtl_chars: usize,
    pub producer: Option<String>,
    pub creator: Option<String>,
    pub open_ms: f64,
    /// Display list and raster of the first page at 100% on a 96 dpi screen.
    pub first_page_ms: f64,
}

pub fn survey(path: &Path) -> Survey {
    let mut s = Survey {
        path: path.display().to_string(),
        bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        ..Survey::default()
    };
    if let Err(e) = fill(path, &mut s) {
        s.error = Some(format!("{e}"));
    }
    s
}

fn fill(path: &Path, s: &mut Survey) -> Result<()> {
    let t0 = Instant::now();
    let doc = pdf_core::ffi::open_pdf_shared(path)?;
    s.open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    s.encrypted = doc.trailer()?.get_dict("Encrypt")?.is_some();
    if doc.needs_password()? {
        s.needs_password = true;
        return Ok(());
    }
    s.repaired = pdf_core::ffi::was_repaired(&doc)?;
    s.signed = docinfo::is_signed(&doc)?;
    let count = usize::try_from(doc.page_count()?).unwrap_or(0);
    s.pages = count;

    for i in 0..count {
        let page = doc.find_page(i32::try_from(i).unwrap_or(i32::MAX))?;
        let boxes = read_page_boxes(&page)?;
        let g = PageGeometry::new(&boxes);
        if g.rotation != 0 {
            *s.rotated_pages.entry(g.rotation.to_string()).or_default() += 1;
        }
        if g.visible_box.x0.abs() > 0.01 || g.visible_box.y0.abs() > 0.01 {
            s.offset_crop_pages += 1;
        }
        if (boxes.user_unit - 1.0).abs() > 1e-6 {
            s.user_unit_pages += 1;
        }
        count_annotations(&page, s)?;
    }

    s.label_rules = labels::read_rules(&doc)?.len();
    let items = outline::read_outline(&doc)?;
    s.outline_items = count_items(&items);
    s.outline_depth = depth(&items);
    if let Some(fields) = doc
        .catalog()?
        .get_dict("AcroForm")?
        .and_then(|f| f.get_dict("Fields").ok().flatten())
    {
        s.form_fields = fields.len()?;
    }
    if let Some(info) = doc.trailer()?.get_dict("Info")? {
        s.producer = string(&info, "Producer")?;
        s.creator = string(&info, "Creator")?;
    }

    let mut samples = vec![0, count / 2, count.saturating_sub(1)];
    samples.dedup();
    for (n, page) in samples.into_iter().filter(|p| *p < count).enumerate() {
        let t1 = Instant::now();
        let list = render::display_list(&doc, page)?;
        if n == 0 {
            render::rasterize(&list, 96.0 / 72.0)?;
            s.first_page_ms = t1.elapsed().as_secs_f64() * 1000.0;
        }
        let text = list.to_text_page(TextPageFlags::empty())?.to_text()?;
        for c in text.chars().filter(|c| !c.is_whitespace()) {
            s.sample_text_chars += 1;
            let u = u32::from(c);
            if (0x3040..=0x30FF).contains(&u)
                || (0x4E00..=0x9FFF).contains(&u)
                || (0xAC00..=0xD7AF).contains(&u)
            {
                s.sample_cjk_chars += 1;
            }
            if (0x0590..=0x08FF).contains(&u) {
                s.sample_rtl_chars += 1;
            }
        }
    }
    Ok(())
}

fn count_annotations(page: &PdfObject, s: &mut Survey) -> Result<()> {
    let Some(annots) = page.get_dict("Annots")? else {
        return Ok(());
    };
    if !annots.is_array()? {
        return Ok(());
    }
    for i in 0..i32::try_from(annots.len()?).unwrap_or(i32::MAX) {
        let Some(annot) = annots.get_array(i)? else {
            continue;
        };
        if !annot.is_dict()? {
            continue;
        }
        let subtype = match annot.get_dict("Subtype")? {
            Some(n) if n.is_name()? => String::from_utf8_lossy(&n.as_name()?).into_owned(),
            _ => "(none)".to_owned(),
        };
        let has_ap = annot
            .get_dict("AP")?
            .and_then(|ap| ap.get_dict("N").ok().flatten())
            .is_some();
        if !has_ap && subtype != "Link" && subtype != "Popup" {
            s.annotations_without_ap += 1;
        }
        *s.annotations.entry(subtype).or_default() += 1;
    }
    Ok(())
}

fn count_items(items: &[outline::ReadOutlineItem]) -> usize {
    items.iter().map(|i| 1 + count_items(&i.children)).sum()
}

fn depth(items: &[outline::ReadOutlineItem]) -> usize {
    items
        .iter()
        .map(|i| 1 + depth(&i.children))
        .max()
        .unwrap_or(0)
}

fn string(dict: &PdfObject, key: &str) -> Result<Option<String>> {
    Ok(match dict.get_dict(key)? {
        Some(v) if v.is_string()? => Some(v.as_string_lossy()?),
        _ => None,
    })
}
