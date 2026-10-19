//! Generated sample documents for tests, benchmarks and the interop harness.
//!
//! Real-world files belong in `tests/corpus/`. These generated files cover the geometric
//! cases (every rotation, an offset CropBox, UserUnit) deterministically, and give the
//! performance checks a large file of known shape.

use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf::{Buffer, Font, SimpleFontEncoding};

use crate::docinfo::PdfA;
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
    /// The PDF/A level the document claims in XMP metadata. Only the claim is written:
    /// the pages do not comply (their font is not embedded).
    pub pdfa: Option<PdfA>,
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
            pdfa: None,
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
    if let Some(level) = spec.pdfa {
        let xmp = pdfa_metadata(level);
        let mut metadata =
            doc.add_stream(&Buffer::from_copied_bytes(xmp.as_bytes())?, None, false)?;
        metadata.dict_put("Type", PdfObject::new_name("Metadata")?)?;
        metadata.dict_put("Subtype", PdfObject::new_name("XML")?)?;
        doc.catalog()?.dict_put("Metadata", metadata)?;
    }
    Ok(doc)
}

/// An XMP packet that claims a PDF/A level, as PDF/A writers identify their files.
fn pdfa_metadata(level: PdfA) -> String {
    let conformance = level
        .conformance
        .map(|c| format!("   <pdfaid:conformance>{c}</pdfaid:conformance>{NL}"))
        .unwrap_or_default();
    [
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>"#,
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">"#,
        r#" <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
        r#"  <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">"#,
        &format!(
            "   <pdfaid:part>{}</pdfaid:part>{NL}{conformance}  </rdf:Description>",
            level.part
        ),
        " </rdf:RDF>",
        "</x:xmpmeta>",
        r#"<?xpacket end="w"?>"#,
    ]
    .join(NL)
}

const NL: &str = "\n";

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

/// What [`image_sample`] draws, by image resource name.
pub const IMAGE_SAMPLE_CASES: &[(&str, &str)] = &[
    (
        "Photo",
        "RGB photograph, 1200 x 900, Flate, drawn at 400 dpi",
    ),
    (
        "Screen",
        "RGB screenshot of a few colours, 600 x 400, Flate, 300 dpi",
    ),
    (
        "Formed",
        "RGB photograph in a form scaled by half, 800 x 600, 288 dpi",
    ),
    (
        "GrayRgb",
        "grey pixels stored as RGB, 600 x 600, Flate, 200 dpi",
    ),
    (
        "Bitonal",
        "black and white stored as 8-bit grey, 1000 x 1000, 400 dpi",
    ),
    (
        "Mask",
        "image mask (stencil), 800 x 200, painted red, 400 dpi",
    ),
    ("Soft", "RGB with a soft mask, 400 x 400, 200 dpi"),
    ("Jpeg", "RGB JPEG at quality 95, 1200 x 800, 400 dpi"),
    ("Cmyk", "CMYK JPEG at quality 95, 600 x 400, 200 dpi"),
    (
        "Palette",
        "Indexed RGB, 16 colours, 500 x 500, Flate, 250 dpi",
    ),
    (
        "Stamp",
        "RGB photograph in a stamp annotation's appearance, 600 x 600, 288 dpi",
    ),
    (
        "Tiled",
        "RGB photograph tiled by a pattern scaled by half, 400 x 400, 576 dpi",
    ),
];

/// A deterministic photograph-like image: smooth colour gradients with fine noise.
fn photo(width: usize, height: usize, seed: u32) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut data = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = f64::from(state % 9) - 4.0;
            let fx = x as f64 / width as f64;
            let fy = y as f64 / height as f64;
            let r = 128.0 + 100.0 * (fx * 6.0 + fy * 2.0).sin() + noise;
            let g = 128.0 + 90.0 * (fy * 5.0 - fx * 3.0).cos() + noise;
            let b = 128.0 + 80.0 * ((fx + fy) * 4.0).sin() + noise;
            data.extend([r, g, b].map(|v| v.clamp(0.0, 255.0) as u8));
        }
    }
    data
}

/// Adds an image XObject: `dict` its entries besides the type; `samples` Flate-compressed
/// (no predictors, as many generators write them) if `flate`, or already encoded.
fn add_image(doc: &mut PdfDocument, dict: &str, samples: &[u8], flate: bool) -> Result<PdfObject> {
    let (data, filter) = if flate {
        (
            crate::ffi::compress::deflate(samples, 6)?,
            "/Filter /FlateDecode",
        )
    } else {
        (samples.to_vec(), "")
    };
    let dict = doc.new_object_from_str(&format!(
        "<< /Type /XObject /Subtype /Image {dict} {filter} >>"
    ))?;
    Ok(doc.add_stream(&Buffer::from_copied_bytes(&data)?, Some(&dict), true)?)
}

