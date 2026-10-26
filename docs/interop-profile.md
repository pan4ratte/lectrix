# Annotation interoperability profile

This is AGENTS.md section 5 in full, which AGENTS.md keeps in short; keep the two in step. Every annotation Lectrix
writes must display, print and be editable in Acrobat Reader, PDFium-based viewers (Edge,
Chrome), pdf.js (Firefox) and Foxit. Write conservatively, read leniently, and never weaken
these rules to make a test pass: report the conflict instead.

## Write profile

| # | Rule | Where it is enforced | Test |
| --- | --- | --- | --- |
| 1 | Standard subtypes only (Highlight, Underline, StrikeOut, Squiggly, Text, Ink, FreeText); no custom subtypes or private keys | `annot::Kind` (closed enum), `annot::create` | type system; `annotations::every_type_has_every_profile_key_on_every_page_geometry` |
| 2 | Normal appearance `/AP /N` on every annotation, regenerated after every edit, by MuPDF synthesis; the corrections Lectrix applies are in ADR 0006 | `annot::write::synthesize` (every create, edit and repair ends there; it fails if MuPDF draws nothing) | round trip, interop harness `phase5` |
| 3 | QuadPoints in Acrobat order (UL, UR, LL, LR) in PDF user space, written by one function. Selected text becomes quads in Rust, from MuPDF's character quads, so they follow the text's direction: one quad for each run of selected characters that lie side by side on the page, from the start edge of the first one there to the end edge of the last, its corners named as the glyphs stand. Right-to-left text, whose characters MuPDF lists running leftwards, so gets a quad in Acrobat order that covers all of it; a range that crosses between the two directions gets several (ADR 0017) | `annot::quads::quad_points_array`; `text::range_quads` | `quads` unit tests, `text::tests::a_range_runs_from_its_first_edge_on_the_page_to_its_last`, `annotations::selected_text_becomes_quads_in_the_text_direction`, `annotations::right_to_left_text_is_marked_whole_and_needs_no_repair` |
| 4 | `/Rect` = union of content + stroke width + 1 pt margin, grown together with the appearance `/BBox` when MuPDF's bounds are tighter. Text boxes keep the margin in `/RD [1 1 1 1]` | `annot::write::finalize_rect`, `synthesize` | round trip (every type, every page geometry) |
| 5 | Highlights blend with Multiply; opacity in `/CA` on the annotation (also at 1) and in the appearance | MuPDF `pdf_write_highlight_appearance`; `annot::write::set_opacity` | round trip checks the ExtGState of every markup type |
| 6 | `/NM` (UUID), `/T` (the author from Settings, by default the system user name), `/CreationDate`, `/M` (with time zone), `/F 4`, `/C`, `/P`; notes and markup with a note get a `/Popup` with `/Parent` (the popup also gets `/P` and `/F 28`, as Acrobat writes). **Text boxes:** `/C` is the background fill for FreeText (PDF 32000-1 12.5.6.6), so Lectrix writes `/C []` (no background) and the text colour in `/DA` (decided by the user on 2026-10-04: `/C` as the type defines it) | `annot::write::write_metadata`, `set_color`, `ensure_popup` | round trip |
| 7 | FreeText: Helvetica via `/DA` (`/Helv`), one size and colour, plain `/Contents`, no `/RC`, no `/CL`; the box grows to fit its text, measured with MuPDF's Helvetica widths and line breaks | `annot::create`, `annot::text_box` | round trip; `text_box` unit tests |
| 8 | All screen/PDF coordinate conversions through `geometry.rs` (Rotate, offset CropBox, UserUnit) | `geometry::PageGeometry` (annotation keys are written in user space, never through MuPDF's page transform) | unit tests + `roundtrip::geometry_matches_mupdf_page_transform`; every annotation test runs on 7 page geometries |
| 9 | Ink simplified with Ramer–Douglas–Peucker, 0.5 pt (in view space, so in points whatever the UserUnit) | `annot::ink::simplify` | `ink` unit tests; round trip checks every input point is within 0.5 pt |
| 10 | Never rewrite, reorder or drop annotations the user did not touch; edits change only edited keys plus `/M` and the appearance (and the geometry keys synthesis draws from: `/Rect`, `/RD`, `/BS`). Editing a note's text removes `/RC` (rich text), which readers would otherwise show instead. Turning text markup into another text-markup type (Highlight, Underline, StrikeOut, Squiggly) rewrites `/Subtype` and replaces the whole `/AP`, so no rollover or down appearance of the old type remains; quads, colour, opacity, note, author and names stay. An edit of text markup whose QuadPoints are not in Acrobat order (crossed or mirrored) puts them in it, the same corners, before the appearance is drawn from them (ADR 0017). An appearance stream from the file may be shared by several annotations (Acrobat, MuPDF's garbage collection and Compress write them so): it is never changed in place. MuPDF's synthesis writes a new one, and growing an appearance's `/BBox` with its `/Rect` (rule 4, repair) copies it first | `annot::edit`; `annot::write::grow_with_appearance`; incremental saves | `annotations::editing_another_apps_annotation_changes_only_the_edited_keys` (unknown keys kept; other annotations keep their bytes), `annotations::repair_changes_no_appearance_another_annotation_shares`, `annotations::text_markup_changes_type_and_keeps_everything_else` (every type into every other, on every page geometry), interop harness `phase5` (`edited.pdf`) |

