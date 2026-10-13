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
| `turned-normal.pdf`, `turned-rot90.pdf` | Three pages, each with a red text box ("Typed on a page shown at 90 degrees", then 180 and 270, and "Second line") typed as on a page turned that far on screen only: on page 1 its text reads when the page is turned a quarter clockwise, on page 2 upside down, on page 3 when turned a quarter counter-clockwise. The second file's pages are also rotated 90° in the document. Each must be visible, and when edited in Acrobat keep its direction. |
| `types-crop.pdf` | The same on pages with a cropped, offset visible area. |
| `types-userunit.pdf` | The same on pages with a UserUnit of 2 (pages twice the usual size). |
| `edited.pdf` | `types-normal.pdf` after edits: the highlight is pink with the note "Edited note", the green underline on page 2 is now a green highlight, the blue squiggly on page 4 is now a blue underline, the note moved to the lower middle, the drawing resized to a wide box with 4 pt strokes, the text box retyped at 16 pt, and the strikeout deleted. |
| `replies.pdf` | `types-normal.pdf` with a reply ("Reply on page 1 — ünïcödé") to the highlight on page 1 and one ("Reply on page 5 — ünïcödé") to the sticky note on page 5, by "Lectrix Harness" (ADR 0012). |
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

## `replies.pdf`

In each reader:

- [ ] The highlight's and the note's threads in the comments list show the reply under
      them, with its author and text (accented letters intact).
- [ ] Pages 1 and 5 look exactly like `types-normal.pdf`: no extra note icon anywhere.
- [ ] Pointing at (or clicking) the sticky note on page 5 shows the note's own text,
      "Sticky note text", not the reply's.
- [ ] A reply written there (in Acrobat or Foxit) shows in Lectrix's annotation list under
      the same annotation.

If Acrobat or Foxit don't list Lectrix's replies, note it: ADR 0012 has the fallback.

## Repair

- [ ] `problems-before-repair.pdf`: note which annotations each reader does not show (for
      example, Edge shows no highlight without an appearance).
- [ ] `problems-repaired.pdf` and `app-repaired.pdf`: every annotation shows in every
      reader, with the same text, colour and author as before, in the same place.

## Your own files

If you have PDFs whose annotations display wrongly in Acrobat, open one in Lectrix: the
annotation list (the Annotations panel, in the right pane at first) marks the ones that need repair. Run Document > Repair annotations,
save a copy, and check the copy in Acrobat.

- [ ] Open a book with a linked table of contents: the hand shows over its lines, the
      line is tinted and a tooltip names the page, a click goes to the chapter, and Alt+Left
      comes back. Its bookmarks read as in Acrobat.
- [ ] Rest the pointer on a web link in a document: the tooltip shows its address. Click
      it: it opens in the default browser, and Lectrix names the address it opened. The
      same for a bookmark that is a web link.
- [ ] Open a PDF/A file (Word: Save As > PDF > Options > "PDF/A compliant"; LibreOffice:
      Export as PDF > Archive). Lectrix shows the bar "This file claims compliance with the
      PDF/A standard…" with the same level Acrobat names in its Standards panel, and the
      annotation tools are disabled.
- [ ] After **Enable editing**, add a bookmark and save: no notification about PDF/A, and
      Acrobat still opens the file with its own PDF/A bar.
- [ ] Add a highlight and save: Lectrix says the file "no longer claims to be PDF/A-…".
      Acrobat opens it without its PDF/A bar, and File > Properties still shows the title,
      author and other details the file had.

# Manual release checklist: installers, crash recovery, accessibility

About 20 minutes. The installers come from CI (the `installers` artifact) or from
`npm run bundle` (`target/release/bundle/`). CI installs and uninstalls both silently and
checks the registry; what it cannot check is the pages you click through and what Windows
does afterwards.

## Installers

- [ ] In Explorer, `Lectrix_<version>_x64-setup.exe` shows Lectrix's icon (the `.msi` shows
      Windows' own installer icon, which a file of that type can't change). After
      installing, so does `uninstall.exe` in the install folder.
- [ ] `Lectrix_<version>_x64-setup.exe` from an administrator account: a UAC prompt comes
      first, then a "Choose Users" page with "Install for anyone using this computer"
      preselected. Pick "Install just for me": the folder page shows
      `%LOCALAPPDATA%\Programs\Lectrix` (ADR 0015).
- [ ] After the folder page comes a "PDF files" page with "Open PDF files with Lectrix"
      checked and a sentence about Windows asking. Leave it checked and finish. Lectrix
      starts; drag a PDF from Explorer onto it, and it opens (it was not started as
      administrator).
