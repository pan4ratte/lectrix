# 0034: Pages exported as PNG, JPEG and TIFF

- Status: accepted (the formats, flow and dialog, and exporting documents that forbid
  copying decided by the user, 2026-11-02)
- Date: 2026-11-02

## Context

Users ask to save pages as pictures: a page for a slide or a web page, a scan for an
archive, a book's pages for an app that takes only images. Acrobat exports to JPEG, JPEG
2000, PNG and TIFF, at a resolution, in colour, greys or black and white; FineReader
saves pages as BMP, JPEG, PNG, TIFF and JPEG 2000; Foxit and PDF-XChange much the same.

Four questions: which formats and through what; how pages are drawn and written without
using too much memory; where the files go; and what to do with documents whose security
settings forbid copying their content.

**Encoders.** MuPDF 1.27.2 writes PNG, PNM, PSD, JPEG and JPEG 2000 (`write-pixmap.h`),
not TIFF. Lectrix already has the `png` crate (page images), libjpeg-turbo's encoder
(Compress, ADR 0027) and MuPDF's CCITT Group 4 encoder. The `tiff` crate (MIT, image-rs)
writes LZW and Deflate and many pages to a file, but has no bilevel images.

## Decision

1. **PNG, JPEG and TIFF.** WebP and JPEG XL were built first, at the user's request, with
   Imazen's pure-Rust zenwebp 0.4.6 and jxl-encoder 0.3.4, and then dropped by the user
   (below, "WebP and JPEG XL"). JPEG 2000 was offered and not taken.
2. **The encoders:** the `png` crate; libjpeg-turbo through Compress's encoder, given a
   resolution for the JFIF header (`encode_jpeg_at`); the `tiff` crate, with black and
   white written as one strip of MuPDF's CCITT Group 4 through its low-level directory
   writer (WhiteIsZero, as fax readers take it; Windows' codec reads it black on white).
   One new dependency, `tiff`, whose LZW and Deflate come from weezl and flate2.
3. **The preview** shows the file itself for PNG and JPEG (so JPEG's artifacts show), and
   the page's own pixels for TIFF, which keeps them.
4. **Pages are drawn as the viewer draws them** (content, annotations, form fields:
   `Session::export_list`, not cached), on white, at `dpi / 72` pixels a point, by worker
   threads from display lists (ADR 0001): one a core but one, as many as a budget of 1 GB
   allows for the largest page. A page is written whole, so a page has at most 150
   million pixels (450 MB in colour), and JPEG at most 65,535 a side. Every page is
   checked before any is drawn, and the dialog says which is too large and the highest
   resolution that fits.
5. **Files are written into temporary files** beside their names (`.lectrix-part`), and
   renamed once every page is written; stopped or failing, the temporary files are
   removed. A multi-page TIFF is written in the order of its pages while later pages are
   drawn, at most twice the workers ahead.
6. **The Save dialog names the files, Acrobat's way** (the user's choice): one name; one
   page, or one TIFF, is written to it; several pages beside it, named after it with each
   page's number or label (`export::names`). Lectrix asks before replacing files the Save
   dialog didn't ask about. The files beside the one named are in the folder the user
   picked in the dialog (AGENTS.md section 2: file access only to paths the user picked);
   the webview never names a path, and Show in folder takes a token.
7. **Documents that forbid copying can be exported** (the user's decision). Their "copy"
   permission (bit 5, "copy or otherwise extract text and graphics") is not checked here,
   unlike Extract (section 6.7). AGENTS.md section 5.4 says permission flags are
   respected; this is a deliberate exception for exporting images, where a picture of a
   page is what printing the page or taking a screenshot of it would give.

8. **Copy page as image** draws a page as exporting does, in colour at the resolution
   Settings gives (150 dpi to start with), and Rust puts it on the clipboard through
   Tauri's clipboard plugin (`tauri-plugin-clipboard-manager` 2.5.0, MIT or Apache-2.0,
   the user's choice) and arboard under it: on Windows as a PNG and a `CF_DIBV5` bitmap,
   on Linux as `image/png` (X11 and Wayland). The webview gets no clipboard permission.
   The clipboard takes four bytes a pixel, and on Windows a bitmap of as many besides, so
   a page is copied with at most 36 million pixels (A4 and Letter at 600 dpi), a larger
   one at the highest resolution under that, and the notification says which. Its copy
   permission is not checked, as for exporting (7).

## Measurements

Release build, 2026-11-02, on mains (the machine has many cores), `pdf-cli
export-images`. Twenty text pages of `08-calibre-made.pdf` at 300 dpi (8.4 million pixels
a page):

| Format | Time | Peak memory | Files |
| --- | --- | --- | --- |
| PNG | 0.55 s | 479 MB | 7.9 MB |
| JPEG, quality 85 | 0.81 s | 1,033 MB | 10.8 MB |
| TIFF, LZW | 0.48 s | 459 MB | 18.1 MB |
| TIFF, black and white | 0.24 s | 157 MB | 0.7 MB |

Memory for one page at 600 dpi (33.7 million pixels), a pixel: PNG and TIFF 6 bytes in
colour, 2 in grey (the page drawn and its copy); JPEG 9 (libjpeg keeps the coefficients
for its Huffman pass) (`Options::bytes_per_pixel`).

## WebP and JPEG XL

Built and measured, then removed. The two encoders made the program 3.4 MB larger
(`pdf-cli` 12.0 to 15.6 MB; cargo-bloat: jxl-encoder 2.1 MB, zenwebp 0.55 MB), 2.5 MB
with jxl-encoder built for size. Google's libwebp would be about as large as zenwebp,
and libjxl, with its highway and brotli, larger than jxl-encoder and a C++ and CMake build
besides. Both of Imazen's crates are AGPL-3.0-only, so a build with them could be
distributed only under AGPL-3.0, not "or later". No desktop PDF app the user compares
Lectrix with exports either format. The user dropped JPEG XL for its size and WebP for
the little demand for it in a PDF app and for the license (2026-11-02). WebP could come
back through libwebp (BSD-3-Clause, built with `cc`) if people ask for it.

What was learned: 20 pages at 300 dpi took 1.2 s as WebP (2.2 s lossless) and 5.6 s as
JPEG XL at effort 4 (15 s lossless at effort 3), JPEG XL's encoder taking 50 to 72 bytes
a pixel; jxl-oxide 0.12.6 could not read jxl-encoder's lossless files larger than one
group, while jxl-rs 0.8.0 could.

## Alternatives considered

- **WebP and JPEG XL**, above.
- **Drawing in bands** to keep memory low, as printing does (ADR 0028): a page of 150
  million pixels is 450 MB in colour, within reach; not done.
- **A folder picker** instead of the Save dialog: a name for the files would then be
  Lectrix's alone; the user chose Acrobat's way.
- **Refusing documents that forbid copying**, as Extract does: the user decided against
  it.

## Consequences

- Copy page as image adds the clipboard plugin, arboard, and the `image` crate with its
  PNG and BMP codecs (arboard's own PNG writer for the clipboard), clipboard-win on
  Windows and x11rb and wl-clipboard-rs on Linux: `lectrix.exe` is 0.5 MB larger (28.93 to 29.45 MB, release build).
- The program is 0.25 MB larger (`pdf-cli` 12.05 to 12.31 MB: the `tiff` crate and the
  export code).
- `pdf-cli export-images` exports headlessly, for tests and measurements.
