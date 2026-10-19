//! Compress (section 6.11, ADR 0027): images rewritten where they are, at the resolution
//! they are drawn at; nothing visible changed by the lossless preset; fonts subset except
//! the form's; removals; encryption kept; the PDF/A claim removed; cancelling writes
//! nothing.

mod common;

use std::path::{Path, PathBuf};

use mupdf::pdf::{PdfDocument, PdfObject, PdfWriteOptions};
use mupdf::{Buffer, Document, Font};
use pdf_core::compress::{
    self, ColorMode, CompressOptions, CompressReport, Compression, Preset, Progress, Removals,
};
use pdf_core::docinfo::{self, PdfA};
use pdf_core::save::{SaveKind, save_atomic};
use pdf_core::testgen::{self, SampleSpec};

use common::{out_dir, qpdf_check, qpdf_check_with, writing};

fn image_sample(dir: &Path) -> PathBuf {
    let path = dir.join("images.pdf");
    let doc = testgen::image_sample().unwrap();
    writing(|| save_atomic(&doc, SaveKind::Full, None, &path)).unwrap();
    path
}

fn run(input: &Path, out: &Path, options: &CompressOptions) -> CompressReport {
    writing(|| compress::compress_file(input, None, options, out, &mut |_| true)).unwrap()
}

/// Each page rendered at 72 dpi, as RGBA.
fn renders(path: &Path, password: Option<&str>) -> Vec<pdf_core::render::RgbaImage> {
    let mut doc = Document::open(path.to_str().unwrap()).unwrap();
    if let Some(password) = password {
        assert!(doc.authenticate(password).unwrap());
    }
    (0..doc.page_count().unwrap() as usize)
        .map(|i| {
            let list = pdf_core::render::display_list(&doc, i).unwrap();
            pdf_core::render::render_rgba(&list, 1.0, None).unwrap()
        })
        .collect()
}

/// Peak signal-to-noise ratio of `b` against `a`, in decibels (infinite if equal).
fn psnr(a: &pdf_core::render::RgbaImage, b: &pdf_core::render::RgbaImage) -> f64 {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let mut sum = 0f64;
    for (x, y) in a.data.iter().zip(&b.data) {
        let d = f64::from(*x) - f64::from(*y);
        sum += d * d;
    }
    let mse = sum / a.data.len() as f64;
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    }
}

fn open(path: &Path) -> PdfDocument {
    PdfDocument::open(path.to_str().unwrap()).unwrap()
}

/// The image object a page's resources name `name`, followed through forms, patterns and
/// annotation appearances.
fn image(doc: &PdfDocument, page: i32, path: &[&str]) -> PdfObject {
    let mut obj = doc.find_page(page).unwrap();
    for key in path {
        obj = obj
            .get_dict(*key)
            .unwrap()
            .unwrap_or_else(|| panic!("no {key} in {path:?}"));
    }
    obj
}

fn int(obj: &PdfObject, key: &str) -> i32 {
    obj.get_dict(key).unwrap().unwrap().as_int().unwrap()
}

fn name(obj: &PdfObject, key: &str) -> Option<String> {
    obj.get_dict(key)
        .unwrap()
        .filter(|v| v.is_name().unwrap())
        .map(|v| String::from_utf8_lossy(&v.as_name().unwrap()).into_owned())
}

/// Every image object in `doc` that is not a soft mask, with its object number.
fn all_images(doc: &PdfDocument) -> Vec<(i32, PdfObject)> {
    let mut smasks = Vec::new();
    let mut images = Vec::new();
    for num in 1..doc.xref_len().unwrap() as i32 {
        let obj = doc.new_indirect(num, 0).unwrap();
        if obj.is_stream().unwrap() && name(&obj, "Subtype").as_deref() == Some("Image") {
            if let Some(s) = obj.get_dict("SMask").unwrap() {
                smasks.push(s.as_indirect().unwrap());
            }
            images.push((num, obj));
        }
    }
    images.retain(|(num, _)| !smasks.contains(num));
    images
}