- [ ] Double-click a PDF in Explorer: Windows asks which app to use and offers Lectrix (or,
      if you had chosen a default before, it keeps it; Lectrix is then listed under Open with
      and in Settings > Apps > Default apps). Choosing Lectrix opens the file in it.
- [ ] With Lectrix as the default app, Explorer shows the Lectrix PDF icon (the page with
      the blue swan and "PDF", the swan alone at small sizes) for PDFs, in the Details and
      Large icons views. If the old icon stays, sign out and in again: Explorer caches icons.
- [ ] The install folder holds `LICENSE.txt`, `THIRD_PARTY_NOTICES.md` and
      `THIRD_PARTY_LICENSES.md`.
- [ ] Help > About shows the installed version and "by Mark Ingrem"; the name turns blue
      under the pointer and opens github.com/pan4ratte, and "Source code on GitHub" opens
      github.com/pan4ratte/lectrix, both in the default browser.
- [ ] Uninstall (Settings > Apps): Lectrix disappears from Open with and Default apps.
- [ ] Install again for anyone using this computer, with the box unchecked: Lectrix is in
      `C:\Program Files\Lectrix` and not offered for PDFs. Uninstall.
- [ ] `Lectrix_<version>_x64_en-US.msi`: the same two runs (the page comes after the folder
      page; it needs administrator rights).
- [ ] Fedora (GNOME Software), final versions only (a beta has no rpm): install the `.rpm`, then
      close Software and open it again. Under Installed, Lectrix has its icon, and its page
      shows the icon, "View, annotate and combine PDF files" and the description. The app
      grid shows the icon too.

## Updates (from the second release on)

Needs an installed earlier release and a newer one published (ADR 0011,
`docs/releasing.md`).

- [ ] Start the earlier version: within a few seconds a notice offers the new version.
      Not now hides it. Restart Lectrix: no notice, since GitHub answered less than a day
      ago. Help > Check for updates still offers it.
- [ ] Close Lectrix, then in `%LOCALAPPDATA%\org.lectrix.pdf\state.json` set
      `last_update_check` (seconds since 1970) to more than a day ago, or remove it: the
      next start offers the new version again.
- [ ] From 1.1.0 on: the notice's "See what's new" lists each version newer than the one
      installed, in `CHANGELOG.md`'s words, with Update at the bottom, which starts the
      download.
- [ ] Update: a progress bar fills, then "ready to install" (Windows) or "installed".
- [ ] With a document that has unsaved changes, Restart now asks about saving first. Then
      Lectrix closes, the installer shows its progress window (Windows; an administrator
      answers a UAC prompt first, ADR 0015) and Lectrix opens again; Help > About shows
      the new version, and it is still in the same folder. It started on the start screen:
      "Updated to <version>" sits to the right of the name; See what's new opens the
      changelog. The next start doesn't show it.
- [ ] After an update, open a PDF from File Explorer as the first start: a notification
      "Lectrix was updated to <version>" with What's new, instead of the start screen's.
- [ ] Windows: Update, then Later, then close Lectrix: the installer runs and does not
      reopen Lectrix. The next start is the new version.
- [ ] If Lectrix was installed without "Open PDF files with Lectrix", it is still not
      offered for PDFs after updating.
- [ ] Updating from `1.0.0-beta.1` (where PDFs showed the app icon), with Lectrix as the
      default app: PDFs show the Lectrix PDF icon afterwards.
- [ ] Settings: turn off "Check for updates automatically", and remove
      `last_update_check` as above: no notice at the next start.
- [ ] With that setting still off, Help > Check for updates…: "Checking for updates…",
      then the notice for the new version (on the earlier version) or "Lectrix is up to
      date" (on the latest). Don't ask again, then Check for updates… once more: the same
      version is offered.
- [ ] With the network unplugged, Check for updates…: a plain error naming the internet
      connection.

## Organizing pages

- [ ] Open a book with bookmarks and a linked table of contents, and choose **Document >
      Organize pages**. Drag a page to another place with the mouse, then drag three
      selected pages together: a line shows where they will go, and they land there.
- [ ] Draw a rectangle over several pages, starting between two of them: the pages it
      touches are selected as it grows. Drag down past the edge: the grid scrolls.
- [ ] Pinch on the touchpad over the grid, then turn the mouse wheel with Ctrl held: the
      pictures follow the fingers and glide for the wheel, and the page under the pointer
      stays under it. Point at a page: its label gives way to a bar with the label and
      the buttons that rotate and delete that page.
