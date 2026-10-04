# 0006: Corrections Folio makes to MuPDF's annotation appearances and keys

- Status: accepted (Phase 5; approved by the user on 2026-10-04)
- Date: 2026-10-04

## Context

AGENTS.md section 5.1 rule 2 says every annotation gets its normal appearance from
MuPDF's synthesis, and that a type whose MuPDF appearance fails the interop tests gets a
custom appearance stream, documented in an ADR. In Phase 5, MuPDF's synthesis is used for
all seven types Folio creates, and for repairs. But for two types, what MuPDF writes
around the drawing either breaks a profile rule or puts the annotation in a different
place in different readers. This ADR records the small corrections Folio applies after
synthesis (`crates/pdf-core/src/annot/write.rs`, `synthesize`). No type gets a drawing of
Folio's own: the shapes are MuPDF's.

## Findings

1. **Sticky notes on rotated pages.** PDF 32000-1 12.5.6.4 has text annotations behave as
   if NoZoom and NoRotate were set. Acrobat and MuPDF do that: the icon stays upright, at
   the size of its appearance `/BBox`, fixed at the upper-left corner of `/Rect` in user
   space. PDFium (Edge, Chrome) and pdf.js (Firefox) don't: they scale the appearance into
   `/Rect` and turn it with the page. MuPDF draws its icon in a 16 × 16 box, so with a
   20 pt `/Rect` MuPDF showed a 16 pt icon and PDFium and pdf.js a 20 pt one. On a page
   with `/Rotate`, the two groups place the icon one icon-width apart, and PDFium and
   pdf.js show it lying on its side.
   - A first attempt turned the icon with the appearance's `/Matrix`. PDFium and pdf.js
     then drew it upright, but Acrobat and MuPDF (which already keep it upright) turned it
     twice. The interop harness caught this.
2. **Text boxes.** MuPDF's `pdf_create_annot` writes a `/CL` callout line on every
   FreeText (`pdf_set_annot_rect` "recalculates" the callout for the whole subtype), with
   no `/IT /FreeTextCallout`. And after synthesis MuPDF sets `/Rect` to the text box itself
   with `/RD [0 0 0 0]`, so rule 4's 1 pt margin is missing.
3. **Opacity 1.** MuPDF removes `/CA` when the opacity is 1. Rule 5 asks for `/CA` on the
   annotation as well as in the appearance.
4. **Flags.** MuPDF gives new notes `/F 28` (Print, NoZoom, NoRotate). Rule 6 asks for
   `/F 4`.

## Decision

- **Notes follow Acrobat's model.** The upper-left corner of `/Rect` (user space) is the
  point under the icon's top-left corner on screen, whatever the page's rotation. After
  synthesis, the appearance's `/BBox` is set to the size of `/Rect`, and the drawing is
  scaled to fill it inside the content stream (a `cm` around MuPDF's content). Every
  reader then draws the icon at the same size. On rotated pages, PDFium and pdf.js still
  turn it with the page (their behaviour, not something a file can change). The harness
  accepts the icon in either place: `/Rect`, or `/Rect` turned about its upper-left
  corner (`display_area` in `tests/interop/run.py`).
- **Text boxes:** `/CL` is removed when a text box is created (Folio makes no callouts),
  and after each synthesis `/Rect` and the appearance `/BBox` grow by 1 pt together,
  recorded in `/RD [1 1 1 1]`, so the next synthesis finds the same text box. A text box
  that is a callout (from another app, `/IT /FreeTextCallout`) can't be moved or
  resized in Folio, because its line would need moving too.
- **`/CA` is always written,** also at 1.
- **`/F 4` for every type,** notes too (they are kept upright as above).

## Consequences

- All seven types pass the interop harness on normal, rotated (90, 180, 270), cropped,
  cropped and rotated, and UserUnit pages in MuPDF, PDFium and pdf.js
  (`python tests/interop/run.py phase5`).
- On rotated pages, notes keep a visible difference between the two groups of readers
  (place and orientation, not visibility). Acrobat, the reference, shows them where they
  were put.
- If a future MuPDF draws notes at the size of `/Rect`, the `/BBox` step does nothing
  (it is skipped when the box already matches).

## Alternatives considered

- **Own appearance streams for notes:** more code to keep correct, and it would not
  change PDFium's and pdf.js's handling of rotated pages.
- **`/F 28` (as MuPDF and Acrobat write notes):** makes no difference in Acrobat and
  MuPDF, which treat notes as NoZoom and NoRotate anyway. PDFium and pdf.js don't honour
  those flags. Rule 6 says `/F 4`.
- **A rotating appearance `/Matrix`:** see finding 1.