#[test]
fn the_lossless_preset_changes_no_pixel() {
    let dir = out_dir("compress-lossless");
    let input = image_sample(&dir);
    let out = dir.join("lossless.pdf");
    let report = run(&input, &out, &Preset::Lossless.options());
    qpdf_check(&out);
    assert!(report.after < report.before / 2, "{report:?}");
    assert_eq!(report.images.downsampled, 0);
    assert_eq!(report.fonts_subset, 1, "MuPDF's Times is subset");
    for (page, (a, b)) in renders(&input, None)
        .iter()
        .zip(renders(&out, None))
        .enumerate()
    {
        assert_eq!(a.data, b.data, "page {} changed", page + 1);
    }

    let doc = open(&out);
    // Grey stored as RGB becomes grey; black and white stored as grey, one bit a pixel.
    let gray = image(&doc, 1, &["Resources", "XObject", "GrayRgb"]);
    assert_eq!(name(&gray, "ColorSpace").as_deref(), Some("DeviceGray"));
    let bitonal = image(&doc, 1, &["Resources", "XObject", "Bitonal"]);
    assert_eq!(int(&bitonal, "BitsPerComponent"), 1);
    // JPEG images are left as they are.
    let jpeg = image(&doc, 2, &["Resources", "XObject", "Jpeg"]);
    assert_eq!(int(&jpeg, "Width"), 1200);
    assert_eq!(name(&jpeg, "Filter").as_deref(), Some("DCTDecode"));
}

#[test]
fn images_are_downsampled_to_where_they_are_drawn_largest() {
    let dir = out_dir("compress-balanced");
    let input = image_sample(&dir);
    let out = dir.join("balanced.pdf");
    let report = run(&input, &out, &Preset::Balanced.options());
    qpdf_check(&out);
    assert!(report.after * 10 < report.before, "{report:?}");
    for (page, (a, b)) in renders(&input, None)
        .iter()
        .zip(renders(&out, None))
        .enumerate()
    {
        let quality = psnr(a, &b);
        assert!(quality > 35.0, "page {}: {quality:.1} dB", page + 1);
    }

    let doc = open(&out);
    // 1200 x 900 drawn at 400 dpi, to 150 dpi.
    let photo = image(&doc, 0, &["Resources", "XObject", "Photo"]);
    assert_eq!((int(&photo, "Width"), int(&photo, "Height")), (450, 338));
    assert_eq!(name(&photo, "Filter").as_deref(), Some("DCTDecode"));
    // Inside a form scaled by half: 288 dpi.
    let formed = image(
        &doc,
        0,
        &[
            "Resources",
            "XObject",
            "Fm",
            "Resources",
            "XObject",
            "Formed",
        ],
    );
    assert_eq!(int(&formed, "Width"), 417);
    // A screenshot stays lossless.
    let screen = image(&doc, 0, &["Resources", "XObject", "Screen"]);
    assert_eq!(name(&screen, "Filter").as_deref(), Some("FlateDecode"));
    // In a pattern scaled by half: 576 dpi.
    let tiled = image(
        &doc,
        2,
        &[
            "Resources",
            "Pattern",
            "P1",
            "Resources",
            "XObject",
            "Tiled",
        ],
    );
    assert_eq!(int(&tiled, "Width"), 104);
    // In a stamp's appearance, 600 pixels in 150 pt: 288 dpi.
    let annots = doc
        .find_page(2)
        .unwrap()
        .get_dict("Annots")
        .unwrap()
        .unwrap();
    let stamp = annots.get_array(0).unwrap().unwrap();
    let ap = stamp
        .get_dict("AP")
        .unwrap()
        .unwrap()
        .get_dict("N")
        .unwrap()
        .unwrap();
    let stamp_image = ap
        .get_dict("Resources")
        .unwrap()
        .unwrap()
        .get_dict("XObject")
        .unwrap()
        .unwrap()
        .get_dict("Stamp")
        .unwrap()
        .unwrap();
    assert_eq!(int(&stamp_image, "Width"), 313);
    // Under the threshold (1.5 x 150 dpi): resolution kept, quality lowered.
    let cmyk = image(&doc, 2, &["Resources", "XObject", "Cmyk"]);
    assert_eq!(int(&cmyk, "Width"), 600);
    assert_eq!(name(&cmyk, "ColorSpace").as_deref(), Some("DeviceCMYK"));
    // The image mask stays one.
    let mask = image(&doc, 1, &["Resources", "XObject", "Mask"]);
    assert!(
        mask.get_dict("ImageMask")
            .unwrap()
            .unwrap()
            .as_bool()
            .unwrap()
    );
    assert!(mask.get_dict("ColorSpace").unwrap().is_none());
}

