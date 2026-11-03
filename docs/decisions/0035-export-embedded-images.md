# 0035: The images in a document exported as they are kept

- Status: accepted (copying JPEG and JPEG 2000, PNG with transparency for the rest and for
  a JPEG with a soft mask, each image once, leaving out small ones, and exporting
  documents that forbid copying decided by the user, 2026-11-02 and 2026-11-03)
- Date: 2026-11-03

## Context

Besides pictures of its pages (ADR 0034), people want the pictures *in* a document: the
photos of a brochure, the illustrations of a book, a scan's page images. Acrobat has
Export a PDF > Image > Export all images (re-encoded in the format chosen); `mutool
extract` writes each image object, its JPEG as it is when it can.

Five questions: what each image becomes; which images are "in" a page; what counts as
one image; how to keep the work off the document's thread; and the security setting that
forbids copying "text and graphics".

## Decision

1. **JPEG and JPEG 2000 are copied byte for byte** (the user's choice), when the file,
   opened alone, shows what the document does (`embedded::kind_of`):
   - a JPEG in grey, RGB or CMYK, without a `/Decode` of its own, a mask or a colour key,
     whose `/ColorTransform` (if any) is what its markers imply. Apps that open a JPEG
     file invert Adobe CMYK (Photoshop's convention), which PDF doesn't, so a CMYK JPEG
     is copied only with an Adobe marker and `/Decode [1 0 1 0 1 0 1 0]`, as Photoshop's
     JPEGs are placed; other CMYK JPEGs would open as negatives, and are decoded.
   - The document's colour profile (ICCBased) is added as APP2 segments to a JPEG without
     one of its own, and an EXIF orientation is set upright (PDF ignores it, so a file
     that kept it would open turned): both leave the image data as it is.
   - JPEG 2000 in grey or RGB, without `/Decode` or a mask, as `.jp2` (or `.j2k` for a
     bare codestream).
2. **The rest is decoded by MuPDF and written as PNG** (the user's choice): grey or RGB
   (other colour spaces converted to RGB), the soft mask as alpha (MuPDF's premultiplied
   colour undone), the image's own profile in `iCCP` where its colours are kept, else
   sRGB; one bit a pixel for one-bit images; a stencil mask in black on white. **A JPEG
   with a soft mask becomes a PNG too** (the user's choice over the JPEG alone, whose
   cut-out would show its background).
3. **A finer mask sets the size.** A layered scan (MRC: FineReader's, the Internet
   Archive's) draws its text as a coarse colour image under a sharp mask (411 × 550
   pixels under a JBIG2 mask of 4,927 × 6,599 in the local corpus). Written at the
   colour's size the text would blur, so an image is written at its mask's size when the
   mask has more pixels, its colours scaled up (chosen without the user).
4. **The images on a page** are found as Compress finds them (ADR 0027), by reading its
   contents, forms, patterns, soft-mask groups and annotations' appearances; the walker
   now says which reports are images drawn and which their masks, so a mask is part of
   its image, not an image of its own. Inline images and Type 3 glyphs' images are not
   objects and are left out.
5. **Each image once** (the user's choice): the first time it is drawn, by object
   number, and also an image alike as another object, which merged documents have. Alike
   means a 128-bit hash of the image's dictionary (without keys that don't change what it
   shows, such as `/Metadata` and `/StructParent`) and of everything it refers to, its
   streams' data as the file keeps them, so the hash needs no decoding.
6. **Small images can be left out** (the user's choice): those narrower or lower than 16
   to 256 pixels; on, at 32, to start with (chosen without the user), for icons, bullets
   and rules.
7. **The document's thread only loads the images.** Finding them reads the pages eight at
   a time, so the document keeps rendering. Writing them, the actor loads each image
   (MuPDF reads its data into memory) and copies JPEG and JPEG 2000; worker threads, one
   a core but one, decode the rest, write the PNGs and the files, as display lists are
   drawn on rendering threads (ADR 0001): a loaded image no longer reads its document,
   and each thread uses its own cloned context. Each image reserves the memory it will
   take before it is handed out, within 1 GB between them (an image larger than that
   alone). Files go into temporary files that take their names once all are written, as
   for pages.
8. **Documents that forbid copying are exported all the same** (the user's decision):
   AGENTS.md section 5.4's exception for exporting pages (ADR 0034) now covers the images
   in them too. Their permission names "extract graphics" outright; the user preferred one
   rule for both exports.
9. **Files are named Acrobat's way**, as pages are: one name in the Save dialog; one image
   is written to it with its own extension, several beside it as "report-007-1.jpg" (the
   page's number or label, and the image's place on that page).

## Measurements

Release build, 2026-11-03, `pdf-cli export-embedded`, over the local corpus (30 files):
every image read, none unreadable. Finding the images took at most 5.5 s (an encrypted
file of 1,400 pages without images) and under 4 s for the others; most files wrote in under
2 s. The slowest: `29-slow-first-page.pdf`, 1,652 images (830 JPEG 2000 copied, 822 layered
text images at 4,927 × 6,599 as PNG, about 0.45 s each), 264 s and 590 MB with other work
running; written at the colour images' size it took 148 s. `27-scanned-annotated.pdf`, 720
PNG, 7.5 s; `21-rotated-180.pdf`, 1,353 images, 12 s.

## Alternatives considered

- **Re-encoding every image in a format chosen**, as Acrobat does: generational loss for
  JPEGs, and a choice most people needn't make.
- **A JPEG with a soft mask as its JPEG alone**, as `mutool extract` writes it when asked
  not to apply masks: smaller and untouched, but the transparency is lost.
- **A layered scan's text at the colour image's size**: three times faster, but blurred.
  Writing its mask and its colour as two images instead keeps them small, but neither is
  what the page shows.
- **Refusing documents that forbid copying**, as Extract does (section 6.7): the user
  decided against it.
- **A folder picker**: the user chose Acrobat's Save dialog for pages (ADR 0034).

## Consequences

- No new dependency: the `png` crate writes the PNGs and their profiles, and the JPEG
  markers are read and written in Rust (`embedded::jpeg_info`, `jpeg_file`).
- `ffi/images.c` holds the MuPDF calls (loading an image, its facts, its compressed data
  and profile, its pixels with its mask); `ffi::images::LoadedImage` is `Send`, with the
  reasoning in its `SAFETY` comment.
- `pdf-cli export-embedded` exports headlessly, for tests and measurements.
