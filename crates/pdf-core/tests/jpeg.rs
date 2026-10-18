//! JPEG images are decoded by libjpeg-turbo, with its SIMD code, and rows wider than the
//! JPEG filter's 4 KB pieces come out whole (Lectrix's `mupdf-sys` fork, ADR 0024).

mod common;

use std::ffi::c_int;
use std::path::{Path, PathBuf};

use common::out_dir;
use pdf_core::session::Session;

/// 3000 × 48 pixels, RGB (9,000 bytes a row): red, green, blue and light grey bands, each
/// a quarter of the width.
const FIXTURE: &str = "tests/fixtures/bands-3000x48.jpg";
const BANDS: [[u8; 3]; 4] = [[220, 40, 40], [40, 180, 60], [40, 70, 220], [200, 200, 200]];

/// A one-page PDF, 3000 × 48 pt, filled by the fixture; `crop` is the page's CropBox.
fn jpeg_pdf(dir: &Path, name: &str, crop: [u32; 4]) -> PathBuf {
    let jpeg = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)).unwrap();
    let content = b"q 3000 0 0 48 0 0 cm /Im0 Do Q";
    let [x0, y0, x1, y1] = crop;
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 3000 48] /CropBox [{x0} {y0} {x1} {y1}] \
             /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        )
        .into_bytes(),
    ];
    let mut image = format!(
        "<< /Type /XObject /Subtype /Image /Width 3000 /Height 48 /ColorSpace /DeviceRGB \
         /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        jpeg.len()
    )
    .into_bytes();
    image.extend_from_slice(&jpeg);
    image.extend_from_slice(b"\nendstream");
    objects.push(image);
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    objects.push(stream);

    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
}

/// The RGB of the pixel at (x, y).
fn pixel(img: &pdf_core::render::RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * img.width + x) * 4) as usize;
    [img.data[i], img.data[i + 1], img.data[i + 2]]
}

fn close(a: [u8; 3], b: [u8; 3], tolerance: i32) -> bool {
    a.iter()
        .zip(b)
        .all(|(&x, y)| (x as i32 - y as i32).abs() <= tolerance)
}

#[test]
fn a_jpeg_wider_than_the_filters_pieces_decodes_in_place() {
    let dir = out_dir("jpeg");

    // The whole image, at its own size: every band where it belongs, along every row.
    let (session, _) = Session::open(&jpeg_pdf(&dir, "whole.pdf", [0, 0, 3000, 48]), None).unwrap();
    let img = session.render_rgba(0, 1.0, None, &[]).unwrap().0;
    assert_eq!((img.width, img.height), (3000, 48));
    for (band, colour) in BANDS.iter().enumerate() {
        let x = band as u32 * 750 + 375;
        for y in [2, 24, 45] {
            let p = pixel(&img, x, y);
            assert!(
                close(p, *colour, 12),
                "band {band} at ({x}, {y}): {p:?}, not {colour:?}"
            );
        }
    }

    // Only the third band shown: MuPDF decodes just those columns of each row.
    let (session, _) =
        Session::open(&jpeg_pdf(&dir, "third.pdf", [1500, 0, 2250, 48]), None).unwrap();
    let img = session.render_rgba(0, 1.0, None, &[]).unwrap().0;
    assert_eq!((img.width, img.height), (750, 48));
    for x in [10, 375, 740] {
        let p = pixel(&img, x, 24);
        assert!(close(p, BANDS[2], 12), "third band at {x}: {p:?}");
    }
}

// SAFETY (for the calls below): libjpeg-turbo's own probes, which take nothing and only
// read the CPU's features.
unsafe extern "C" {
    fn jsimd_can_idct_islow() -> c_int;
    fn jsimd_can_ycc_rgb() -> c_int;
}

/// libjpeg-turbo builds without its SIMD code when NASM is missing (x86) and says so only
/// in a build-script warning cargo does not show; the decoder is then about as slow as
/// MuPDF's own. This catches such a build.
#[test]
#[cfg(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"))]
fn libjpeg_turbo_has_its_simd_code() {
    // SAFETY: see the declarations.
    let (idct, colour) = unsafe { (jsimd_can_idct_islow(), jsimd_can_ycc_rgb()) };
    assert!(
        idct != 0 && colour != 0,
        "libjpeg-turbo was built without SIMD: install NASM and put it on PATH (CONTRIBUTING.md), \
         then `cargo clean -p mozjpeg-sys` and build again"
    );
}
