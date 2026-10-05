# Manual release checklist: annotations

AGENTS.md section 9: machines check every commit; a person checks before every release.
This list is for that person. It takes about 30 minutes.

## The files

Run `python tests/interop/run.py phase5` and the end-to-end tests (`npm test` in
`tests/e2e`). They write the files to `target/test-output/manual/phase5/`:

| File | What is in it |
| --- | --- |
| `types-normal.pdf` | One annotation per page, all written by Lectrix: 1 highlight with a note ("Highlight note — ünïcödé"), 60% opacity; 2 green underline; 3 red strikeout; 4 blue squiggly underline; 5 sticky note ("Sticky note text"); 6 blue drawing, two strokes, 80% opacity; 7 red text box, two lines; 8 area highlight (a rectangle, 50% opacity). Author "Lectrix Harness". |
| `types-rot90.pdf` | The same on pages turned 90°. The four text markups follow the text, so they turn with it. The note, drawing, text box and area highlight were placed the way you place them in the app on a turned page: at the same spot on screen as in `types-normal.pdf` (top left of the page as shown), upright as you see it. Relative to the text they are therefore elsewhere than in `types-normal.pdf`, and not turned with it. |
| `types-crop.pdf` | The same on pages with a cropped, offset visible area. |
| `types-userunit.pdf` | The same on pages with a UserUnit of 2 (pages twice the usual size). |
| `edited.pdf` | `types-normal.pdf` after edits: the highlight is pink with the note "Edited note", the green underline on page 2 is now a green highlight, the blue squiggly on page 4 is now a blue underline, the note moved to the lower middle, the drawing resized to a wide box with 4 pt strokes, the text box retyped at 16 pt, and the strikeout deleted. |
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
- [ ] It appears in the reader's comments list, with the author "Lectrix Harness" and, for
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
- [ ] In `edited.pdf`, the comments list calls the annotation on page 2 a highlight and the
      one on page 4 an underline (types changed in Lectrix), and each can be edited there.
- [ ] Nothing was lost: count the annotations in each reader's comments list (7 and 5).

## Repair

- [ ] `problems-before-repair.pdf`: note which annotations each reader does not show (for
      example, Edge shows no highlight without an appearance).
- [ ] `problems-repaired.pdf` and `app-repaired.pdf`: every annotation shows in every
      reader, with the same text, colour and author as before, in the same place.

## Your own files

If you have PDFs whose annotations display wrongly in Acrobat, open one in Lectrix: the
annotation pane (title bar, right) marks the ones that need repair. Run Document > Repair annotations,
save a copy, and check the copy in Acrobat.

# Manual release checklist: installers, crash recovery, accessibility

About 20 minutes. The installers come from CI (the `installers` artifact) or from
`npx tauri build` (`target/release/bundle/`). CI installs and uninstalls both silently and
checks the registry; what it cannot check is the pages you click through and what Windows
does afterwards.

## Installers

- [ ] `Lectrix_0.1.0_x64-setup.exe`: after the folder page comes a "PDF files" page with
      "Open PDF files with Lectrix" checked and a sentence about Windows asking. Leave it
      checked and finish.
- [ ] Double-click a PDF in Explorer: Windows asks which app to use and offers Lectrix (or,
      if you had chosen a default before, it keeps it; Lectrix is then listed under Open with
      and in Settings > Apps > Default apps). Choosing Lectrix opens the file in it.
- [ ] The install folder holds `LICENSE.txt`, `THIRD_PARTY_NOTICES.md` and
      `THIRD_PARTY_LICENSES.md`.
- [ ] Uninstall (Settings > Apps): Lectrix disappears from Open with and Default apps.
- [ ] Install again with the box unchecked: Lectrix is not offered for PDFs. Uninstall.
- [ ] `Lectrix_0.1.0_x64_en-US.msi`: the same two runs (the page comes after the folder
      page; it needs administrator rights).

## Crash recovery

- [ ] Open a PDF, add a highlight, wait two and a half minutes, then end Lectrix in Task
      Manager (End task). Start Lectrix: it asks "Restore unsaved changes?" naming the file.
      Restore: the highlight is back and the tab shows unsaved changes. Save, and check the
      file in another reader.
- [ ] Same again, but answer "Not now": the question comes back at the next start. Then
      "Discard…" and confirm: it does not come back.
- [ ] Quit normally with "Don't save": the next start asks nothing.

## Accessibility

- [ ] Turn on Narrator (Ctrl+Win+Enter). Tab through the window with a document open:
      each control is announced with a sensible name (tools, page box, zoom, sidebar tabs,
      bookmarks, annotation rows). Turn Narrator off.
- [ ] Focus rings, the selected tab, the active annotation tool and text fields are
      easy to see in Lectrix blue in both the light and the dark appearance.