#[test]
fn every_preset_and_jpeg_2000_render_close_to_the_original() {
    let dir = out_dir("compress-presets");
    let input = image_sample(&dir);
    let original = renders(&input, None);
    let mut jpx = Preset::Balanced.options();
    jpx.color.compression = Compression::Jpeg2000;
    jpx.gray.compression = Compression::Jpeg2000;
    let cases = [
        ("high", Preset::HighQuality.options(), 38.0),
        ("smallest", Preset::Smallest.options(), 30.0),
        ("jpx", jpx, 33.0),
    ];
    let mut sizes = Vec::new();
    for (case, options, floor) in cases {
        let out = dir.join(format!("{case}.pdf"));
        let report = run(&input, &out, &options);
        qpdf_check(&out);
        sizes.push(report.after);
        for (page, (a, b)) in original.iter().zip(renders(&out, None)).enumerate() {
            let quality = psnr(a, &b);
            assert!(
                quality > floor,
                "{case}, page {}: {quality:.1} dB",
                page + 1
            );
        }
        if case == "jpx" {
            let doc = open(&out);
            let photo = image(&doc, 0, &["Resources", "XObject", "Photo"]);
            assert_eq!(name(&photo, "Filter").as_deref(), Some("JPXDecode"));
        }
    }
    assert!(
        sizes[1] < sizes[0],
        "Smallest is smaller than High quality: {sizes:?}"
    );
}

#[test]
fn colour_modes_turn_images_grey_or_black_and_white() {
    let dir = out_dir("compress-colour-modes");
    let input = image_sample(&dir);
    for (case, mode) in [("grey", ColorMode::Gray), ("bw", ColorMode::BlackAndWhite)] {
        let mut options = Preset::HighQuality.options();
        options.color_mode = mode;
        let out = dir.join(format!("{case}.pdf"));
        run(&input, &out, &options);
        qpdf_check(&out);
        let doc = open(&out);
        for (num, obj) in all_images(&doc) {
            if obj.get_dict("ImageMask").unwrap().is_some() {
                continue;
            }
            assert_eq!(
                name(&obj, "ColorSpace").as_deref(),
                Some("DeviceGray"),
                "{case}: image {num}"
            );
            if mode == ColorMode::BlackAndWhite {
                assert_eq!(int(&obj, "BitsPerComponent"), 1, "{case}: image {num}");
            }
        }
    }
}

#[test]
fn an_encrypted_file_stays_encrypted() {
    let dir = out_dir("compress-encrypted");
    let input = dir.join("encrypted.pdf");
    let doc = testgen::image_sample().unwrap();
    let mut options = PdfWriteOptions::default();
    options
        .set_encryption(mupdf::pdf::Encryption::Aes256)
        .set_user_password("open sesame")
        .set_owner_password("owner");
    writing(|| doc.save_with_options(input.to_str().unwrap(), options)).unwrap();

    let out = dir.join("compressed.pdf");
    assert!(matches!(
        compress::compress_file(&input, None, &Preset::Balanced.options(), &out, &mut |_| {
            true
        }),
        Err(pdf_core::Error::PasswordRequired)
    ));
    let report = writing(|| {
        compress::compress_file(
            &input,
            Some("open sesame"),
            &Preset::Balanced.options(),
            &out,
            &mut |_| true,
        )
    })
    .unwrap();
    assert!(report.after < report.before);
    qpdf_check_with(&out, Some("open sesame"));
    let mut doc = open(&out);
    assert!(doc.needs_password().unwrap());
    assert!(doc.authenticate("open sesame").unwrap());
    assert_eq!(renders(&out, Some("open sesame")).len(), 3);
}