- [ ] Drag a PDF from File Explorer over the grid: a line shows the gap under the pointer
      and follows it (also on a second monitor with another display scale), and the grid
      scrolls near its top and bottom edges. Drop it: the dialog names that place, and the
      inserted pages are there, selected. Drop two files at once, one of them protected by
      a password: the dialog comes after the password and lists both, and both go in.
- [ ] On the start screen, choose **Organize pages…** and pick a book: its pages show as
      a grid. Press **Larger pages** until it is disabled: one page fills the window, in
      the middle, and the pages come one under another. Make the window smaller: the
      page shrinks to fit. Step back down to several columns: they stay centered.
- [ ] Select a chapter's pages and choose **Extract**: a new document opens in a tab of
      its own, named "Pages from …", marked unsaved, with its pages as a grid and their
      bookmarks, annotations and page labels. Move a page in it, then press Ctrl+S: the
      dialog starts in the book's folder with that name. Save, and open the file in
      Acrobat Reader: it is the same there.
- [ ] Extract pages again and close the new tab: Lectrix asks whether to save the
      document. Choose Don't save; `%LOCALAPPDATA%\org.lectrix.pdf\unsaved` is then
      empty. Extract once more, wait two minutes, end Lectrix in Task Manager and start
      it again: it offers to restore the document, which comes back unsaved, and Save
      asks where.
- [ ] While reading, open the Document menu and the menu of a page in the Pages panel:
      neither has page labels, and the bar above the pages has no Organize pages
      button. Choose **Document > Organize pages**, select the front matter's pages,
      choose **Labels**, pick i, ii, iii and Apply; then the body's first page to its
      last with a prefix: every page's label under it follows as you type, and the
      pages you did not pick keep their labels. Narrator reads the choices and the
      fields. Point at a page: the bar with its label and buttons keeps clear of the
      lower edge of the page's block.
- [ ] Delete a chapter's first page: Lectrix asks first, then says what went with it and
      offers Save As (optimized). Back in the viewer, the table of contents' other lines
      still lead to their chapters, and Undo brings the page and its bookmark back.
- [ ] Save, then open the file in Acrobat Reader, Edge and Firefox: the pages are in the
      new order, the bookmarks lead to the right pages, and the page labels read as
      Lectrix shows them.
- [ ] Delete a page and choose Save As (optimized) from the notification: the new file is
      smaller, and searching it for a phrase from the deleted page finds nothing.
- [ ] In the largest book you have with a long list of bookmarks (thousands), select a
      page in the grid and hold Alt+Shift+Right: the page moves along without the grid
      falling behind the key. Press Ctrl+Z as many times, then go back to reading: the
      bookmarks lead to the same pages as before.

## Window

- [ ] Resize and move the window, close Lectrix and start it again: same size and place.
- [ ] Maximize it, close and start again: maximized; Restore gives the size from before.
- [ ] Minimize it and close it from the taskbar: the next start is as before minimizing.
- [ ] With a second monitor: leave the window on it, close, unplug the monitor, start:
      the window is on the remaining monitor, its title bar reachable.

## Full screen

- [ ] With a document open, press F11: the title bar, the view bar and the side panes slide
      out of the window at once, smoothly also the first time after starting Lectrix, with
      the pages under them and no gap where a bar was; the window then fills the screen,
      taskbar included. The toolbar floats with the page and zoom controls in it, and a
      notification says how to leave (the first time only). F11 again: the bars and panes
      slide back in over the pages, smoothly, the window returns to where it was, and that
      notification is gone. The same from a maximized window, which is maximized again
      afterwards.
- [ ] Rest the pointer at the left edge of the screen, then at the right: that side's pane
      slides over the pages, which do not move, and goes a moment after the pointer has
      left it. Drag the pages' scrollbar up and down: the right pane does not come during
      the drag.
- [ ] Hold Alt: both panes show, and the toolbar when Settings show it only near its edge.
      Let go: they go. No system menu opens, and the next key pressed does what it does.
      Alt+Left still goes back, Alt+Tab switches away and coming back shows no pane left
      behind, and Alt+F4 closes Lectrix.
- [ ] With the Highlight tool, hold Alt and drag on the page: an area highlight is drawn,
      and panes that had come go as the drag starts.
- [ ] With a second monitor beside the screen: moving the pointer across to it brings no
      pane.
- [ ] Close Lectrix in full screen (Alt+F4) and start it again: a window as before full
      screen, not one the size of the screen.
- [ ] With the toolbar docked (as it starts; Settings > Toolbars): full screen floats it,
      and leaving docks it again. The same with it docked at the left or the right of the
      pages: the bar there slides away with the others and comes back.
