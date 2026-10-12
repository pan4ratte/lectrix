# 0017: Repair leaves mirrored QuadPoints as their app wrote them

- Status: accepted (the user's decision, 2026-10-12)
- Date: 2026-10-12

## Context

A user reported two things about Repair annotations:

1. After repairing a file annotated in PDFgear, PDFgear showed none of its annotations, or
   only some of them.
2. In another file, Lectrix offered to repair annotations again after it had repaired
   them, though no other app had touched the file in between.

The sample file (a scanned book, annotated on a Mac: producer "Quartz PDFContext", the
annotation dictionaries as Apple's PDFKit writes them) has 26 highlights. Each has an
appearance stream, no `/NM`, no `/P`, and `/QuadPoints` with the bottom edge first:
lower-left, lower-right, upper-left, upper-right. Section 5.3 as written made repair do
three things to each: add `/NM`, add `/P`, and rewrite the corners in Acrobat's order
(upper-left, upper-right, lower-left, lower-right).

What was measured on that file:

- The repaired file is the original plus one incremental update of 26 annotation
  dictionaries. `qpdf --check` reports nothing new; MuPDF reads it without repair.
- PDFium draws every annotated page pixel for pixel the same before and after: it draws
  the appearance streams, which repair did not touch.
- The app's path (render, repair, render, save in place, reopen) writes the same bytes as
  `pdf-cli annot repair`, and a second scan is clean.

So in the readers Lectrix promises (section 5), the repair changed nothing that shows. Of
its three changes, `/NM` and `/P` are keys Acrobat writes on every annotation; the corner
order is the only one that changes data the writing app reads back. PDFgear was not
available to test (it is not installed here, and the reporter's is the macOS one), so that
the corner order is what it trips on is an inference, not a measurement.

**Two kinds of "not Acrobat's order".** A quad that is not in Acrobat's order is one of:

- **Crossed**: the spec's counter-clockwise order (LL, LR, UR, UL), a clockwise one, a
  scramble. Read in Acrobat's order its two long edges run in opposite directions, and a
  reader that draws from the corners fills a bow-tie.
- **Mirrored**: the bottom edge first (LL, LR, UL, UR), or each edge from right to left
  (UR, UL, LR, LL). Both edges run the same way; read in Acrobat's order it is still the
  same four-sided shape. pdf.js reads every quad as its bounding box, whatever the order
  (`getQuadPoints` in the pinned `pdfjs-dist`).

What a mirrored quad costs shows only when a reader draws the annotation again from its
corners (it has no appearance, or the user edits it there): MuPDF and Acrobat round a
highlight's ends the other way, and draw an underline or squiggle along the edge they take
for the lower one, which is the upper one.

**The second report** turned out to be Lectrix's own. For right-to-left text MuPDF lists a
line's characters in reading order, running leftwards, each with an upright quad.
`text::range_quads` took the first character's left edge and the last one's right edge,
as for left-to-right text: a quad written from right to left (mirrored), and one that fell
short by a character at each end. Lectrix then reported its own annotation as having
corners in the wrong order, after every mark on Hebrew or Arabic text. Repair made it
worse: it read such a quad as text on its head with the bottom edge first, so an underline
drawn again from the repaired corners would have run above its line.

## Decision

1. **Mirrored corners are not a problem to report, and repair does not rewrite them, while
   the annotation has an appearance.** Every reader shows that appearance; the app that
   wrote the corners reads them back in its own order. This is rule 10 (leave others' work
   alone) and "repair never changes position" taken at their word. Only crossed corners
   are reported ("needs repair") and rewritten.
2. **Lectrix puts corners in Acrobat's order whenever it draws the annotation itself**:
   when repair generates a missing appearance (or has to draw a new one for a bigger
   `/Rect`), and on every edit of text markup, before MuPDF's synthesis, which reads the
   corners in Acrobat's order. Crossed and mirrored alike; the coordinates never change.
   Rule 3 (everything Lectrix writes or draws has its quads in Acrobat's order) stands.
3. **Selected text becomes quads by where its characters are on the page**, not by their
   place in the range: one quad for each run of selected characters that lie side by side,
   from the start edge of the first one on the page to the end edge of the last. A range
   of left-to-right or of right-to-left text is one quad, with its corners named as the
   glyphs stand; a range that crosses between the two directions is several.
4. **Putting corners in order prefers upright text.** The orders other apps use are tried
   first, as before; if the result would be text on its head on the page as shown, it is
   turned upright. (A quad written from right to left otherwise reads as upside-down text
   with its bottom edge first.)
5. **A quad is reported only if it can be put right, and repair fixes quads one by one.**
   A quad whose corners lie in one line has no order; it used to keep an annotation's
   "needs repair" badge for good, since repair wrote nothing if any one quad of an
   annotation had no order.

## Consequences

- A file annotated in a PDFKit app is still offered a repair, for the `/NM` and `/P` that
  PDFKit leaves out, and gets only those. Its corners and appearances stay byte for byte.
- An annotation with mirrored corners that is later edited in Acrobat is drawn there from
  those corners: slightly different highlight ends, an underline on the upper edge. That
  is what Acrobat does with such a file today, without Lectrix; repair no longer prevents
  it. Editing the annotation in Lectrix first does.
- Marks Lectrix 1.0 made on right-to-left text keep their mirrored corners and stay a
  character short at each end (repair never moves corners). They are no longer reported,
  and an edit in Lectrix puts their corners in order.
- If PDFgear turns out to trip on `/NM` or `/P` instead, this decision does not help it,
  and those two keys would need the same question. Three copies of the sample with one
  change each were made for the reporter to try.

## Alternatives considered

- **Keep rewriting every quad that is not in Acrobat's order.** What section 5.3 said.
  It protects a later edit in Acrobat at the price of rewriting data that displays
  correctly everywhere, in every file PDFKit apps write, and it is the likely cause of the
  first report.
- **Leave mirrored highlights alone, rewrite mirrored underlines and squiggles**, since
  only those are drawn in the wrong place when redrawn. It would still rewrite the writing
  app's data for a difference that shows only after an edit elsewhere.
- **Stop reporting missing `/NM` and `/P`**, so PDFKit files need no repair at all. Not
  taken: nothing shows they do harm, and section 5.3 lists them.

## Tests

`annot::quads` unit tests (crossed and mirrored orders, upright preference, corners in one
line); `text::tests::a_range_runs_from_its_first_edge_on_the_page_to_its_last`;
`annotations::mirrored_corners_stay_as_written_while_the_annotation_keeps_its_appearance`
(a PDFKit-style file: nothing reported for the corners, repair keeps them and the
appearance, draws the one without an appearance from ordered corners, and leaves nothing
for a second repair; an edit orders them);
`annotations::right_to_left_text_is_marked_whole_and_needs_no_repair`.