#[test]
fn cancelling_writes_nothing() {
    let dir = out_dir("compress-cancel");
    let input = image_sample(&dir);
    for stop_at in ["reading", "images", "writing"] {
        let out = dir.join(format!("cancelled-{stop_at}.pdf"));
        let _ = std::fs::remove_file(&out);
        let result = writing(|| {
            compress::compress_file(&input, None, &Preset::Balanced.options(), &out, &mut |p| {
                !matches!(
                    (stop_at, p),
                    ("reading", Progress::Reading { done: 1, .. })
                        | ("images", Progress::Images { done: 3, .. })
                        | ("writing", Progress::Writing)
                )
            })
        });
        assert!(
            matches!(result, Err(pdf_core::Error::Cancelled)),
            "{stop_at}"
        );
        assert!(!out.exists(), "{stop_at}");
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".lectrix-tmp")
            })
            .count();
        assert_eq!(leftovers, 0, "{stop_at}");
    }
}

#[test]
fn the_pdfa_claim_goes() {
    let dir = out_dir("compress-pdfa");
    let input = common::sample_file(
        &dir,
        "pdfa.pdf",
        SampleSpec {
            pdfa: Some(PdfA {
                part: 2,
                conformance: Some('B'),
            }),
            ..SampleSpec::default()
        },
    );
    let out = dir.join("compressed.pdf");
    let report = run(&input, &out, &Preset::Lossless.options());
    assert_eq!(
        report.pdfa_removed,
        Some(PdfA {
            part: 2,
            conformance: Some('B')
        })
    );
    assert_eq!(docinfo::read_pdfa(&open(&out)), None);
}

/// A sample with a thumbnail, private data, an attachment, scripts and tags.
fn with_extras(dir: &Path) -> PathBuf {
    let mut doc = testgen::sample_document(&SampleSpec::default()).unwrap();
    let stream = |doc: &mut PdfDocument, dict: &str, data: &[u8]| {
        let dict = doc.new_object_from_str(dict).unwrap();
        doc.add_stream(
            &Buffer::from_copied_bytes(data).unwrap(),
            Some(&dict),
            false,
        )
        .unwrap()
    };
    let thumb = stream(
        &mut doc,
        "<< /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 >>",
        &[0; 12],
    );
    let private = stream(&mut doc, "<< >>", &[7; 5000]);
    let attached = stream(&mut doc, "<< /Type /EmbeddedFile >>", b"attached text");
    let mut page = doc.find_page(0).unwrap();
    page.dict_put("Thumb", thumb).unwrap();
    let piece = doc
        .new_object_from_str("<< /Illustrator << /LastModified (D:20260101) >> >>")
        .unwrap();
    piece
        .get_dict("Illustrator")
        .unwrap()
        .unwrap()
        .dict_put("Private", private)
        .unwrap();
    page.dict_put("PieceInfo", piece).unwrap();
    let mut catalog = doc.catalog().unwrap();
    let names = doc
        .new_object_from_str(
            "<< /EmbeddedFiles << /Names [(a.txt) << /Type /Filespec /F (a.txt) /EF << >> >>] >> \
             /JavaScript << /Names [(hello) << /S /JavaScript /JS (app.alert(1)) >>] >> >>",
        )
        .unwrap();
    names
        .get_dict("EmbeddedFiles")
        .unwrap()
        .unwrap()
        .get_dict("Names")
        .unwrap()
        .unwrap()
        .get_array(1)
        .unwrap()
        .unwrap()
        .get_dict("EF")
        .unwrap()
        .unwrap()
        .dict_put("F", attached)
        .unwrap();
    catalog.dict_put("Names", names).unwrap();
    catalog
        .dict_put(
            "OpenAction",
            doc.new_object_from_str("<< /S /JavaScript /JS (this.print()) >>")
                .unwrap(),
        )
        .unwrap();
    catalog
        .dict_put(
            "StructTreeRoot",
            doc.new_object_from_str("<< /Type /StructTreeRoot >>")
                .unwrap(),
        )
        .unwrap();
    catalog
        .dict_put(
            "MarkInfo",
            doc.new_object_from_str("<< /Marked true >>").unwrap(),
        )
        .unwrap();
    let path = dir.join("extras.pdf");
    writing(|| save_atomic(&doc, SaveKind::Full, None, &path)).unwrap();
    path
}