Dates are written in UTC as `D:YYYYMMDDHHmmSS+00'00'`.

## Replies (ADR 0012)

A reply is a `/Text` note answering another annotation through `/IRT`; readers show it in
that annotation's thread, not on the page. Rules 2, 4 and 6 have these exceptions for
replies, and only for them:

| Rule | For a reply | Where | Tested by |
| --- | --- | --- | --- |
| 2 | An empty normal appearance (an empty form, `/BBox [0 0 0 0]`): nothing draws it, and pdf.js draws no icon of its own. Edits never redraw it; repair gives another app's reply without an appearance an empty one | `annot::reply::put_empty_appearance`, `add_reply`, `edit_reply`; `annot::repair` | `annotations::replies_are_threaded_notes_that_nothing_draws` (MuPDF draws the page pixel for pixel as without the reply, after adding, editing and repairing it); interop harness `phase5` (`replies.pdf`: MuPDF, PDFium and pdf.js change 0 pixels) |
| 4 | `/Rect` of no area at the parent's top-left corner (user space): pdf.js's annotation layer skips it, so no element of the reply covers the parent and takes its hover | `annot::reply::add_reply` | same tests; the harness checks pdf.js reads a `/Rect` of no area |
| 6 | No `/Popup`. `/IRT` (the parent), `/Name /Comment`, `/Open false`, the parent's `/C` (yellow without one), and the usual `/NM`, `/T`, `/CreationDate`, `/M`, `/F 4`, `/P` | `annot::reply::add_reply` | same tests; pdf.js links each reply to its parent |

Only a reply's text and author can change (plus `/M`); emptying it deletes it in the app.

## Reading other apps' annotations

- Every standard type MuPDF supports is displayed (MuPDF renders them; Lectrix draws its own
  overlay only for the selection).
- The annotation list reads `/Annots` from the page dictionaries without loading pages
  (1,316 notes in 29 ms). Popups, links and form widgets are not listed.
- Missing appearance or unreadable quads: MuPDF draws them from their properties for
  display only (a local appearance, never saved); the list shows a "needs repair" badge.
