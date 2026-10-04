# 0008: A fork of `mupdf-sys` that decodes JPEG 2000 at the resolution drawn

- Status: accepted (Phase 6 review, decision 2, option (a); 2026-10-04)
- Date: 2026-10-04

## Context

Scanned books are often stored as JPEG 2000 (archive.org's are). The Phase 6 report
found that MuPDF 1.27.2 always decodes JPEG 2000 images in full: when it draws an image
smaller than it is, it asks the decoder for a reduced image (`l2factor`, a number of
halvings) and subsamples whatever the decoder could not reduce, but its JPEG 2000 path
(`compressed_image_get_pixmap` → `fz_load_jpx`) ignores the request. OpenJPEG can decode at
a reduced resolution itself, as far as the codestream's resolution levels go. A
33-megapixel page drawn at fit width cost a full decode (0.9 s), and so did its thumbnail.

MuPDF also decodes one JPEG 2000 image at a time across the process (a global lock in its
OpenJPEG glue); lifting that is a larger change and is left to upstream.

MuPDF's sources are 1.1 GB with their third-party libraries, so vendoring them is not an
option. `mupdf-sys` copies the sources into its build directory before compiling and
already patches that copy (`patch_mupdf_sources` in its `build.rs`).

## Decision

1. **Fork `mupdf-rs`, not MuPDF.** `https://github.com/pan4ratte/mupdf-rs`, branch
   `folio-mupdf-1.27.2`, based on the upstream commit Lectrix already used (`537d505`). The
   fork adds `mupdf-sys/folio_patches.rs` and calls it after upstream's own patch step
   (three lines in `build.rs`). Its MuPDF submodule still points at Artifex's repository.
2. **Lectrix uses the fork through `[patch]`** in the workspace `Cargo.toml`, pinned to a
   commit, so the vendored `mupdf` crate and `pdf-core` keep naming the upstream source.
3. **The patch** (`load-jpx.c`, `image.c`), as exact text replacements that must each
   match once, so a MuPDF update that moves the code fails the build instead of silently
   dropping the change:
   - `fz_load_jpx_reduced` asks OpenJPEG for as many halvings as MuPDF requested, clamped
     to the codestream's resolution levels (`opj_get_cstr_info`,
     `opj_set_decoded_resolution_factor`), and reports how many it applied;
     `compressed_image_get_pixmap` lowers `l2factor` by that, so MuPDF subsamples only the
     rest.
   - Only images at the origin of their reference grid are reduced (the reduced size is
     then simply the full size halved, rounded up); others decode in full.
   - If a reduced decode fails, the image is decoded in full, as before.
   - No public header changes.
4. **Upstream.** The same change as a plain C diff against MuPDF 1.27.2 is in
   `docs/upstream/mupdf-1.27.2-jpx-reduced-decoding.patch`. Artifex takes contributions
   under its contributor agreement, so the user submits it. If MuPDF adopts it, the fork
   goes away.

## Results

`pdf-cli render`, process start to PNG written, `29-slow-first-page`:

| | Before | After |
| --- | --- | --- |
| Page 1 (4975 × 6658) at fit width, 1421 × 1902 px | 992 ms | 425 ms |
| Page 1 thumbnail, 150 × 200 px | 904 ms | 42 ms |
| 40 body pages (1643 × 2200) at fit width, mean | 219 ms | 218 ms |
| 40 body pages as thumbnails, mean | 113 ms | 47 ms |

In the app, the book's first page shows in 376 to 382 ms (1.85 s at the start of Phase 6,
1.05 s after the thumbnail scheduling fix). Body pages already decode at about the size
drawn, so scrolling them is still bound by MuPDF's one-at-a-time decoding.

Renders differ from the full decode by 2/255 on average at fit width (OpenJPEG's wavelet
reduction instead of MuPDF's subsampling) and look the same; thumbnails are smoother.
`crates/pdf-core/tests/jpx.rs` checks that small renders match the full decode scaled
down; the pdf-core tests and the local corpus pass with the fork.

## Consequences

- MuPDF is now built from a fork: updating MuPDF means rebasing the fork's two files onto
  the new upstream `mupdf-rs` and checking that every replacement still matches (the build
  fails if one does not).
- The fork is public (GitHub forks of public repositories are), and CI fetches it like
  the upstream repository.

## Alternatives considered

- **Vendor MuPDF's sources** with the patch: 1.1 GB in the repository.
- **Fork MuPDF itself** and point the submodule at it: a 1.1 GB repository to keep in step
  with Artifex for a 100-line change.
- **Wait for upstream** (option (b)): no gain until a MuPDF release takes it.
- **Lectrix-side workarounds** (a larger image store, rendering thumbnails from page
  images): they avoid repeated decodes but never the first full one.