#[test]
fn removals_are_found_and_removed() {
    let dir = out_dir("compress-removals");
    let input = with_extras(&dir);
    let found = compress::analyze(&input, None).unwrap();
    assert_eq!(found.found.thumbnails, 1);
    assert_eq!(found.found.private_data, 1);
    assert_eq!(found.found.attachments, 1);
    assert_eq!(found.found.javascript, 2);
    assert!(found.found.tags);
    assert_eq!(found.attachments.count, 1);
    assert_eq!(found.thumbnails.count, 1);
    assert_eq!(found.private_data.count, 1);
    let parts = [
        found.images,
        found.fonts,
        found.content,
        found.attachments,
        found.thumbnails,
        found.private_data,
        found.metadata,
    ];
    assert_eq!(
        parts.iter().map(|u| u.bytes).sum::<u64>() + found.other,
        found.file_size
    );

    // The preset's removals: thumbnails and private data only.
    let out = dir.join("preset.pdf");
    run(&input, &out, &Preset::Lossless.options());
    let after = compress::analyze(&out, None).unwrap().found;
    assert_eq!((after.thumbnails, after.private_data), (0, 0));
    assert_eq!(
        (after.attachments, after.javascript, after.tags),
        (1, 2, true)
    );

    let mut options = Preset::Lossless.options();
    options.remove = Removals {
        thumbnails: true,
        private_data: true,
        attachments: true,
        javascript: true,
        tags: true,
    };
    let out = dir.join("all.pdf");
    let report = run(&input, &out, &options);
    qpdf_check(&out);
    assert_eq!(report.removed.thumbnails, 1);
    assert_eq!(report.removed.private_data, 1);
    assert_eq!(report.removed.attachments, 1);
    assert_eq!(report.removed.javascript, 2);
    assert!(report.removed.tags);
    let after = compress::analyze(&out, None).unwrap();
    assert_eq!(after.found, compress::Found::default());
    assert_eq!(after.attachments.count, 0);
}

#[test]
fn the_forms_fonts_are_kept_whole() {
    let dir = out_dir("compress-form-fonts");
    let mut doc = testgen::sample_document(&SampleSpec::default()).unwrap();
    let times = Font::new("Times-Roman").unwrap();
    let courier = Font::new("Courier").unwrap();
    let form_font = doc.add_font(&times).unwrap();
    let page_font = doc.add_font(&courier).unwrap();
    let glyphs = |font: &Font| -> String {
        "Lectrix"
            .chars()
            .map(|c| format!("{:04X}", font.encode_character(c as i32).unwrap()))
            .collect()
    };
    // Both are shown on page 1; only Times is in the form's resources.
    let content = format!(
        "BT /TiRo 12 Tf 72 100 Td <{}> Tj /Cour 12 Tf 0 -20 Td <{}> Tj ET",
        glyphs(&times),
        glyphs(&courier)
    );
    let stream = doc
        .add_stream(
            &Buffer::from_copied_bytes(content.as_bytes()).unwrap(),
            None,
            false,
        )
        .unwrap();
    let mut page = doc.find_page(0).unwrap();
    let mut contents = doc.new_array().unwrap();
    contents
        .array_push(page.get_dict("Contents").unwrap().unwrap())
        .unwrap();
    contents.array_push(stream).unwrap();
    page.dict_put("Contents", contents).unwrap();
    let mut fonts = page
        .get_dict("Resources")
        .unwrap()
        .unwrap()
        .get_dict("Font")
        .unwrap()
        .unwrap();
    fonts.dict_put("TiRo", form_font.clone()).unwrap();
    fonts.dict_put("Cour", page_font).unwrap();
    let form = doc
        .new_object_from_str("<< /Fields [] /DA (/TiRo 0 Tf 0 g) /DR << /Font << >> >> >>")
        .unwrap();
    form.get_dict("DR")
        .unwrap()
        .unwrap()
        .get_dict("Font")
        .unwrap()
        .unwrap()
        .dict_put("TiRo", form_font)
        .unwrap();
    doc.catalog().unwrap().dict_put("AcroForm", form).unwrap();
    let input = dir.join("form.pdf");
    writing(|| save_atomic(&doc, SaveKind::Full, None, &input)).unwrap();

    let program = |path: &Path, name: &str| -> Vec<u8> {
        let doc = open(path);
        let font = image(&doc, 0, &["Resources", "Font", name]);
        let descendant = font
            .get_dict("DescendantFonts")
            .unwrap()
            .unwrap()
            .get_array(0)
            .unwrap()
            .unwrap();
        let descriptor = descendant.get_dict("FontDescriptor").unwrap().unwrap();
        ["FontFile", "FontFile2", "FontFile3"]
            .iter()
            .find_map(|k| descriptor.get_dict(*k).unwrap())
            .unwrap()
            .read_stream()
            .unwrap()
    };
    let out = dir.join("compressed.pdf");
    let report = run(&input, &out, &Preset::Lossless.options());
    qpdf_check(&out);
    assert_eq!(report.fonts_subset, 1, "{report:?}");
    assert_eq!(
        program(&out, "TiRo"),
        program(&input, "TiRo"),
        "the form's font is whole"
    );
    assert!(program(&out, "Cour").len() < program(&input, "Cour").len());
    let (a, b) = (renders(&input, None), renders(&out, None));
    assert_eq!(a[0].data, b[0].data);
}

