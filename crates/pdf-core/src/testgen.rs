//! Generated sample documents for tests, benchmarks and the interop harness.
//!
//! Real-world files belong in `tests/corpus/`. These generated files cover the geometric
//! cases (every rotation, an offset CropBox, UserUnit) deterministically, and give the
//! performance checks a large file of known shape.

use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf::{Buffer, Font, SimpleFontEncoding};

use crate::error::Result;
use crate::geometry::Rect;
use crate::objects;

/// The sentence every sample page contains, used to find text for markup tests.
pub const MARKER: &str = "The quick brown fox jumps over the lazy dog";

#[derive(Debug, Clone)]
pub struct SampleSpec {
    pub pages: usize,
    pub media_box: Rect,
    pub rotate: i32,
    pub crop_box: Option<Rect>,
    pub user_unit: Option<f64>,
    pub title: String,
}

impl Default for SampleSpec {
    fn default() -> Self {
        SampleSpec {
            pages: 3,
            media_box: Rect::new(0.0, 0.0, 612.0, 792.0),
            rotate: 0,
            crop_box: None,
            user_unit: None,
            title: "Lectrix sample".into(),
        }
    }
}

const FILLER: [&str; 6] = [
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.",
    "Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip.",
    "Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu.",
    "Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt.",
    "Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium dolor.",
    "Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit.",
];

/// Builds a document with `spec.pages` text pages. Text is placed relative to the visible
/// box so it stays on-screen with any CropBox.
pub fn sample_document(spec: &SampleSpec) -> Result<PdfDocument> {
    let mut doc = PdfDocument::new();

    // Base-14 Helvetica, not embedded, with /Widths so every reader spaces it the same.
    let font = doc.add_simple_font(&Font::new("Helvetica")?, SimpleFontEncoding::Latin)?;

    let visible = spec
        .crop_box
        .map(|c| c.intersect(&spec.media_box))
        .unwrap_or(spec.media_box);

    for i in 0..spec.pages {
        let content = page_content(i, visible);
        let stream =
            doc.add_stream(&Buffer::from_copied_bytes(content.as_bytes())?, None, false)?;

        let mut fonts = doc.new_dict()?;
        fonts.dict_put("F1", font.clone())?;
        let mut resources = doc.new_dict()?;
        resources.dict_put("Font", fonts)?;

        let mut page = doc.new_dict()?;
        page.dict_put("Type", PdfObject::new_name("Page")?)?;
        page.dict_put("MediaBox", objects::rect_array(&doc, spec.media_box)?)?;
        if let Some(crop) = spec.crop_box {
            page.dict_put("CropBox", objects::rect_array(&doc, crop)?)?;
        }
        if spec.rotate != 0 {
            page.dict_put("Rotate", PdfObject::new_int(spec.rotate)?)?;
        }
        if let Some(unit) = spec.user_unit {
            page.dict_put("UserUnit", PdfObject::new_real(unit as f32)?)?;
        }
        page.dict_put("Resources", resources)?;
        page.dict_put("Contents", stream)?;
        let page = doc.add_object(&page)?;
        doc.insert_page(doc.page_count()?, &page)?;
    }

    let mut info = doc.new_dict()?;
    info.dict_put("Title", objects::text_string(&doc, &spec.title)?)?;
    info.dict_put("Producer", PdfObject::new_string("Lectrix testgen")?)?;
    let info = doc.add_object(&info)?;
    doc.trailer()?.dict_put("Info", info)?;
    Ok(doc)
}

fn page_content(index: usize, visible: Rect) -> String {
    let left = visible.x0 + 36.0;
    let top = visible.y1 - 60.0;
    let mut s = String::new();
    // A thin frame inside the visible box, to make crop and rotation visible in renders.
    s.push_str(&format!(
        "0.6 G 1 w {} {} {} {} re S\n",
        visible.x0 + 18.0,
        visible.y0 + 18.0,
        visible.width() - 36.0,
        visible.height() - 36.0
    ));
    s.push_str(&format!(
        "BT /F1 24 Tf {left} {top} Td (Page {}) Tj ET\n",
        index + 1
    ));
    s.push_str(&format!(
        "BT /F1 12 Tf {left} {} Td ({MARKER} on page {}.) Tj ET\n",
        top - 36.0,
        index + 1
    ));
    let mut y = top - 60.0;
    let mut line = index;
    while y > visible.y0 + 48.0 {
        s.push_str(&format!(
            "BT /F1 11 Tf {left} {y} Td ({}) Tj ET\n",
            FILLER[line % FILLER.len()]
        ));
        y -= 15.0;
        line += 1;
    }
    s
}