- QuadPoints out of Acrobat order get the badge only when their corners are crossed.
  Mirrored ones are read as they are (hit-testing, the list's order and the marked text
  use each quad's corners whatever their order) and reported as nothing (ADR 0017).
- Replies (`/IRT`) appear in their parent's thread in the list, where they can be edited and
  deleted, and new ones written (see "Replies"). The viewer does not hit-test them. Deleting
  an annotation deletes its replies (after asking) and its popup.

## Repair

Scan, summarize problems, and on confirmation fix them in one undoable operation
(`annot::repair`):

- missing appearances (for the types MuPDF can draw), generated by MuPDF synthesis
  written into the document (`ffi::request_appearance`; without it MuPDF only draws a
  display-only appearance);
- crossed QuadPoints into Acrobat order (`annot::quads::crossed`, quad by quad): the
  spec's counter-clockwise order, a clockwise one, a scramble, which readers that draw
  from the corners fill as a bow-tie. Other apps' orders are tried first, then the corners
  as they appear on screen, and text that would come out on its head on the page as shown
  is taken to be upright. Mirrored QuadPoints (the bottom edge first, as apps built on
  Apple's PDFKit write, or each edge from right to left) are the same shape read either
  way: while the annotation has an appearance they are not reported and stay as their app
  wrote them, and they are put in order only where repair has to draw the annotation (no
  appearance, or a new one for a bigger `/Rect`). ADR 0017;
- `/Rect` grown where the content lies more than the 1 pt margin outside it (Acrobat's own
  highlights miss skewed quads by a fraction of a point, harmlessly); the appearance
  `/BBox` grows with it, or a new appearance is drawn when its `/Matrix` makes that
  impossible;
- missing `/NM`, `/F 4` (only when `/F` is absent), `/M`, `/P`.

Unreadable QuadPoints are reported and left alone; a quad whose corners lie in one line has
no order and is neither reported nor rewritten. Content, colour, author and position never
change; every change is written to the app log with the object number. What a scan
reports as fixable is what repair fixes, so a second repair finds nothing to do
(`annotations::mirrored_corners_stay_as_written_while_the_annotation_keeps_its_appearance`).

## Saving

AGENTS.md section 5.4 in full:

- Default save is incremental: changes are appended and the original bytes stay intact.
- If MuPDF reports the file was repaired on open (damaged cross-reference table), it cannot be saved incrementally; do a full save and tell the user once, in plain words, why. A file that opens without an error but with no pages (a cross-reference table that leads nowhere, such as a last section naming itself as the previous one) gets the same repair from Lectrix, and is then treated the same way; if it still has no pages, it is reported as too damaged to open.
- "Save As (optimized)" does a full rewrite with garbage collection and stream compression.
- If the document is digitally signed, warn before the first edit that edits will be shown as changes after signing, and always save incrementally.
- Encrypted documents: prompt for the password, keep the encryption on save, and respect permission flags (no annotating if the document disallows it). The exceptions are Unlock (section 6.8, ADR 0025), which writes the file, or a copy, without the encryption, and Protect (section 6.9, ADR 0026), which writes it with a password, restrictions or both that the user chose: always AES-256, in a full save, and checked before the file replaces anything.
- PDF/A: a file that claims a PDF/A level (`pdfaid:part` in the XMP metadata of the catalog's `/Metadata`; the claim is read, compliance is not checked) opens for reading only, as Acrobat opens it. The session refuses every operation meanwhile, the frontend offers nothing that changes the document (annotations, bookmarks, page labels, rotating, moving, deleting or inserting pages, repair; such a shortcut says why in a notification with an Enable editing button), and bookmarks expanded or collapsed meanwhile are not written. A banner under the view bar says so, with the level ("This file claims compliance with the PDF/A standard (PDF/A-1b), so it was opened for reading only. Saving annotations or inserted pages removes that claim."), and its **Enable editing** button makes the document one like any other for as long as it is open. Settings can open such files ready to edit (section 6.6), and one that comes with changes restored after a crash opens that way. Lectrix cannot vouch that what it writes complies, so a file never keeps a claim Lectrix may have broken: a save whose changes include an annotation added, changed, deleted or repaired, pages inserted from another file, or pages deleted, takes the identification properties (`pdfaid:part`, `conformance`, `amd`, `corr`, `rev`) out of the XMP, leaves the rest of the metadata as it was, and says so in a notification ("report.pdf no longer claims to be PDF/A-1b."). Changes undone before the save do not count. Bookmarks, page labels, page rotation and the order of the pages alone keep the claim, as does a Save As, optimized or not. A document restored after a crash loses the claim at its next save, since what its changes were is not known.

Where it is enforced: "Save As (optimized)" renumbers objects, so the app then takes the new
bookmark and annotation ids. While a PDF/A file is open for reading only, the session refuses
operations (`Error::ReadOnly`). The write profile above is not PDF/A's (blended highlights,
a font that is not embedded), which is why a save that may break the claim removes it
(`docinfo::remove_pdfa_claim`: the `pdfaid` properties go, the rest stays byte for byte),
decided in `session.rs` (`Operation::may_break_pdfa`); tested in
`crates/pdf-core/tests/pdfa.rs` and `tests/e2e/specs/pdfa.test.mjs`.

## Engine notes (from the interop harness)

- PDFium ignores `/UserUnit` when sizing pages; MuPDF and pdf.js honour it. Annotations stay
  correctly placed relative to page content in all three engines.
- Sticky notes (`/Text`) on rotated pages: Acrobat and MuPDF keep the icon upright at the
  upper-left corner of `/Rect` (text annotations behave as NoZoom and NoRotate, PDF
  32000-1 12.5.6.4); PDFium and pdf.js turn it with the page, one icon-width away. Lectrix
  writes notes for Acrobat's model (ADR 0006); the harness accepts either place.
- MuPDF 1.27 writes a `/CL` callout line on every new FreeText; Lectrix removes it.
- A FreeText's `/Rotate` (an Adobe key the specification does not list) says how far its
  text is turned in user space. MuPDF's create gives a new box the page's `/Rotate`, so its
  text is upright on a rotated page, and draws the text by the box's own. Lectrix writes
  the page's `/Rotate` plus the turn of the page on screen where the box was typed (the
  view rotation, a page's own turn), as Acrobat does for a box typed in a rotated view,
  and none when that is 0 (`annot::write::put_text_rotation`). The box is fitted to its
  text where the text is upright (`PageGeometry::turned`), on creation and on every edit,
  by the box's own `/Rotate`: another app's box without one, on a rotated page, is fitted
  along user space, where MuPDF draws its text. Tested in
  `annotations::a_text_box_typed_on_a_page_turned_on_screen_stands_upright_there` (every
  page geometry, each turn) and by the harness (`turned-normal.pdf`, `turned-rot90.pdf`:
  drawn alike by MuPDF, PDFium and pdf.js, which all draw the appearance stream).
- Thin lines (underline, strikeout, squiggly are under 1 pt): the rasterizers spread their
  anti-aliased edges differently, which at 2x render scale outweighed the line in the
  harness's colour comparison (a strikeout on a 180° page: distance 76 against the limit
  60, while the line's own colour matched). The annotation suite (`phase5`) renders at 4x (worst
  distance 49).
