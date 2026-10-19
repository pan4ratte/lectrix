//! Compress's calls into MuPDF and libjpeg (`compress.c`): where images are drawn, decoding
//! them, the encoders, and font subsetting (section 6.11, ADR 0027).

use std::ffi::{c_int, c_uchar, c_void};

use mupdf::Context;
use mupdf::pdf::PdfDocument;
use mupdf_sys::{fz_context, fz_pixmap, pdf_document};

use super::{LectrixError, check};
use crate::error::{Error, Result};

type Found = extern "C" fn(opaque: *mut c_void, num: c_int, dpi: f32);

unsafe extern "C" {
    fn lectrix_walk_page_images(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        page: c_int,
        found: Found,
        opaque: *mut c_void,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_image_decode(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        num: c_int,
        out: *mut *mut fz_pixmap,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pixmap_info(
        ctx: *mut fz_context,
        pix: *const fz_pixmap,
        w: *mut c_int,
        h: *mut c_int,
        n: *mut c_int,
        alpha: *mut c_int,
        stride: *mut isize,
        samples: *mut *mut c_uchar,
    ) -> c_int;
    fn lectrix_pixmap_to_gray(
        ctx: *mut fz_context,
        pix: *mut fz_pixmap,
        out: *mut *mut fz_pixmap,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_drop_pixmap(ctx: *mut fz_context, pix: *mut fz_pixmap);
    fn lectrix_free(p: *mut c_void);
    fn lectrix_encode_jpeg(
        samples: *const c_uchar,
        w: c_int,
        h: c_int,
        n: c_int,
        stride: isize,
        quality: c_int,
        subsample: c_int,
        out: *mut *mut c_uchar,
        out_len: *mut usize,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_encode_jpx(
        ctx: *mut fz_context,
        samples: *const c_uchar,
        w: c_int,
        h: c_int,
        n: c_int,
        stride: isize,
        quality: c_int,
        out: *mut *mut c_uchar,
        out_len: *mut usize,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_encode_g4(
        ctx: *mut fz_context,
        bits: *const c_uchar,
        w: c_int,
        h: c_int,
        stride: isize,
        out: *mut *mut c_uchar,
        out_len: *mut usize,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_deflate(
        ctx: *mut fz_context,
        data: *const c_uchar,
        len: usize,
        level: c_int,
        out: *mut *mut c_uchar,
        out_len: *mut usize,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_subset_fonts(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        err: *mut LectrixError,
    ) -> c_int;
}

extern "C" fn found_trampoline(opaque: *mut c_void, num: c_int, dpi: f32) {
    // SAFETY: `opaque` is the `&mut dyn FnMut` that `walk_page_images` passed, alive for
    // the whole call that reports through it.
    let found = unsafe { &mut *(opaque as *mut &mut dyn FnMut(i32, f32)) };
    found(num, dpi);
}

/// Calls `found` with (object number, pixels per inch) for each image drawn on page
/// `page`, its soft mask and its stencil mask: by the page's contents, through forms,
/// patterns and soft masks, and by its annotations' and widgets' appearances. Changes
/// nothing in `doc`.
pub fn walk_page_images(
    doc: &PdfDocument,
    page: i32,
    found: &mut dyn FnMut(i32, f32),
) -> Result<()> {
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    let mut found = found;
    // SAFETY: `ctx` is this thread's context and `doc` belongs to this thread; the callback
    // and its `opaque` (a pointer to `found`) are only used during the call, and the
    // callback does not unwind (an `extern "C"` function aborts on a panic).
    let rc = unsafe {
        lectrix_walk_page_images(
            ctx,
            doc.as_raw_pdf_ptr(),
            page,
            found_trampoline,
            &mut found as *mut &mut dyn FnMut(i32, f32) as *mut c_void,
            &mut err,
        )
    };
    check(rc, err)
}

/// A colour space as decoded samples have it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelSpace {
    /// No colour space: an image mask's coverage.
    None,
    Gray,
    Rgb,
    Cmyk,
    /// Lab, separations, DeviceN and the like.
    Other,
}

/// An image decoded by MuPDF: its own size, `/Decode` applied, Indexed expanded.
pub struct Decoded {
    ptr: *mut fz_pixmap,
    pub width: usize,
    pub height: usize,
    /// Components per pixel, alpha included.
    pub n: usize,
    pub alpha: bool,
    pub space: PixelSpace,
    stride: usize,
    samples: *const u8,
}

impl Decoded {
    fn wrap(ptr: *mut fz_pixmap) -> Result<Decoded> {
        let ctx = Context::get().as_raw_ptr();
        let (mut w, mut h, mut n, mut alpha) = (0, 0, 0, 0);
        let mut stride = 0isize;
        let mut samples = std::ptr::null_mut();
        // SAFETY: `ptr` is a live pixmap this thread's context made; the out-pointers are
        // live locals.
        let space = unsafe {
            lectrix_pixmap_info(
                ctx,
                ptr,
                &mut w,
                &mut h,
                &mut n,
                &mut alpha,
                &mut stride,
                &mut samples,
            )
        };
        let decoded = Decoded {
            ptr,
            width: usize::try_from(w).unwrap_or(0),
            height: usize::try_from(h).unwrap_or(0),
            n: usize::try_from(n).unwrap_or(0),
            alpha: alpha != 0,
            space: match space {
                0 => PixelSpace::None,
                1 => PixelSpace::Gray,
                2 => PixelSpace::Rgb,
                3 => PixelSpace::Cmyk,
                _ => PixelSpace::Other,
            },
            stride: usize::try_from(stride).unwrap_or(0),
            samples,
        };
        if samples.is_null() || decoded.stride < decoded.width * decoded.n {
            return Err(Error::InvalidArgument(
                "a decoded image has no samples".into(),
            ));
        }
        Ok(decoded)
    }

    /// Row `y`: `width * n` bytes.
    pub fn row(&self, y: usize) -> &[u8] {
        assert!(y < self.height, "row {y} of {}", self.height);
        // SAFETY: the pixmap holds `height` rows of `stride` bytes, each starting with
        // `width * n` samples (checked in `wrap`), and lives as long as `self`.
        unsafe {
            std::slice::from_raw_parts(self.samples.add(y * self.stride), self.width * self.n)
        }
    }

    /// The samples as one buffer of `height` rows of `width * n` bytes.
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.width * self.n * self.height);
        for y in 0..self.height {
            out.extend_from_slice(self.row(y));
        }
        out
    }

    /// The same image in device grey, through MuPDF's colour management.
    pub fn to_gray(&self) -> Result<Decoded> {
        let ctx = Context::get().as_raw_ptr();
        let mut out = std::ptr::null_mut();
        let mut err = LectrixError::new();
        // SAFETY: `self.ptr` is live; `out` and `err` are live locals.
        let rc = unsafe { lectrix_pixmap_to_gray(ctx, self.ptr, &mut out, &mut err) };
        check(rc, err)?;
        Decoded::wrap(out)
    }
}

impl Drop for Decoded {
    fn drop(&mut self) {
        let ctx = Context::get().as_raw_ptr();
        // SAFETY: `ptr` is a pixmap this thread made and nothing else holds; dropping it
        // does not throw.
        unsafe { lectrix_drop_pixmap(ctx, self.ptr) };
    }
}

/// Decodes image object `num` of `doc`.
pub fn decode_image(doc: &PdfDocument, num: i32) -> Result<Decoded> {
    let ctx = Context::get().as_raw_ptr();
    let mut out = std::ptr::null_mut();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context and `doc` belongs to this thread; `out` and
    // `err` are live locals.
    let rc = unsafe { lectrix_image_decode(ctx, doc.as_raw_pdf_ptr(), num, &mut out, &mut err) };
    check(rc, err)?;
    Decoded::wrap(out)
}

/// Takes memory an encoder made with malloc.
fn take(rc: c_int, err: LectrixError, out: *mut c_uchar, len: usize) -> Result<Vec<u8>> {
    let result = check(rc, err).map(|()| {
        if out.is_null() {
            Vec::new()
        } else {
            // SAFETY: on success the encoder returns `len` bytes at `out`.
            unsafe { std::slice::from_raw_parts(out, len) }.to_vec()
        }
    });
    // SAFETY: `out` is null or memory from malloc that nothing else holds.
    unsafe { lectrix_free(out.cast()) };
    result
}

fn dims(width: usize, height: usize, n: usize, len: usize) -> Result<(c_int, c_int, c_int)> {
    let bad = || Error::InvalidArgument("image samples do not match their size".into());
    if width == 0 || height == 0 || len < width * height * n {
        return Err(bad());
    }
    Ok((
        c_int::try_from(width).map_err(|_| bad())?,
        c_int::try_from(height).map_err(|_| bad())?,
        c_int::try_from(n).map_err(|_| bad())?,
    ))
}

/// Encodes `samples` (`height` rows of `width * n` bytes; `n` = 1, 3 or 4) as a baseline
/// JPEG at `quality` (1-100), RGB with its colour halved both ways if `subsample`.
pub fn encode_jpeg(
    samples: &[u8],
    width: usize,
    height: usize,
    n: usize,
    quality: u8,
    subsample: bool,
) -> Result<Vec<u8>> {
    let (w, h, n_) = dims(width, height, n, samples.len())?;
    let mut out = std::ptr::null_mut();
    let mut len = 0;
    let mut err = LectrixError::new();
    // SAFETY: `samples` holds `height` rows of `width * n` bytes (checked by `dims`) and is
    // only read; the out-pointers are live locals.
    let rc = unsafe {
        lectrix_encode_jpeg(
            samples.as_ptr(),
            w,
            h,
            n_,
            (width * n) as isize,
            c_int::from(quality),
            c_int::from(subsample),
            &mut out,
            &mut len,
            &mut err,
        )
    };
    take(rc, err, out, len)
}

/// Encodes `samples` as JPEG 2000 at `quality` (MuPDF's: (100 - quality) : 1).
pub fn encode_jpx(
    samples: &[u8],
    width: usize,
    height: usize,
    n: usize,
    quality: u8,
) -> Result<Vec<u8>> {
    let (w, h, n_) = dims(width, height, n, samples.len())?;
    let ctx = Context::get().as_raw_ptr();
    let mut out = std::ptr::null_mut();
    let mut len = 0;
    let mut err = LectrixError::new();
    // SAFETY: as for `encode_jpeg`; `ctx` is this thread's context. The pixmap MuPDF wraps
    // around `samples` only reads them and is dropped before the call returns.
    let rc = unsafe {
        lectrix_encode_jpx(
            ctx,
            samples.as_ptr(),
            w,
            h,
            n_,
            (width * n) as isize,
            c_int::from(quality),
            &mut out,
            &mut len,
            &mut err,
        )
    };
    take(rc, err, out, len)
}

/// Encodes a 1-bit image as CCITT Group 4: rows of `width.div_ceil(8)` bytes, first pixel
/// in the high bit, 0 for black. Decoded with `/BlackIs1 false`, the bits come back as
/// they are.
pub fn encode_g4(bits: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let stride = width.div_ceil(8);
    let (_, h, _) = dims(stride, height, 1, bits.len())?;
    let w = c_int::try_from(width).map_err(|_| Error::InvalidArgument("image too wide".into()))?;
    let ctx = Context::get().as_raw_ptr();
    let mut out = std::ptr::null_mut();
    let mut len = 0;
    let mut err = LectrixError::new();
    // SAFETY: `bits` holds `height` rows of `stride` bytes (checked by `dims`) and is only
    // read; the out-pointers are live locals.
    let rc = unsafe {
        lectrix_encode_g4(
            ctx,
            bits.as_ptr(),
            w,
            h,
            stride as isize,
            &mut out,
            &mut len,
            &mut err,
        )
    };
    take(rc, err, out, len)
}

/// Compresses `data` with zlib (Flate) at `level` (1-9).
pub fn deflate(data: &[u8], level: u8) -> Result<Vec<u8>> {
    let ctx = Context::get().as_raw_ptr();
    let mut out = std::ptr::null_mut();
    let mut len = 0;
    let mut err = LectrixError::new();
    // SAFETY: `data` is only read; the out-pointers are live locals.
    let rc = unsafe {
        lectrix_deflate(
            ctx,
            data.as_ptr(),
            data.len(),
            c_int::from(level.clamp(1, 9)),
            &mut out,
            &mut len,
            &mut err,
        )
    };
    take(rc, err, out, len)
}

/// Subsets `doc`'s embedded TrueType and CFF fonts to the glyphs it shows.
pub fn subset_fonts(doc: &mut PdfDocument) -> Result<()> {
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context and `doc` belongs to this thread (and is
    // borrowed mutably, as MuPDF changes it).
    let rc = unsafe { lectrix_pdf_subset_fonts(ctx, doc.as_raw_pdf_ptr(), &mut err) };
    check(rc, err)
}
