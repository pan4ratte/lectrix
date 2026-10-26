# Changes to offer upstream

Changes Lectrix carries in its dependencies, as plain patches against the upstream sources,
for sending to their maintainers. If upstream adopts one, Lectrix drops its copy.

| Patch | Upstream | Carried in | Status |
| --- | --- | --- | --- |
| `mupdf-1.27.2-jpx-reduced-decoding.patch`: decode JPEG 2000 at the resolution MuPDF asks for (`l2factor`) instead of always in full | MuPDF (Artifex Software); contributions go through Artifex's contributor agreement, see mupdf.com | Lectrix's fork of `mupdf-rs`, `mupdf-sys/folio_patches.rs` (ADR 0008) | Not yet sent |
| `mupdf-1.27.2-dct-row-copy.patch`: the JPEG filter copies a decoded row out with `memcpy` instead of a byte at a time (rows wider than its 4 KB output buffer, any image over 1,024 RGB pixels across) | MuPDF (Artifex Software), as above | Lectrix's fork of `mupdf-rs`, `mupdf-sys/folio_patches.rs` (ADR 0024) | Not yet sent |
| `mupdf-1.27.2-tounicode-from-cmap.patch`: an embedded CIDFont with no ToUnicode CMap gets one read off its own Unicode cmap, so its text extracts as text instead of U+FFFD (the TODO in `pdf_load_to_unicode`) | MuPDF (Artifex Software), as above | Lectrix's fork of `mupdf-rs`, `mupdf-sys/folio_patches.rs` (ADR 0029) | Not yet sent |

The patch's comments say "Lectrix:"; reword them before sending. Upstream may prefer the
new `fz_load_jpx_reduced` declared in `include/mupdf/fitz/image.h` rather than in
`image.c`. The measurements in ADR 0008 show what it gains.

## Upstream work

Bugs found upstream and what was sent about them (moved here from `docs/status.md`).

- `docs/upstream/mupdf-1.27.2-jpx-reduced-decoding.patch`: decoding JPEG 2000 at the
  resolution drawn, for Artifex. Not yet sent. Its comments say "Lectrix:"; reword them
  before sending.
- MuPDF's saved journal cannot be loaded back (`pdf_deserialise_journal` and
  `pdf_add_journal_fragment` disagree; ADR 0001). This could be reported.
- After an undo, MuPDF's incremental save writes the trailer's `/Size` from the edit
  section's length, which still counts the objects the undone step created, though it
  writes none of them (`writexref` in `pdf-write.c`). Lectrix trims those entries before
  every incremental save and recovery copy (`ffi::trim_unused_objects`); the fix belongs in
  MuPDF and could be reported.
- An incremental save that is not kept leaves the document pointing at it: MuPDF sets
  `doc->startxref` to the update it wrote, so a caller that writes to a temporary file
  and fails to move it into place writes a self-referring `/Prev` next time (ADR 0003, "A
  save that fails"). And a snapshot of a document with a cross-reference stream leaves a
  "Save document" step in the journal, although a snapshot is meant to leave the document
  as it was. Lectrix works around both in the shim; both could be reported.
- GTK 3 windows drawn with GL on Wayland update at 60 fps on faster displays, which holds
  WebKitGTK (and Lectrix's scrolling on Linux) to 60. Reported by the user on 2026-10-12:
  GTK issue 8461 (https://gitlab.gnome.org/GNOME/gtk/-/work_items/8461), and a comment on
  WebKit bug 199376 (https://bugs.webkit.org/show_bug.cgi?id=199376). Texts in
  `docs/upstream/`. When GTK fixes it, measure again with `tests/perf/wheel-linux.py` on a
  120 Hz screen.
- `pdf_get_xref_entry_aux` (`pdf-xref.c`, MuPDF 1.27.2) reads the section an object was
  last found in from `doc->xref_index` and then starts at section 0 all the same (`else
  j = 0`), going through every run of every section before it. A file whose last update
  has a thousand runs (1,310 objects scattered over 77,000) makes every lookup of an
  older object a thousand steps: edits in Lectrix are five times slower after such a
  save (the gap is under "Organizing pages" in [docs/status/known-gaps.md](../status/known-gaps.md)). Using the index, or keeping a
  section's runs in an order that can be searched by halves, would mend it; this could
  be reported, with a file made by `pdf-cli pages move` and a save.
- The vendored `mupdf` crate's patches (`third_party/mupdf-rs/LECTRIX_PATCHES.md`) could be
  offered to mupdf-rs as a pull request, if the user wants.
- Svelte (5.57.1, and 5.57.2 still) leaves a `$derived` stale for good when an effect's
  cleanup reads it in the flush that changes what it depends on: a component going away
  reads its props. The document shown was such a value, and after another tab had been
  shown the side panels, the view bar and Save stayed on one pane of documents side by
  side, whichever had the focus (2026-10-25). The values that follow the tab shown
  (`app.active`, `app.activeGroup`, `TabGroup.focused`, and what App and SplitView take
  from them) are therefore getters, not `$derived`. A report with a reproduction is in
  `docs/upstream/svelte-derived-read-in-cleanup.md`; not yet filed. Once Svelte fixes it,
  they can be cached again.
