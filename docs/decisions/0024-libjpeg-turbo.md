# 0024: JPEG decoded by libjpeg-turbo, and JPEG rows copied whole

- Status: accepted (asked for by the user, 2026-10-18)
- Date: 2026-10-19

## Context

A 283-page book made with OneVision PDFengine showed its first page after 580 to
626 ms, where a text file shows it in 100 to 170 ms (`docs/status.md`, the cover gap).
Its cover is the whole jacket in one 6035 × 2693 CMYK JPEG with an ICC profile and a soft
mask that is a second JPEG of that size; the page crops the front out of it. At the
3.5 px per point the page is fitted at, MuPDF has to decode both images in full (their
rows all, the columns shown), about 345 ms of the page's render.

Two things in MuPDF made that slow, measured on 2026-10-18 with `pdf-cli bench` and
copies of the file with parts taken out:

- **Its JPEG decoder.** MuPDF bundles the Independent JPEG Group's libjpeg 9, which has
  no SIMD code. libjpeg-turbo (Pillow's build) decodes the cover's 16-megapixel CMYK
  image in 93 ms and its mask in 13.
- **Its JPEG filter's copying.** `filter-dct.c` hands its output on in 4 KB pieces
  (`fz_dctd.buffer`). A row wider than that (any image over 1,024 RGB pixels across) is
  decoded into a row buffer and copied out a byte at a time (`*p++ = *state->rp++`):
  65 million times for the cover, 16 million for its mask.

The fork of `mupdf-sys` (ADR 0008) is where such changes go: MuPDF itself is not forked.

## Decision

1. **The fork's `mupdf-sys` gets a `libjpeg-turbo` feature**, which Lectrix turns on
   (`Cargo.toml`). It builds MuPDF against an outside libjpeg (`SHARE_JPEG`, the way
   Linux distributions build MuPDF with libjpeg-turbo) and leaves MuPDF's own libjpeg
   out:
   - **The decoder comes from `mozjpeg-sys` 2.2.3** (pinned exactly): it compiles
     libjpeg-turbo's code with `cc` (no CMake), statically, and publishes its headers
     (`DEP_JPEG_INCLUDE`). MozJPEG is libjpeg-turbo with a different encoder; the
     decoder is libjpeg-turbo's. Features: `nasm_simd` (SIMD where NASM is found),
     `arith_dec` (arithmetic-coded JPEG, which MuPDF's libjpeg decodes too) and
     `parallel`; not `unwinding`, which MuPDF's `longjmp` error handling does not need.
   - **make** (macOS, Linux): `USE_SYSTEM_LIBJPEG=yes`, `SYS_LIBJPEG_CFLAGS` with those
     headers, `SYS_LIBJPEG_LIBS` empty (cargo links the library).
   - **MSBuild** (Windows): `libthirdparty.vcxproj` loses libjpeg's 44 sources;
     `libmupdf.vcxproj` gets `SHARE_JPEG` and libjpeg-turbo's headers in place of
     libjpeg's. Each edit must find what it changes, or the build fails.
   - `mupdf-sys` names `mozjpeg_sys` (`extern crate`), so its library is linked.
   - Under `SHARE_JPEG`, MuPDF's custom libjpeg memory manager (`jmemcust.c`) is
     compiled out, and the decoder's scratch memory comes from `malloc`, outside MuPDF's
     store accounting.
2. **The JPEG filter copies with `memcpy`** (`FILTER_DCT` in `folio_patches.rs`, always,
   whatever decoder): the two byte loops become one copy each. The same change as a
   plain diff is `docs/upstream/mupdf-1.27.2-dct-row-copy.patch`.
3. **NASM is a build requirement** (CONTRIBUTING.md). Without it, `mozjpeg-sys` builds
   libjpeg-turbo without SIMD and says so only in a build-script warning, which cargo does
   not show for registry crates; the decoder is then about as fast as MuPDF's own. So:
   - `crates/pdf-core/tests/jpeg.rs` fails on x86 and Arm when libjpeg-turbo's SIMD
     probes (`jsimd_can_idct_islow`, `jsimd_can_ycc_rgb`) say it has none;
   - every CI job that compiles MuPDF installs NASM (`.github/actions/install-nasm`)
     and fails if `nasm -v` cannot run, the release builds included (they run no tests).
   - Apple silicon needs no NASM (libjpeg-turbo's Arm code is C and GNU assembler);
     Intel Macs and Linux do.

## Results

`pdf-cli bench`, release builds of the same evening, measured back to back (raster time
of one page, five runs, ms):

| | Fork as it was | + row copy | + libjpeg-turbo |
| --- | --- | --- | --- |
| The jacket cover at 3.5 px/pt (fit width) | 343 to 348 | 277 to 290 | 247 to 258 |
| The jacket cover at 2.0 px/pt (decoded at half resolution) | 150 to 160 | 135 to 142 | 120 to 127 |
| A 328-page book scanned as JPEG (5.9 megapixels a page), 40 pages at 2.28 px/pt, mean render | 87 | 73 | 55 |
| The same at 1.0 px/pt | 54 | 41 | 21 |

The rest of the cover's time is MuPDF's own work on the decoded image (the subarea,
CMYK to RGB through the ICC profile, the mask, scaling), about 100 ms, which neither
change touches. Decoding only the columns shown (libjpeg-turbo's `jpeg_crop_scanline`)
would take more of it, but needs the JPEG filter to know the subarea; not done.

In the app (`tests/perf/measure.ps1`, on mains, against the release app built from the
commit before, back to back):

| | Before | After |
| --- | --- | --- |
| The jacket book, first page visible | 467 to 481 ms | 371 to 387 ms |
| The scanned book, first page visible | 240 to 262 ms | 202 to 231 ms |
| The scanned book, a page rendered, mean (server side) | 60 ms | 35 ms |
| The scanned book, blank after a jump, median and worst | 151 ms, 188 ms | 115 ms, 154 ms |
| The scanned book, blank while scrolling steadily, worst | 26 ms | 0 |

Rendered JPEG pixels differ slightly from MuPDF's libjpeg (libjpeg-turbo's IDCT and
chroma upsampling are not libjpeg 9's): three pages of the scanned book by 0.06 to 0.12
levels in 255 on average and at most 14 on a single sample, the jacket cover by at most
1. The pdf-core tests, the local corpus and `jpeg.rs` (a 3000-pixel-wide JPEG, whole and
cropped to one band) pass.

`lectrix.exe` grows by about 0.4 MB (`pdf-cli.exe`: 10.21 MB, 10.42 MB with
libjpeg-turbo's C code, 10.60 MB with its SIMD code as well).

## Consequences

- Building Lectrix needs NASM on PATH (x86); a build without it works but fails
  `jpeg.rs`. After installing NASM, `cargo clean -p mozjpeg-sys` (its build script does
  not run again by itself).
- Lectrix ships libjpeg-turbo's code under the IJG, BSD-3-Clause and zlib licenses
  (`THIRD_PARTY_NOTICES.md`); the license check allows IJG, which MuPDF's own libjpeg
  was already under.
- Updating MuPDF means checking that the vcxproj edits and the row-copy patch still
  match (the build fails if not); updating `mozjpeg-sys` means checking its features and
  that MuPDF's `SHARE_JPEG` code still builds against it.

## Alternatives considered

- **libjpeg-turbo built by CMake** (`turbojpeg-sys` or the fork's own CMake step): the
  same decoder, with CMake as one more build requirement; `mozjpeg-sys` needs only the C
  compiler the build already has.
- **libjpeg-turbo's sources vendored into the fork** with a build of its own: the same
  result as `mozjpeg-sys`, with its build script (the 8-bit, 12-bit and 16-bit
  compilation units, the generated headers, the SIMD selection) to write and keep.
- **A Rust JPEG decoder** (zune-jpeg) behind MuPDF's DCT filter: as fast, but it would
  have to reproduce what MuPDF asks of libjpeg (Adobe markers and transforms, inverted
  CMYK, reduced decoding, damaged streams), all of it new code at the bottom of every
  render.
- **The row copy alone**: two-thirds of the cover's gain at no cost, but little for
  scanned books, whose rows are decoded at about the size drawn and whose time is the
  decoder's.
