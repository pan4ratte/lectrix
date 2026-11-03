//! The calls into MuPDF behind exporting the images in a document (`images.c`; section
//! 6.14, ADR 0035): an image loaded by object number, the facts that decide whether its
//! own bytes can be copied, those bytes and its colour profile, and its pixels with its
//! mask as alpha.

use std::ffi::{c_int, c_uchar};

use mupdf::Context;
use mupdf::pdf::PdfDocument;
use mupdf_sys::{fz_context, fz_image, fz_pixmap, pdf_document};

use super::compress::Decoded;
use super::{LectrixError, check};
use crate::error::{Error, Result};

#[repr(C)]
#[derive(Default)]
struct RawFacts {
    w: c_int,
    h: c_int,
    bpc: c_int,
    n: c_int,
    imagemask: c_int,
    has_mask: c_int,
    use_decode: c_int,
    use_colorkey: c_int,
    decode_inverted: c_int,
    colorspace: c_int,
    compressed: c_int,
    color_transform: c_int,
    compressed_len: usize,
}

unsafe extern "C" {
    fn lectrix_image_load(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        num: c_int,
        out: *mut *mut fz_image,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_image_drop(ctx: *mut fz_context, image: *mut fz_image);
    fn lectrix_image_facts_of(ctx: *mut fz_context, image: *mut fz_image, facts: *mut RawFacts);
    fn lectrix_image_compressed(
        ctx: *mut fz_context,
        image: *mut fz_image,
        len: *mut usize,
    ) -> *const c_uchar;
    fn lectrix_image_icc(
        ctx: *mut fz_context,
        image: *mut fz_image,
        len: *mut usize,
    ) -> *const c_uchar;
    fn lectrix_image_pixels(
        ctx: *mut fz_context,
        image: *mut fz_image,
        max_side: c_int,
        out: *mut *mut fz_pixmap,
        err: *mut LectrixError,
    ) -> c_int;
}

/// An image's colour space, as MuPDF names its kinds (`enum fz_colorspace_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceKind {
    /// None: a stencil mask.
    None,
    Gray,
    Rgb,
    Bgr,
    Cmyk,
    Lab,
    Indexed,
    /// Separation and DeviceN.
    Separation,
}

/// The data an image keeps as the file has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compressed {
    Jpeg,
    Jpeg2000,
}

/// What MuPDF made of an image's dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageFacts {
    /// Its size as a PNG: its own, or its mask's if that has more pixels.
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    /// Colour components (1 for a stencil mask).
    pub components: u8,
    /// A stencil mask (`/ImageMask true`), painted in the colour it is drawn with.
    pub image_mask: bool,
    /// It has a soft mask or a stencil mask (an image).
    pub has_mask: bool,
    /// Its `/Decode` is not the default.
    pub use_decode: bool,
    /// It has a colour key mask (`/Mask` as an array).
    pub use_colorkey: bool,
    /// Its `/Decode` is `[1 0]` for every component.
    pub decode_inverted: bool,
    pub space: SpaceKind,
    /// Its JPEG or JPEG 2000 data, if it is kept as such.
    pub compressed: Option<Compressed>,
    /// A JPEG's `/ColorTransform` (in `/DecodeParms`), if the file gives one.
    pub color_transform: Option<i32>,
    /// How long the compressed data is, bytes.
    pub compressed_len: usize,
}

/// An image of a document, loaded by MuPDF with its data (decoded only when asked for).
pub struct LoadedImage {
    ptr: *mut fz_image,
}

// SAFETY: a loaded image holds its data in memory and no longer reads its document; MuPDF
// keeps images, their colour spaces and its store safe to share between threads that use
// contexts cloned from one another (it decodes the images of display lists on rendering
// threads the same way, ADR 0001), and each call here uses its own thread's context. The
// image is used by one thread at a time (it is not `Sync`).
unsafe impl Send for LoadedImage {}

impl LoadedImage {
    /// Loads image object `num` of `doc`; fails for an object that is not an image.
    pub fn load(doc: &PdfDocument, num: i32) -> Result<LoadedImage> {
        let ctx = Context::get().as_raw_ptr();
        let mut out = std::ptr::null_mut();
        let mut err = LectrixError::new();
        // SAFETY: `ctx` is this thread's context and `doc` belongs to this thread; `out` and
        // `err` are live locals.
        let rc = unsafe { lectrix_image_load(ctx, doc.as_raw_pdf_ptr(), num, &mut out, &mut err) };
        check(rc, err)?;
        if out.is_null() {
            return Err(Error::InvalidArgument(format!("object {num} has no image")));
        }
        Ok(LoadedImage { ptr: out })
    }

