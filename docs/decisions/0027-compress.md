# 0027: Compress replaces images where they are, with Lectrix's own image pipeline

- Status: accepted (the dialog and flow decided by the user, 2026-10-19)
- Date: 2026-10-19

## Context

Compress is the second of the tools planned after v1 (`docs/status.md`, "Planned tools";
section 6.11): a PDF written smaller, with precise control over images (resolution,
compression, quality, colour), as ABBYY FineReader gives, and the quality bar of Acrobat's
PDF Optimizer. The plan named MuPDF 1.27.2's `pdf_rewrite_images` and `pdf_subset_fonts`.

Reading `pdf-image-rewriter.c` showed that the image rewriter falls short of that bar:

- **It rewrites every content stream.** It works through MuPDF's content filter, which
  writes each page's contents and every annotation appearance anew, other apps'
  annotations included (against AGENTS.md section 5.1, rule 10), and puts each rewritten
  image in as a new object.
- **It measures images inside forms in the form's space.** The filter reads each form
  XObject on its own, starting from the identity matrix, so an image in a form scaled by
  half is taken for twice its resolution on the page; tiling patterns likewise.
- **A shared image is encoded again wherever it is drawn**: a logo on 300 pages, 300 times.
- **Soft masks are never touched** (a `FIXME` in the source), so images with transparency
  barely shrink.
- **Grey images stay RGB.** It tells an RGB image that is all grey, but writes it as RGB.
- **Black and white is dithered** with an ordered halftone, which ruins anti-aliased text;
  and a CMYK image is never taken for grey or black and white (`s[2] != 0` where `== 0` is
  meant).
- **"Average" downsampling only halves**, so a resolution between powers of two is out of
  reach.
- **Its JPEG encoder** (`fz_write_pixmap_as_jpeg`) writes progressive JPEGs at full colour
  resolution (4:4:4), which are a quarter to a third larger than 4:2:0 at the same quality
  and slower to decode.

## Decision

1. **Images are replaced where they are, object by object.** Each image XObject's stream
   is written anew under its own object number (`pdf_update_stream`, raw), with its
   `/Width`, `/Height`, `/BitsPerComponent`, `/ColorSpace`, `/Filter` and `/DecodeParms`;
   `/Decode` goes once it is in the pixels. Page contents, forms, annotations and their
   appearances are not rewritten, and an image drawn on many pages is encoded once.
   (`crates/pdf-core/src/compress/images.rs`)
2. **Where each image is drawn comes from a processor of Lectrix's own** (`compress.c`,
   `lectrix_walk_page_images`): a `pdf_processor` that keeps the transformation matrix
   through `q`, `Q` and `cm`, and follows it into form XObjects (their `/Matrix`), tiling
   patterns (their `/Matrix`, from the space of the stream that uses them), soft-mask
   groups, and annotation and widget appearances (`pdf_process_annot`). It reports each
   image's object number and resolution, and the same for its soft mask and stencil mask;
   it changes nothing. An image keeps the resolution of the place it is drawn largest.
3. **The pixel work is Lectrix's own** (`compress/pixels.rs`): resampling (Catmull-Rom
   widened to the reduction, or the area average, to any size), grey and black-and-white
   detection, Otsu's threshold, PNG predictors. MuPDF decodes (`fz_get_pixmap_from_image`)
   and converts colour (`fz_convert_pixmap`, colour-managed), and encodes JPEG 2000
   (OpenJPEG), CCITT Group 4 and Flate.
4. **JPEG is encoded through libjpeg directly** (`lectrix_encode_jpeg`), the libjpeg MuPDF
   links already: libjpeg-turbo with MozJPEG's encoder (ADR 0024). Baseline, with
   MozJPEG's trellis quantization and optimized Huffman tables; colour halved both ways
   (4:2:0) below quality 90. `pdf-core` names `mozjpeg-sys` (=2.2.3, the version and
   features already in the tree) only to have its headers for the shim.
5. **Lossless stays lossless, and smaller.** An image stored with general-purpose filters
   is compressed again exactly as it is (Flate at its highest level, with PNG predictors
   when they help, or Group 4 for one bit a pixel). An image that looks drawn (a few
   colours, or mostly flat areas) is never turned into JPEG. Every image is replaced only
   by something smaller; a lossy image re-encoded at its own size and colours only if that
   saves a tenth.
6. **Fonts** are subset with MuPDF's `pdf_subset_fonts` (TrueType and CFF CID fonts), but
   the fonts of the form's default resources (`/AcroForm /DR`) are put back as they were
   afterwards: readers type new text in fields and text boxes with them.
7. **The file is written** with garbage collection that merges duplicate objects and
   streams (level 4), Flate at its highest effort for streams without a filter, and object
   streams with a cross-reference stream unless the user unticks it
   (`SaveKind::Compressed`). Never Brotli, which no other reader decodes. Encryption is
   kept. The vendored `mupdf` crate gets setters for object streams and compression effort
   (`LECTRIX_PATCHES.md`, row 6).
8. **The result is computed first, into a file of Lectrix's own** (the app's local data,
   `compress/`), shown with its size and a preview, and only then saved over the file or
   as a copy (section 6.11).

## Alternatives considered

- **`pdf_rewrite_images` as it is**: the shortcomings above. Patching it in the fork
  would mean replacing most of it, and rewriting content streams would remain.
- **MuPDF's JPEG writer**: no control over chroma subsampling or progression.
- **A Rust JPEG encoder (the `mozjpeg` crate, `jpeg-encoder`)**: a new dependency where
  the encoder is already linked.
- **Images as new objects with the content streams pointing to them**: what MuPDF does;
  every page and appearance rewritten for nothing.

## Consequences

- Rendering is the test: the lossless preset must render every page pixel for pixel as
  before, and the others within a set PSNR (`crates/pdf-core/tests/compress.rs`); the
  interop suite `compress` renders the results in MuPDF, PDFium and pdf.js.
- A page that can't be read leaves its images at their resolution; a damaged image is
  left as it was. Images reached only through Type 3 glyphs, and inline images, are not
  downsampled.
- JBIG2 and MRC, for scans, wait for OCR (Leptonica).