- [ ] Settings > Toolbars > Where it floats: Left, then full screen. The toolbar is at the
      left of the screen with everything stacked: the page controls (the position in small
      text under the page box), the zoom controls, the tools. The zoom level's menu, the
      page display's and the visual adjustments open to its right, over the pages. The
      same at the right, where they open to its left and the toolbar keeps clear of the
      pages' scrollbar.

## Visual adjustments

- [ ] In a document with photographs and highlights, open Visual adjustments (the button
      after the page display) and turn on Invert colours: the pages and their thumbnails
      turn dark with light text, highlights keep their colour, and the rest of the window
      is as it was. Select text, search, and type a text box: all show on the dark page.
- [ ] Move Brightness and Contrast, turn on Greyscale: the pages follow at once. The tab
      shows no unsaved changes, and the file opens unchanged in Acrobat.
- [ ] Move Sepia in a document with photographs: the paper takes the tone of old paper,
      the more the further; the text stays black and the photographs keep their colours,
      warmed. With Invert colours on too, the paper stays dark and the light text is warm.
      Scroll quickly with Sepia on: as smooth as without.
- [ ] Settings > Appearance > Visual adjustments: move Sepia and turn on Greyscale. Every
      open document follows, in every window, and so does a file opened next. Adjust one
      document in its own panel: it keeps that when Settings change again, and Reset in
      its panel puts it back to what Settings say. Reset in Settings: the documents are as
      their files have them, except any still adjusted by itself.
- [ ] With a state file from 1.1 (a file left inverted there, and one never adjusted): the
      first opens inverted, the second follows Settings.
- [ ] Scroll a long document quickly with adjustments on: as smooth as without.
- [ ] Close Lectrix and open the file again: adjusted as it was; another file is not.

## Pages turned on screen

- [ ] In a scan with a page that lies on its side, go to that page and press Rotate
      current page clockwise in the Pages panel: the page and its thumbnail turn, the
      other pages stay, and the tab shows no unsaved changes. Select text on the turned
      page, follow a link, search for a word and make a highlight: each is where the
      pointer is, and the highlight is on the text.
- [ ] On that turned page, type a text box, place a note and Alt-drag an area highlight:
      the box is typed upright as you see the page and grows downwards, and the note's
      icon is below and right of the click (lying on its side with the page). Save, and
      open the file in Acrobat Reader: the page is as the file has it, the box's text
      runs the way the scan's text does, and the note's icon is upright. Double-click the
      box in Lectrix with the page turned back: its editor lies as its text does.
- [ ] Turn another page from its thumbnail's menu, then choose View > Rotate view
      clockwise: every page turns, the two turned pages a quarter further than the rest.
- [ ] Close Lectrix and open the file again: the pages are turned as they were left. The
      file opens unchanged in Acrobat.
- [ ] Open a PDF/A file (the banner shows): its pages turn the same way.
- [ ] With a page turned, choose Document > Organize pages: the grid shows the pages as
      the file has them. Move the turned page to another place and go back to reading:
      it is still turned, in its new place.

## Tabs and windows

- [ ] With three documents open, drag a tab along the others: it follows the pointer, the
      others make way, and it stays where it is dropped.
- [ ] Add a highlight, then drag that tab down onto the page and let go: a new window opens
      under the pointer with the document on the same page, still marked unsaved, and Undo
      takes the highlight back. The first window no longer has the tab.
- [ ] Drag the tab from the new window onto the first window (anywhere on it) and let go:
      it joins the first window's tabs and the emptied window closes.
- [ ] Drag a tab out to a second monitor, if there is one: the window opens there, its
      title bar reachable.
- [ ] With two windows, each with an unsaved document: the taskbar names each after its
      document. File > Exit asks in each window; Cancel in one keeps that window and the
      one Exit was picked in. Close one window with its own button: only that one closes.
- [ ] With two windows, double-click in Explorer a PDF that is open in the window behind:
      that window comes to the front with the tab shown.

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
      each control is announced with a sensible name (tools, page box, zoom, pane buttons and panel tabs,
      bookmarks, annotation rows). On the page box and on a document's tab, Narrator also
      reads what their tooltips say (how to use the page box; the file's path). Turn
      Narrator off.
- [ ] Rest the pointer on a view bar button, an annotation tool and the window's Close
      button, then Tab to a panel tab: each shows Lectrix's own tooltip, with a tail
      pointing at the control, in both appearances. No tooltip is drawn by Windows.
- [ ] Focus rings, the selected tab, the active annotation tool and text fields are
      easy to see in Lectrix blue in both the light and the dark appearance.
