//! JPEG 2000 images drawn smaller than they are decode at a reduced resolution (Folio's
//! `mupdf-sys` fork, ADR 0008), and still look like the full decode, scaled down.

mod common;

use std::path::{Path, PathBuf};

use common::out_dir;
use pdf_core::session::Session;

const FIXTURE: &str = "tests/fixtures/quadrants-1024.jp2";

/// A one-page PDF, 256 pt square, filled by the 1024 × 1024 fixture: red, green, blue and
/// light grey quadrants with gentle gradients.
fn jpx_pdf(dir: &Path) -> PathBuf {
    let jp2 = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)).unwrap();
    let content = b"q 256 0 0 256 0 0 cm /Im0 Do Q";
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 256 256] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
    ];
    let mut image = format!(
        "<< /Type /XObject /Subtype /Image /Width 1024 /Height 1024 /ColorSpace /DeviceRGB \
         /BitsPerComponent 8 /Filter /JPXDecode /Length {} >>\nstream\n",
        jp2.len()
    )
    .into_bytes();
    image.extend_from_slice(&jp2);
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
    let path = dir.join("jpx.pdf");
    std::fs::write(&path, out).unwrap();
    path
}

/// The RGB of the pixel at (fx, fy), fractions of the image's width and height.
fn pixel(img: &pdf_core::render::RgbaImage, fx: f32, fy: f32) -> [u8; 3] {
    let x = ((img.width as f32 * fx) as usize).min(img.width as usize - 1);
    let y = ((img.height as f32 * fy) as usize).min(img.height as usize - 1);
    let i = (y * img.width as usize + x) * 4;
    [img.data[i], img.data[i + 1], img.data[i + 2]]
}

fn close(a: [u8; 3], b: [u8; 3], tolerance: i32) -> bool {
    a.iter()
        .zip(b)
        .all(|(&x, y)| (x as i32 - y as i32).abs() <= tolerance)
}

#[test]
fn small_renders_of_a_jpeg_2000_image_match_the_full_decode() {
    let dir = out_dir("jpx");
    let (session, _) = Session::open(&jpx_pdf(&dir), None).unwrap();

    // 64 and 128 px wide: MuPDF asks for a decode 8 and 4 times smaller. 1024 px: in full.
    let full = session.render_rgba(0, 4.0, None).unwrap().0;
    assert_eq!((full.width, full.height), (1024, 1024));
    for scale in [0.25f32, 0.5] {
        let small = session.render_rgba(0, scale, None).unwrap().0;
        let side = (256.0 * scale) as u32;
        assert_eq!((small.width, small.height), (side, side), "scale {scale}");
        // Every quadrant shows its own colour, where the full decode shows it.
        for (fx, fy) in [
            (0.25, 0.25),
            (0.75, 0.25),
            (0.25, 0.75),
            (0.75, 0.75),
            (0.1, 0.9),
            (0.9, 0.1),
        ] {
            let (s, f) = (pixel(&small, fx, fy), pixel(&full, fx, fy));
            assert!(
                close(s, f, 12),
                "scale {scale} at ({fx}, {fy}): {s:?} vs full {f:?}"
            );
        }
        // Overall, the small render is the full one scaled down.
        let step = full.width / small.width;
        let mut total = 0u64;
        for y in 0..small.height {
            for x in 0..small.width {
                let s = pixel(
                    &small,
                    (x as f32 + 0.5) / side as f32,
                    (y as f32 + 0.5) / side as f32,
                );
                let f = pixel(
                    &full,
                    ((x * step) as f32 + step as f32 / 2.0) / 1024.0,
                    ((y * step) as f32 + step as f32 / 2.0) / 1024.0,
                );
                total += s
                    .iter()
                    .zip(f)
                    .map(|(&a, b)| (a as i32 - b as i32).unsigned_abs() as u64)
                    .sum::<u64>();
            }
        }
        let mean = total as f64 / (side * side * 3) as f64;
        assert!(mean < 6.0, "scale {scale}: mean difference {mean:.2}/255");
    }
}