#[test]
fn duplicate_objects_are_merged() {
    // Two pages, each with its own copy of the same image (as files combined from copies
    // of one document have).
    let dir = out_dir("compress-duplicates");
    let mut doc = testgen::sample_document(&SampleSpec {
        pages: 2,
        ..SampleSpec::default()
    })
    .unwrap();
    let pixels: Vec<u8> = (0..64 * 64 * 3).map(|i| (i * 7 % 251) as u8).collect();
    for page in 0..2 {
        let dict = doc
            .new_object_from_str(
                "<< /Type /XObject /Subtype /Image /Width 64 /Height 64 /ColorSpace /DeviceRGB /BitsPerComponent 8 >>",
            )
            .unwrap();
        let image = doc
            .add_stream(
                &Buffer::from_copied_bytes(&pixels).unwrap(),
                Some(&dict),
                false,
            )
            .unwrap();
        let content = doc
            .add_stream(
                &Buffer::from_copied_bytes(b"q 64 0 0 64 300 300 cm /Logo Do Q").unwrap(),
                None,
                false,
            )
            .unwrap();
        let mut page_obj = doc.find_page(page).unwrap();
        let mut contents = doc.new_array().unwrap();
        contents
            .array_push(page_obj.get_dict("Contents").unwrap().unwrap())
            .unwrap();
        contents.array_push(content).unwrap();
        page_obj.dict_put("Contents", contents).unwrap();
        let mut resources = page_obj.get_dict("Resources").unwrap().unwrap();
        let mut xobjects = doc.new_dict().unwrap();
        xobjects.dict_put("Logo", image).unwrap();
        resources.dict_put("XObject", xobjects).unwrap();
    }
    let input = dir.join("copies.pdf");
    writing(|| save_atomic(&doc, SaveKind::Full, None, &input)).unwrap();

    let out = dir.join("merged.pdf");
    run(&input, &out, &Preset::Lossless.options());
    qpdf_check(&out);
    let doc = open(&out);
    let logo = |page| {
        image(&doc, page, &["Resources", "XObject", "Logo"])
            .as_indirect()
            .unwrap()
    };
    assert_eq!(logo(0), logo(1), "both pages draw one image object");
    for (page, (a, b)) in renders(&input, None)
        .iter()
        .zip(renders(&out, None))
        .enumerate()
    {
        assert_eq!(a.data, b.data, "page {} changed", page + 1);
    }
}

#[test]
fn options_out_of_range_are_refused() {
    let mut options = Preset::Balanced.options();
    options.color.quality = 0;
    assert!(compress::validate(&options).is_err());
    let mut options = Preset::Balanced.options();
    if let Some(d) = options.color.downsample.as_mut() {
        d.above_dpi = d.to_dpi - 1;
    }
    assert!(compress::validate(&options).is_err());
    for preset in [
        Preset::Lossless,
        Preset::HighQuality,
        Preset::Balanced,
        Preset::Smallest,
    ] {
        compress::validate(&preset.options()).unwrap();
    }
}
