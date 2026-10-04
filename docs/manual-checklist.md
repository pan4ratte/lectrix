# Manual release checklist: annotations

AGENTS.md section 9: machines check every commit; a person checks before every release.
This list is for that person. It takes about 30 minutes.

## The files

Run `python tests/interop/run.py phase5` and the end-to-end tests (`npm test` in
`tests/e2e`). They write the files to `target/test-output/manual/phase5/`:

| File | What is in it |
| --- | --- |
| `types-normal.pdf` | One annotation per page, all written by Folio: 1 highlight with a note ("Highlight note — ünïcödé"), 60% opacity; 2 green underline; 3 red strikeout; 4 blue squiggly underline; 5 sticky note ("Sticky note text"); 6 blue drawing, two strokes, 80% opacity; 7 red text box, two lines; 8 area highlight (a rectangle, 50% opacity). Author "Folio Harness". |
| `types-rot90.pdf` | The same on pages turned 90°. |
| `types-crop.pdf` | The same on pages with a cropped, offset visible area. |
| `types-userunit.pdf` | The same on pages with a UserUnit of 2 (pages twice the usual size). |
| `edited.pdf` | `types-normal.pdf` after edits: the highlight is pink with the note "Edited note", the note moved to the lower middle, the drawing resized to a wide box with 4 pt strokes, the text box retyped at 16 pt, and the strikeout deleted. |
| `app-annotations.pdf` | Made in the app by the end-to-end test: a highlight, a note ("Hello from the note"), a drawing, a text box ("Typed in a box") on page 1, and a highlight by "E2E Tester" on page 2. |
| `problems-before-repair.pdf` | Eight annotations "another app" wrote with problems (no appearance, corners in the wrong order, too-small bounds, missing keys). Several are invisible in some readers. |
| `problems-repaired.pdf`, `app-repaired.pdf` | The same after Repair annotations (the second one repaired in the app). |

## Readers

Adobe Acrobat Reader, Microsoft Edge, Mozilla Firefox, Foxit PDF Reader.

## For each reader, and each `types-*.pdf` file

For every annotation type (highlight, underline, strikeout, squiggly, note, drawing, text
box, area highlight):

- [ ] It is visible, in the right place (on the "quick brown fox" line for the four text
      markups; near the top left for the note; across the middle for the drawing).
- [ ] The colour and the opacity look as listed above (the highlight lets the text through).
- [ ] It appears in the reader's comments list, with the author "Folio Harness" and, for
      the highlight and the note, its note text (with the accented letters intact).
- [ ] It prints (print to PDF or paper; one page is enough per file).
- [ ] It can be edited (change its colour or note) and deleted.

Expected differences, not failures:

- **Sticky notes on `types-rot90.pdf`:** Acrobat and Foxit show the icon upright at the
  spot it was placed. Edge and Firefox show it turned with the page, next to that spot
  (ADR 0006).
- **`types-userunit.pdf` in Edge:** the page is shown at half the size of the other
  readers (Edge ignores UserUnit). The annotations stay on the right text.

## `edited.pdf`, `app-annotations.pdf`

- [ ] Everything listed above shows, with the edits, in every reader.
- [ ] Nothing was lost: count the annotations in each reader's comments list (7 and 5).

## Repair

- [ ] `problems-before-repair.pdf`: note which annotations each reader does not show (for
      example, Edge shows no highlight without an appearance).
- [ ] `problems-repaired.pdf` and `app-repaired.pdf`: every annotation shows in every
      reader, with the same text, colour and author as before, in the same place.

## Your own files

If you have PDFs whose annotations display wrongly in Acrobat, open one in Folio: the
Annotations panel marks the ones that need repair. Run Document > Repair annotations,
save a copy, and check the copy in Acrobat.
