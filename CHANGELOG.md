# Changelog

<!--
What changed in each version of Lectrix, written for the people who use it. Help > What's new
shows this file, and each GitHub release is written from it. How to write it (the sections,
the wording, translations): docs/releasing.md, "The changelog".
-->

## Unreleased

### Start screen

- Click Lectrix’s icon or name in the title bar to see the start screen without closing your documents. Click it again, or a tab, to go back.
- The tools are plain until the pointer is on them, and take Lectrix’s colours under it.
- Switching between the list and the grid of recent files is animated.

### Viewer and tabs

- A document can have a window of its own: drag its tab away from the others and let go, or right-click the tab and pick **Move to new window**. Drop a tab on another Lectrix window to move it there. Unsaved changes, and where you were in the document, go with it.
- A tab you drag along the others follows the pointer while they make way for it, and its right-click menu has **Move left** and **Move right**.
- The taskbar names each window after the document it shows.
- **File > Exit** closes every window, after asking about unsaved changes in each.
- Links in a document work: click one, such as a line of the table of contents, to go where it leads, and press **Alt+Left** to come back. A web link opens in your browser. The link under the pointer is tinted, and a tooltip says where it leads: the page, or the web address.
- On Linux, pinching on a touchpad zooms the document instead of the whole window, and pages appear sooner while scrolling.
- A document stays where you were when you switch to another tab and come back, instead of returning to its first page.
- Drag the start or the end of selected text to select more or less of it.
- With text selected, **Shift** and the arrow keys select more or less: a character or a line at a time, and a word or a paragraph with **Ctrl** too.
- A double-click selects the whole word and a triple-click the whole line, instead of only the part before the pointer.
- A PDF/A file (the standard for documents kept for the long term) opens for reading only, as in Acrobat, so that it is not changed by accident. A bar above the pages says so, and **Enable editing** lets you change the file.
- Saving annotations or inserted pages into a PDF/A file removes the file’s claim to be PDF/A, and Lectrix tells you: it can’t guarantee that they follow the standard. Bookmarks, page labels and rotated pages keep the claim.

### Annotations

- The annotation list shows the annotations of a page from its top to its bottom, not in the order they were made.
- A comment is no longer lost when you click on the page after typing it, and no longer goes to another annotation when you click that one.
- **Esc** in a comment keeps what you typed instead of discarding it.
- While the annotation list is open, a selected annotation no longer gets a bar or a comment panel over the page. Its row in the list has their tools instead: colour and opacity, stroke width or font size, type, and delete.
- With the list open, a new note, or a new highlight that opens its comment, puts the cursor in its comment in the list.
- Searching or filtering the annotation list hides the other annotations on the page too, while the list is open. A new annotation the filters would hide clears them.
- A comment in the annotation list stays where it is when you select its row, instead of shifting, and the type, author and date above it line up with it.
- A colour button shows its colours when the pointer rests on it, without a click.
- The colours sit closer together, the custom colour’s button is beside the opacity slider, and its picker slides open below.

### Bookmarks

- Bookmark titles that the program which made the file padded with invisible characters no longer end in a row of � marks; they show as in Acrobat.
- A bookmark that is a web link opens in your browser.

### Settings

- **Preset colours**, under Annotations, shows the 18 colours every tool offers and lets you replace any of them with your own. **Restore colours** puts the original ones back.
- **Open the colours when the pointer is on their button**, under Annotations, can be turned off to open them with a click.
- **Highlight the link under the pointer** and **Show where a link leads**, under General, can be turned off.
- **Show handles on selected text**, under General, puts a handle at the start and at the end of selected text, to drag it by.
- **Open PDF/A files for reading only**, under General, can be turned off to open such files ready to edit.
- **Shortcuts** lists every keyboard shortcut, with a search field, and lets you change them: press a shortcut, then the keys it should have. Commands without one can get one, and menus and tooltips show the keys you chose.
- **Restore defaults…**, under General, puts every setting back to how Lectrix came, after asking.
- The theme is chosen with a three-way switch (System, Light, Dark) instead of a list.
- Sliders have a cleaner look, like the switches: no outline, a round handle that stands out from the filled part of the track, and a glide from step to step.

### Installation and updates

- Lectrix checks for updates once a day instead of at every start. The setting is now **Check for updates automatically**.
- Before you update, **See what’s new** in the update notice shows what the new version brings.

### General

- Tooltips have Lectrix’s own look in both themes, with a small tail pointing at what they describe. They also show for the control you reach with **Tab** or the arrow keys.
- **Help > What’s new** shows what changed in each version. The first time Lectrix starts after an update, it offers it beside its name on the start screen, or in a notification.
- On Windows, saving no longer fails when an antivirus or Windows Search looks at the file at that moment.
- Saving again after a save failed (for example because the file was open in another program) no longer damages the file. A file damaged this way opened as a blank document with no pages; Lectrix now repairs it when you open it, with everything in it, and **Save As** writes it sound again.
- After a **Save As (optimized)** that failed, the next save no longer loses changes.
- In many documents, **Undo** did nothing the first time after Lectrix had written a crash recovery copy. It undoes your last change again.

## 1.0.0

The first release of Lectrix: a PDF editor whose annotations, bookmarks and page labels open correctly in Acrobat and every other reader.

### Start screen

- Open a file, combine files, or reopen a recent file from a list or from previews of their first pages.

### Viewer and tabs

- Open documents in tabs. Scroll through them continuously or a page at a time, one or two pages side by side.
- Zoom, search, select and copy text, and go back and forward between places you jumped to.
- Page thumbnails, in a column or a grid. Rotate pages.

### Annotations

- Highlight, underline, strike out and squiggly-underline text, add notes, draw, and add text boxes.
- See every annotation in a list, then search, sort, filter and reply to them.
- **Document > Repair annotations** fixes annotations made by other apps so they show everywhere.

### Bookmarks

- Add, rename, reorder, nest and delete bookmarks, and point them at another place.

### Page labels

- Number front matter i, ii, iii and the rest 1, 2, 3, or give appendices a prefix.

### Combining files

- Combine several PDFs into one, arranging, removing and rotating pages first. Bookmarks and page labels come along.
- Insert pages from another file into an open document.

### Settings

- Your author name, the theme, how documents open, and where the annotation toolbar sits.

### Installation and updates

- Installers for Windows, macOS and Linux. Lectrix checks for updates and installs them when you say so.