/// Adds a form XObject drawing `content` with `xobjects` (name, object) as its resources.
fn add_form(
    doc: &mut PdfDocument,
    dict: &str,
    content: &str,
    xobjects: Vec<(&str, PdfObject)>,
) -> Result<PdfObject> {
    let mut form = doc.new_object_from_str(dict)?;
    let mut resources = doc.new_dict()?;
    let mut names = doc.new_dict()?;
    for (name, obj) in xobjects {
        names.dict_put(name, obj)?;
    }
    resources.dict_put("XObject", names)?;
    form.dict_put("Resources", resources)?;
    Ok(doc.add_stream(
        &Buffer::from_copied_bytes(content.as_bytes())?,
        Some(&form),
        false,
    )?)
}

fn rgb_dict(w: usize, h: usize) -> String {
    format!("/Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8")
}

fn gray_dict(w: usize, h: usize) -> String {
    format!("/Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8")
}

/// A document of three pages whose images cover what Compress handles (see
/// [`IMAGE_SAMPLE_CASES`]), with text in an embedded font (MuPDF's Times, CFF, as a CID
/// font) to subset.
pub fn image_sample() -> Result<PdfDocument> {
    use crate::ffi::compress::encode_jpeg;

    let mut doc = PdfDocument::new();
    let photo_im = add_image(&mut doc, &rgb_dict(1200, 900), &photo(1200, 900, 1), true)?;
    let mut screen = Vec::with_capacity(600 * 400 * 3);
    for y in 0..400 {
        for x in 0..600 {
            screen.extend(match (x / 100 + y / 80) % 4 {
                0 => [255, 255, 255],
                1 => [30, 60, 160],
                2 => [240, 240, 240],
                _ => [20, 20, 20],
            });
        }
    }
    let screen_im = add_image(&mut doc, &rgb_dict(600, 400), &screen, true)?;
    let formed_im = add_image(&mut doc, &rgb_dict(800, 600), &photo(800, 600, 2), true)?;
    let gray_rgb: Vec<u8> = photo(600, 600, 3)
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| [p[1]; 3])
        .collect();
    let gray_rgb_im = add_image(&mut doc, &rgb_dict(600, 600), &gray_rgb, true)?;
    let bitonal: Vec<u8> = (0..1000 * 1000)
        .map(|i| {
            let (x, y) = (i % 1000, i / 1000);
            if (y / 20) % 2 == 0 && (x / 7) % 3 != 0 && x > 50 && x < 950 {
                0
            } else {
                255
            }
        })
        .collect();
    let bitonal_im = add_image(&mut doc, &gray_dict(1000, 1000), &bitonal, true)?;
    // An ellipse: 0 bits paint.
    let mut mask = vec![0xffu8; 100 * 200];
    for y in 0..200i32 {
        for x in 0..800i32 {
            if (x - 400).pow(2) / 16 + (y - 100).pow(2) < 90 * 90 {
                mask[y as usize * 100 + x as usize / 8] &= !(0x80 >> (x % 8));
            }
        }
    }
    let mask_im = add_image(
        &mut doc,
        "/Width 800 /Height 200 /ImageMask true /BitsPerComponent 1",
        &mask,
        true,
    )?;
    let alpha: Vec<u8> = (0..400 * 400)
        .map(|i| ((i % 400) as f64 / 400.0 * 255.0) as u8)
        .collect();
    let alpha_im = add_image(&mut doc, &gray_dict(400, 400), &alpha, true)?;
    let mut soft_im = add_image(&mut doc, &rgb_dict(400, 400), &photo(400, 400, 4), true)?;
    soft_im.dict_put("SMask", alpha_im)?;
    let jpeg = encode_jpeg(&photo(1200, 800, 5), 1200, 800, 3, 95, false)?;
    let jpeg_im = add_image(
        &mut doc,
        &format!("{} /Filter /DCTDecode", rgb_dict(1200, 800)),
        &jpeg,
        false,
    )?;
    let cmyk: Vec<u8> = photo(600, 400, 6)
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], 20])
        .collect();
    let cmyk_jpeg = encode_jpeg(&cmyk, 600, 400, 4, 95, false)?;
    let cmyk_im = add_image(
        &mut doc,
        "/Width 600 /Height 400 /ColorSpace /DeviceCMYK /BitsPerComponent 8 /Filter /DCTDecode",
        &cmyk_jpeg,
        false,
    )?;
    let palette: String = (0..16)
        .map(|i| format!("{:02X}{:02X}{:02X}", i * 16, 255 - i * 12, (i * 40) % 256))
        .collect();
    let indices: Vec<u8> = (0..500 * 500)
        .map(|i| (((i % 500) / 32 + (i / 500) / 32) % 16) as u8)
        .collect();
    let palette_im = add_image(
        &mut doc,
        &format!(
            "/Width 500 /Height 500 /ColorSpace [/Indexed /DeviceRGB 15 <{palette}>] /BitsPerComponent 8"
        ),
        &indices,
        true,
    )?;
    let stamp_im = add_image(&mut doc, &rgb_dict(600, 600), &photo(600, 600, 7), true)?;
    let tiled_im = add_image(&mut doc, &rgb_dict(400, 400), &photo(400, 400, 8), true)?;

    let font = Font::new("Times-Roman")?;
    let times = doc.add_font(&font)?;
    let helvetica = doc.add_simple_font(&Font::new("Helvetica")?, SimpleFontEncoding::Latin)?;
    let mut glyphs = String::new();
    for c in MARKER.chars() {
        glyphs.push_str(&format!("{:04X}", font.encode_character(c as i32)?));
    }

    let form = add_form(
        &mut doc,
        "<< /Type /XObject /Subtype /Form /BBox [0 0 400 300] /Matrix [0.5 0 0 0.5 0 0] >>",
        "q 400 0 0 300 0 0 cm /Formed Do Q",
        vec![("Formed", formed_im)],
    )?;
    let pattern = add_form(
        &mut doc,
        "<< /Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 100 100] \
         /XStep 100 /YStep 100 /Matrix [0.5 0 0 0.5 0 0] >>",
        "q 100 0 0 100 0 0 cm /Tiled Do Q",
        vec![("Tiled", tiled_im)],
    )?;
    let stamp_ap = add_form(
        &mut doc,
        "<< /Type /XObject /Subtype /Form /BBox [0 0 150 150] >>",
        "q 150 0 0 150 0 0 cm /Stamp Do Q",
        vec![("Stamp", stamp_im)],
    )?;

    let pages: Vec<(String, Vec<(&str, PdfObject)>)> = vec![
        (
            format!(
                "q 216 0 0 162 72 560 cm /Photo Do Q\n\
                 q 144 0 0 96 360 620 cm /Screen Do Q\n\
                 q 1 0 0 1 72 300 cm /Fm Do Q\n\
                 BT /T 14 Tf 72 740 Td <{glyphs}> Tj ET\n"
            ),
            vec![("Photo", photo_im), ("Screen", screen_im), ("Fm", form)],
        ),
        (
            "q 216 0 0 216 72 520 cm /GrayRgb Do Q\n\
             q 180 0 0 180 330 560 cm /Bitonal Do Q\n\
             1 0 0 rg q 144 0 0 36 72 420 cm /Mask Do Q\n\
             q 144 0 0 144 330 300 cm /Soft Do Q\n\
             BT /H 12 Tf 72 740 Td (Grey, black and white, a mask and transparency) Tj ET\n"
                .to_owned(),
            vec![
                ("GrayRgb", gray_rgb_im),
                ("Bitonal", bitonal_im),
                ("Mask", mask_im),
                ("Soft", soft_im),
            ],
        ),
        (
            "q 216 0 0 144 72 560 cm /Jpeg Do Q\n\
             q 216 0 0 144 330 560 cm /Cmyk Do Q\n\
             q 144 0 0 144 72 360 cm /Palette Do Q\n\
             q /Pattern cs /P1 scn 330 300 200 200 re f Q\n\
             BT /H 12 Tf 72 740 Td (JPEG, CMYK JPEG, a palette, a stamp and a pattern) Tj ET\n"
                .to_owned(),
            vec![
                ("Jpeg", jpeg_im),
                ("Cmyk", cmyk_im),
                ("Palette", palette_im),
            ],
        ),
    ];
    for (index, (content, images)) in pages.into_iter().enumerate() {
        let stream =
            doc.add_stream(&Buffer::from_copied_bytes(content.as_bytes())?, None, false)?;
        let mut xobjects = doc.new_dict()?;
        for (name, im) in images {
            xobjects.dict_put(name, im)?;
        }
        let mut fonts = doc.new_dict()?;
        fonts.dict_put("T", times.clone())?;
        fonts.dict_put("H", helvetica.clone())?;
        let mut resources = doc.new_dict()?;
        resources.dict_put("XObject", xobjects)?;
        resources.dict_put("Font", fonts)?;
        if index == 2 {
            let mut patterns = doc.new_dict()?;
            patterns.dict_put("P1", pattern.clone())?;
            resources.dict_put("Pattern", patterns)?;
        }
        let mut page = doc.new_dict()?;
        page.dict_put("Type", PdfObject::new_name("Page")?)?;
        page.dict_put(
            "MediaBox",
            objects::rect_array(&doc, Rect::new(0.0, 0.0, 612.0, 792.0))?,
        )?;
        page.dict_put("Resources", resources)?;
        page.dict_put("Contents", stream)?;
        let page = doc.add_object(&page)?;
        doc.insert_page(doc.page_count()?, &page)?;
    }

    // A stamp on page 3 whose appearance draws a photograph into 150 pt.
    let mut page = doc.find_page(2)?;
    let mut stamp = doc.new_object_from_str(
        "<< /Type /Annot /Subtype /Stamp /Rect [72 150 222 300] /F 4 /Name /Approved >>",
    )?;
    let mut appearance = doc.new_dict()?;
    appearance.dict_put("N", stamp_ap)?;
    stamp.dict_put("AP", appearance)?;
    stamp.dict_put("P", page.clone())?;
    let stamp = doc.add_object(&stamp)?;
    let mut annots = doc.new_array()?;
    annots.array_push(stamp)?;
    page.dict_put("Annots", annots)?;
    Ok(doc)
}
