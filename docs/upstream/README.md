# Changes to offer upstream

Changes Lectrix carries in its dependencies, as plain patches against the upstream sources,
for sending to their maintainers. If upstream adopts one, Lectrix drops its copy.

| Patch | Upstream | Carried in | Status |
| --- | --- | --- | --- |
| `mupdf-1.27.2-jpx-reduced-decoding.patch`: decode JPEG 2000 at the resolution MuPDF asks for (`l2factor`) instead of always in full | MuPDF (Artifex Software); contributions go through Artifex's contributor agreement, see mupdf.com | Lectrix's fork of `mupdf-rs`, `mupdf-sys/folio_patches.rs` (ADR 0008) | Not yet sent |

The patch's comments say "Lectrix:"; reword them before sending. Upstream may prefer the
new `fz_load_jpx_reduced` declared in `include/mupdf/fitz/image.h` rather than in
`image.c`. The measurements in ADR 0008 show what it gains.