    pub fn facts(&self) -> ImageFacts {
        let ctx = Context::get().as_raw_ptr();
        let mut raw = RawFacts::default();
        // SAFETY: `self.ptr` is a live image this thread loaded; `raw` is a live local the
        // call only writes. It does not throw.
        unsafe { lectrix_image_facts_of(ctx, self.ptr, &mut raw) };
        let small = |v: c_int| u8::try_from(v).unwrap_or(u8::MAX);
        ImageFacts {
            width: u32::try_from(raw.w).unwrap_or(0),
            height: u32::try_from(raw.h).unwrap_or(0),
            bits_per_component: small(raw.bpc),
            components: small(raw.n),
            image_mask: raw.imagemask != 0,
            has_mask: raw.has_mask != 0,
            use_decode: raw.use_decode != 0,
            use_colorkey: raw.use_colorkey != 0,
            decode_inverted: raw.decode_inverted != 0,
            space: match raw.colorspace {
                1 => SpaceKind::Gray,
                2 => SpaceKind::Rgb,
                3 => SpaceKind::Bgr,
                4 => SpaceKind::Cmyk,
                5 => SpaceKind::Lab,
                6 => SpaceKind::Indexed,
                7 => SpaceKind::Separation,
                _ => SpaceKind::None,
            },
            compressed: match raw.compressed {
                1 => Some(Compressed::Jpeg),
                2 => Some(Compressed::Jpeg2000),
                _ => None,
            },
            color_transform: (raw.color_transform >= 0).then_some(raw.color_transform),
            compressed_len: raw.compressed_len,
        }
    }

    /// Its JPEG or JPEG 2000 data as the file has it (the filters before that one undone);
    /// `None` for an image kept otherwise.
    pub fn compressed(&self) -> Option<&[u8]> {
        let ctx = Context::get().as_raw_ptr();
        let mut len = 0;
        // SAFETY: `self.ptr` is live; the call only reads it and does not throw.
        let data = unsafe { lectrix_image_compressed(ctx, self.ptr, &mut len) };
        // SAFETY: the data is the image's own buffer, `len` bytes, kept as long as the
        // image is, which the borrow of `self` ensures.
        (!data.is_null()).then(|| unsafe { std::slice::from_raw_parts(data, len) })
    }

    /// The colour profile the document gives its colour space (an Indexed one's base);
    /// `None` for device colours and spaces without one.
    pub fn icc_profile(&self) -> Option<&[u8]> {
        let ctx = Context::get().as_raw_ptr();
        let mut len = 0;
        // SAFETY: as for `compressed`.
        let data = unsafe { lectrix_image_icc(ctx, self.ptr, &mut len) };
        // SAFETY: the profile's buffer belongs to the image's colour space, which the
        // image holds as long as it lives.
        (!data.is_null() && len > 0).then(|| unsafe { std::slice::from_raw_parts(data, len) })
    }

    /// Its pixels: grey or RGB (other colour spaces converted to RGB), its mask as alpha
    /// (premultiplied); a stencil mask as alpha alone, 255 where it paints. At most
    /// `max_side` pixels a side unless that is 0.
    pub fn pixels(&self, max_side: u32) -> Result<Decoded> {
        let ctx = Context::get().as_raw_ptr();
        let mut out = std::ptr::null_mut();
        let mut err = LectrixError::new();
        let side = c_int::try_from(max_side).unwrap_or(c_int::MAX);
        // SAFETY: `self.ptr` is live; `out` and `err` are live locals.
        let rc = unsafe { lectrix_image_pixels(ctx, self.ptr, side, &mut out, &mut err) };
        check(rc, err)?;
        Decoded::wrap(out)
    }
}

impl Drop for LoadedImage {
    fn drop(&mut self) {
        let ctx = Context::get().as_raw_ptr();
        // SAFETY: `ptr` is an image this thread loaded and nothing else holds; dropping it
        // does not throw.
        unsafe { lectrix_image_drop(ctx, self.ptr) };
    }
}
