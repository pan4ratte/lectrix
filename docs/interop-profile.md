# Annotation interoperability profile

This mirrors AGENTS.md section 5 and must be kept in sync with it. Every annotation Folio
writes must display, print and be editable in Acrobat Reader, PDFium-based viewers (Edge,
Chrome), pdf.js (Firefox) and Foxit. Write conservatively, read leniently, and never weaken
these rules to make a test pass: report the conflict instead.

## Write profile

| # | Rule | Where it is enforced | Test |
| --- | --- | --- | --- |
| 1 | Standard subtypes only (Highlight, Underline, StrikeOut, Squiggly, Text, Ink, FreeText); no custom subtypes or private keys | `annot::MarkupKind` (closed enum) | type system |
| 2 | Normal appearance `/AP /N` on every annotation, regenerated after every edit, by MuPDF synthesis unless an ADR says otherwise | `annot::add_text_markup` calls `update()` | `roundtrip::highlight_has_every_profile_key`, interop harness |
| 3 | QuadPoints in Acrobat order (UL, UR, LL, LR) in PDF user space, written by one function | `annot::quads::quad_points_array` | `quads` unit tests, round trip |
| 4 | `/Rect` = union of content + stroke width + 1 pt margin, grown together with the appearance `/BBox` when MuPDF's bounds are tighter | `annot::finalize_rect` | round trip |
| 5 | Highlights blend with Multiply; opacity in `/CA` on the annotation and in the appearance | MuPDF `pdf_write_highlight_appearance`; `set_opacity` | round trip checks the ExtGState |
| 6 | `/NM` (UUID), `/T`, `/CreationDate`, `/M` (with time zone), `/F 4`, `/C`, `/P`; notes get a `/Popup` with `/Parent` (popup also gets `/P` and `/F 28`, as Acrobat writes) | `annot::write_metadata`, `annot::finish_popup` | round trip |
| 7 | FreeText: Helvetica via `/DA`, one size and colour, plain `/Contents`, no `/RC` | Phase 5 | Phase 5 |
| 8 | All screen/PDF coordinate conversions through `geometry.rs` (Rotate, offset CropBox, UserUnit) | `geometry::PageGeometry` | unit tests + `roundtrip::geometry_matches_mupdf_page_transform` |
| 9 | Ink simplified with Ramer–Douglas–Peucker, about 0.5 pt | Phase 5 | Phase 5 |
| 10 | Never rewrite, reorder or drop annotations the user did not touch; edits change only edited keys plus the appearance | incremental saves; edit path in Phase 5 | Phase 5 |

Dates are written in UTC as `D:YYYYMMDDHHmmSS+00'00'`.

## Reading other apps' annotations

- Display every standard type MuPDF supports.
- Missing appearance or malformed quads: draw from properties for display, write nothing,
  and show a "needs repair" badge.
- Replies (`/IRT`) appear under their parent, read-only in v1.

## Repair

Scan, summarize problems, and on confirmation fix them in one undoable operation: missing
appearances, QuadPoints order, `/Rect` not containing the content, missing `/NM`, `/F 4`,
`/M`, `/P`. Never change content, colour, author or position. Log every change with the
object number.

## Saving

Incremental by default. Repaired-on-open files get a full save, and the user is told why,
once. "Save As (optimized)" does garbage collection and compression. Signed files: warn
before the first edit and always save incrementally. Encrypted files keep their encryption,
and permission flags are respected.

## Engine notes (from the interop harness)

- PDFium ignores `/UserUnit` when sizing pages; MuPDF and pdf.js honour it. Annotations stay
  correctly placed relative to page content in all three engines.
