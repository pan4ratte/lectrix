# Changelog

<!--
What changed in each version of Lectrix, written for the people who use it. Help > What's new
shows this file, and each GitHub release is written from it. How to write it (the sections,
the wording, translations): docs/releasing.md, "The changelog".
-->

## Unreleased

### Viewer and tabs

- Text in documents whose fonts don't say which letter each character is, as many Georgian, Armenian and Indian documents are written, can now be selected, copied and searched. Before, copying gave only question marks, search found nothing and a double-click selected a single letter.
- A double-click selects the whole word in Hindi, Bengali, Tamil and other scripts with vowel signs, and in Arabic and Hebrew with vowel points. Before, the selection stopped at the first vowel sign or point. Ctrl with the arrow keys moves by whole words there too.
- In Thai, Chinese and Japanese, which are written without spaces between words, a double-click selects one word instead of the whole run of text, and Ctrl with the arrow keys moves a word at a time.
- The page's context menu moves with the page when you scroll, instead of staying put on the screen.
- Pressing Esc to close a context menu no longer also clears the selected text or deselects the annotation the menu was for.

### Annotations

- Right-click an annotation for **Copy comment** and **Copy selection**, which copies the text a highlight, underline, strikeout or squiggly line marks. The annotation list's menu has both too. With text selected, **Copy selection** copies that text, as the menu's **Copy** did.
- **Copy** on the quick tools over selected text shows a tick when the text is copied, then the quick tools close. The text stays selected.
- Right-clicking an annotation no longer opens its comment next to the menu.
- A comment opened for a new note, by **Highlight with note** or by a double-click, closes when you leave it empty, instead of staying open as an empty box.

## 1.3.0

### Start screen

- A star on a recent file pins it to the top of the list, where it stays however many other files you open. In the grid of previews, the star is in the top left corner of the preview.
- Right-click a recent file to open it, pin it, show it in its folder or remove it from the list.
- On macOS and Linux, clicking **Open…**, **Combine files…** or **Organize pages…** while recent files are listed as a table opened one of the recent files instead. The tools now do what they say.
- **Unlock…** on the start screen and in the **File** menu removes a PDF's password and restrictions, so it opens, prints and copies anywhere without asking. Enter the file's password if it has one, then save the file itself or an unlocked copy. With a protected document open, **File > Unlock…** unlocks that document.
- A protected document shows a lock before its name in its tab. Click it to see the document's protection.
- **Protect…** on the start screen and in the **File** menu gives a PDF a password to open it, restrictions on printing, changing and copying it, or both. Save the file itself or a protected copy. The file is encrypted with AES-256, which Acrobat, Edge, Chrome, Firefox and Foxit all open.

### Viewer and tabs

- Show documents side by side: drag a tab to the left or right edge of the pages, or right-click it and choose **Show beside the current tab**. Up to four documents share one tab, each in its own column with a header, and you can resize the columns by dragging the line between them. **View > Side by side** has the same commands.
- Show one document twice, side by side, to read two parts of it at once: right-click its tab and choose **Show twice, side by side**. Each view has its own page and zoom, and a change made in one shows in both.
- With documents side by side, the lock in the view bar scrolls them together, each keeping its distance from the others: page 3 beside page 10 stays page 3 beside page 10. Two views of the same document also share their zoom and rotation.
- Pages with JPEG pictures appear sooner. In a book scanned as JPEG, each page takes about a third less time to draw, its thumbnails half the time, and a page you jump to is there sooner. A book whose cover is one large photo shows it about a fifth sooner when it opens.
- Text on the pages is sharper. Pages are now shown pixel for pixel as they were drawn. Before, a page was drawn slightly larger than shown and then shrunk to fit, which softened every letter.
- Tabs grow into the row as they open and shrink away as they close, instead of appearing and vanishing at once.
- A tab dragged off the row and let go inside its window now stays there. Only a tab dropped outside the window gets a window of its own.
- While you drag a tab, the document on screen stays on screen. A tab is shown when you let it go.
- After a double-click on a word, keeping the button down and dragging selects whole words, as in a word processor. Before, the drag went on letter by letter. The same goes for whole lines after a triple-click.
- A click in the text puts a blinking text cursor there. The arrow keys move it, and Shift with them (Ctrl+Shift for whole words) selects text from it, so you can select without the mouse.
- As you type in the search bar, a list under it suggests the words of the document that contain what you typed, each with its number of matches. Pick one with the mouse, or with the arrow keys and Enter, to search for it.
- Typing in the search bar no longer scrolls to the first match: the bar counts the matches, and **Enter** or the arrows take you to them, starting from the page you are on. Editing the document while the search bar is open no longer jumps back to a match either.
- Closed and opened again, the search bar keeps its text and shows its matches again. Before, they showed only once you typed again.
- With text selected, **Ctrl+F** puts that text in the search bar and starts at the selection itself.
- With **Greyscale** on in Visual adjustments, annotations keep their colours: only the page turns grey.
- With **Invert colours** on, annotations keep clear colours on the dark page. A yellow highlight is a gold the light text stays readable on, instead of a dark brown, pale highlights show, and black ink and text boxes turn white.
- While you search, marks beside the scrollbar show where the matches are in the document, the one shown in a stronger colour. **Settings > Viewing > Search** can turn them off.
- Going to the next or previous match glides there, as bookmarks do. **Settings > Viewing > Motion** can turn this off.
- The search bar slides in and fades in as it opens, and goes the same way.
- Pressed a second time, a shortcut you gave to the Pages, Bookmarks or Annotations panel hides it again. In the **View** menu, the panel on screen has a check mark, and choosing it again hides it.

### Annotations

- The annotation list keeps the sort you picked in every document, and after Lectrix restarts. **Settings > Annotations** can turn this off, so that each document starts sorted by page.
- With the Select tool, a click on a highlight (or another text markup) selects it, and a drag that starts on it now selects the text under it, as a drag anywhere else does.
- The text cursor works with the Highlight, Underline, Strikeout and Squiggly tools too: select from it with Shift and the arrow keys, let go of **Shift**, and what you selected is marked.
- Marking text that starts or ends on a superscript or a subscript (a footnote number, a squared x) gives a straight highlight, underline or strikeout along the line. Before, it slanted up or down towards the raised or lowered character.
- New annotations share one colour, whatever the tool: a colour picked for a highlight is the one underlines, notes, drawings and text boxes get too. Opacity, stroke width and font size are still kept for each tool. Your current highlight colour becomes the shared one.
- The colour button is always in the annotation toolbar, before the tools, so you can pick a colour before choosing a tool. It is also the first button over selected text, so you can pick the colour before marking the text.
- The number keys **1** to **9** pick the first nine preset colours for new annotations. The presets now start with yellow, green, red and blue, the colours used most for marking text, with each light colour under its brighter colour. Preset colours you changed in Settings keep their colours.
- When the floating toolbar is set to show only near its edge, it now stays while one of its colour panels or lists is open. Before, it disappeared and left the panel or list floating on its own.
- An empty annotation list shows how to start: highlight text with **H** or add a note with **N**. When the search or the filters leave nothing in the list, a button clears them.
- The tail of a comment panel, a bookmark's panel and a tooltip now grows out of its border, so the two read as one shape. Before, the tail sat over the border like a separate piece.
- Selecting an annotation in the annotation list slides its tools open, instead of making them appear at once. They rise out from under the comment, which stays where it is. The selected row also has the same room on every side, so its tools and comment sit evenly in it.
- With **Add a comment after marking text** on, **Ctrl+Z** in the new annotation’s empty comment removes the annotation itself, and so it does in a new sticky note. Before, it did nothing there. Text you typed in the comment is undone first.

- **Repair annotations** no longer changes how another annotation looks when the two share their appearance, as files saved by some apps have them.

### Bookmarks

- An empty Bookmarks panel is clearer: it says what bookmarks are for and has an **Add bookmark** button. A document that can't have bookmarks added says why.
- **Rename** in a bookmark’s menu now works. Before, the title field closed as soon as it opened.
- A bookmark’s properties open beside the bookmark in the Bookmarks panel, pointing at it. Before, they opened at the top right of the pages.
- A bookmark can be given a colour in its properties, with the same colours as annotations, or set back to the default. Its title shows in that colour, as in other PDF readers; a colour from another app showed as a dot before.
- The bookmark properties no longer have a **Go to** button: click the bookmark to go to it.

### Combining files

- The **Smaller pages** and **Larger pages** buttons, and **Ctrl+-** and **Ctrl+=**, change the size of the pages in one step, without the glide. The mouse wheel still glides.
- The pages start at the top left of the grid, however few there are, instead of in the middle.

### Organizing pages

- The same goes for the page grid: the size buttons and keys change the size in one step, and the pages start at the top left.

### Compressing files

- **Compress…** on the start screen and in the **File** menu writes a PDF smaller. Choose a preset (**Lossless**, **High quality**, **Balanced** or **Smallest**) or set the image resolution, the compression (JPEG, JPEG 2000 or as it is), the quality and the colour (greyscale or black and white) yourself, with separate settings for colour, greyscale and black-and-white images if you want them.
- Before anything is saved, Compress shows the size before and after, what it did, and a page as it was and as it will be, side by side at 100%, 200% or 400%. Then save the file itself or a compressed copy, or go back and change the settings.
- **Lossless** changes nothing you can see: pictures are packed tighter without losing a pixel, fonts keep only the characters the document uses, and copies of the same picture or font are kept once.
- Compress can also remove page thumbnails and other apps' editing data, and, when you tick them, attached files, JavaScript and accessibility tags. It offers only what the file has.
- With a document open, **File > Compress…** compresses that document's file.

### Printing

- **File > Print…** (Ctrl+P) prints the document shown, with its unsaved changes. Choose the printer, the copies, and the pages: all, the current one, or a range typed by number or by page label, such as "i-iv, 1-5". Print only the odd or the even pages to print on both sides by hand, or the last page first.
- Fit pages to the paper, print them at their actual size, shrink only those too large, or choose a scale. Landscape pages print on landscape paper, and in a document that mixes them they are turned to fit, upright.
- Choose the paper and, on printers that can, printing on both sides. **Printer settings…** opens the printer's own settings for its trays, quality and finishing.
- Print with or without annotations, and in colour or in greys. A preview shows each sheet as it will print.
- Lectrix remembers your choices for the next time you print. Documents whose security settings forbid printing say so, and those that allow only low-quality printing print at 150 dpi, as in other readers.

### Settings

- A new command switches between the light and the dark theme. It has no shortcut until you give it one in **Settings > Shortcuts**, under View.
- Pressed again while it is open, the shortcut for Settings closes it. The same goes for About and What’s new, once you give them a shortcut.
- Settings are easier to find. A new **Viewing** group holds what changes how you read: page display, selected text, search, links, tooltips and motion. The author name moved to **Annotations**, and every group is in sections named for what they change.
- One switch in **Settings > Toolbars > Quick tools** hides the quick tools over selected text, and keeps your choice of tools for when you turn them back on.
- **Check now** in **Settings > General > Updates** looks for an update at once.
- On macOS and Linux, Settings no longer says the theme and animations follow Windows.

### General

- **Document > Properties…** (Ctrl+D) shows a document's file, its title, author, subject and keywords, which you can change there, when and with what it was made, its security and the fonts it uses. Fonts that are not embedded, and so can look different in other readers, are marked.
- With a document open, **File > Protect…** chooses the password and restrictions it is saved with next, or removes them. **Change protection…** in its properties does the same.
- Files whose security settings forbid changes, such as adding comments, changing pages or copying text, now open with those changes turned off, as in other readers. Before, Lectrix let you make them. When one is refused, the notification has an **Unlock…** button that removes the file's restrictions, with no password needed. Clicking an annotation tool such a file locks, or pressing its shortcut, now says why instead of doing nothing.
- About and What’s new no longer show a blue ring inside them as they open.
- On macOS, the window has the system’s own buttons at the top left, with its shadow, rounded corners and full screen. Dialogs come down from under the title bar as sheets, and questions look like the system’s alerts, with the main button where macOS puts it: press Return to choose it.
- On macOS, the menus are in the menu bar at the top of the screen, arranged as in other Mac apps: Settings, About and Quit are in the Lectrix menu, and there is a Window menu. Shortcuts use the Command key and are shown as macOS shows them (⇧⌘S), and some follow Mac conventions: ⌘Q quits, ⇧⌘Z redoes, ⌘G finds the next match, ⌥⌘G goes to a page, ⌘[ and ⌘] go back and forward, ⇧⌘F turns on full screen.
- On Linux, Lectrix is now offered for PDF files: it is listed under **Open With** and can be chosen as the app that opens PDFs.
- On macOS, PDFs opened from Finder, with **Open With** or from another app now open in Lectrix, whether it is running or not. Before, nothing opened. Finder also lists Lectrix under **Open With** for PDFs.
- On Linux, the window buttons look like those of GTK apps and sit where your desktop puts them: in GNOME, a single close button at the right, unless you have turned on minimize and maximize.

## 1.2.0

### Start screen

- **Organize pages…** is among the tools: pick a file, and its pages show as a grid, ready to be put in order.

### Viewer and tabs

- Visual adjustments have a **Sepia** slider: the paper takes the tone of old paper, as in an e-book, as much as you like, and the text stays black. It is on screen only and remembered for the file, as the other adjustments are.
- After **Reset** in Visual adjustments, **Esc** closes the panel. Before, it did nothing until you clicked elsewhere.
- The rotate buttons of the Pages panel, and **Rotate page** in a thumbnail’s menu, now turn a page on screen only, as **View > Rotate view** turns them all. The file is not changed, so there is nothing to save, and a file open for reading only can be turned too.
- Lectrix remembers which pages you turned, and the view rotation, for each file.
- Choosing **File > Exit** right after moving a tab to a new window no longer closes that window without asking about its unsaved changes.

### Annotations

- A text box added to a page that is turned on screen, by **View > Rotate view** or in the Pages panel, is typed and written upright as you see the page. Before, in a rotated view it lay on its side.
- A note or an area highlight added to such a page is placed as you see the page, too.

### Page labels

- Page labels are now set in a dialog, as in Acrobat: choose **Labels** in **Organize pages**. Pick all pages, the pages selected in the grid or a range of pages, then a style, a prefix and a first number, or continue the numbering of the page before.
- Only the pages you pick are numbered anew; the others keep their labels. The grid shows the result under each page before you apply it.
- Page labels now belong to **Organize pages** alone: the Page labels side panel is gone, and so are **Page labels** and **New label range from this page** in the Document menu and in the menu of a page in the Pages panel.

### Combining files

- Drag a rectangle over the pages to select the ones it touches. Start anywhere but on a page’s picture, which still drags the page; hold **Ctrl** or **Shift** to add to the pages already selected.
- **Insert pages from file** no longer asks for a page number: the pages go after the page you are on, into the grid of pages (see Organizing pages), where they can be dragged anywhere. Several files can be picked at once; their pages go in one file after another.
- The buttons that turn or remove the page under the pointer are now in a bar under its picture, beside its label.
- A selected page has a border around its whole block, and the pages are centered in the window.
- A selected page now has the colour of its file, the one beside the file’s name in the list. The file’s name is no longer repeated under every page: point at a page to see its file’s colour and, in a tooltip, its name.
- Drag a file up or down the list to put it at another place, or press **Alt+Shift** with the up or down arrow; its pages move with it in the grid.
- Make the pages larger or smaller with the toolbar’s buttons, **Ctrl+=** and **Ctrl+-**, **Ctrl** and the mouse wheel, or a pinch on the touchpad, up to one page filling the window.
- The pane with the files and the options can be made wider or narrower by dragging its edge. Lectrix remembers its width.

### Organizing pages

- **Document > Organize pages** shows the document’s pages as a grid in place of the viewer and the side panes. **Done** or **Esc** goes back to reading; a double-click on a page opens it.
- Drag pages to put them in another order, or press **Alt+Shift** with an arrow key. Links and bookmarks still lead to the same pages, and page labels stay with the places: a moved page takes the label of its new place.
- Rotate the selected pages with the bar’s buttons, or **R** and **Shift+R**. This is now the place to rotate pages in the file: **Document > Rotate pages…** is gone.
- Point at a page to rotate or delete it alone, with the buttons that show beside its label.
- Make the pages larger or smaller with the bar’s buttons, **Ctrl+=** and **Ctrl+-**, **Ctrl** and the mouse wheel, or a pinch on the touchpad, up to one page filling the window.
- Delete the selected pages with the bar’s button or **Del**. Lectrix asks first, deletes the links and bookmarks that led to them, and closes the page numbering up.
- After deleting pages, Lectrix says that they stay inside the file until you save with **File > Save As (optimized)**, and offers to do it.
- Insert pages from other files, one or several at once, with the bar’s button, a right click on a page, or by dragging PDFs from File Explorer into the gap where their pages should go. The inserted pages are selected, ready to be moved.
- Extract the selected pages into a new document with **Extract** or a right click. It opens in a tab of its own with its pages as a grid, and is not a file until you save it: **Save** asks where. The pages’ annotations, bookmarks and page labels come along; the document keeps its pages.
- A right click on a page, or **Shift+F10**, offers everything that can be done with the selected pages.
- Every change applies to the document at once and can be undone with **Ctrl+Z**.

### Settings

- **Appearance** has a **Visual adjustments** section: brightness, contrast, sepia, inverted colours and greyscale for the pages of every document, with **Reset**. A document you adjust by itself keeps its own adjustments; **Reset** in its Visual adjustments panel goes back to the ones in Settings.

### Installation and updates

- On Windows, Lectrix now comes as one installer, the setup program, which installs it for you alone or for everyone who uses the computer. The separate MSI package is gone.
- A copy installed from the MSI package updates as before. Its first update replaces it with the setup program’s install and keeps whether Lectrix opens PDF files.

### General

- Notifications grow into view and shrink away, the others making way for them, and have a slight shadow.
- Dialogs fade in as they open and out as they close.
- A blue or red button keeps its colour under the pointer, and its text stays readable. Before, a blue one turned pale with its text all but gone.
- Progress bars move steadily, and a sheen runs along them while the work goes on.
- In a book with thousands of bookmarks, a change such as a new highlight is applied at once, where it took a moment before. Such a book also opens a little sooner.

## 1.1.0

### Start screen

- Click Lectrix’s icon or name in the title bar to see the start screen without closing your documents. Click it again, or a tab, to go back.
- The tools are plain until the pointer is on them, and take Lectrix’s colours under it.
- Switching between the list and the grid of recent files is animated.

### Viewer and tabs

- **View > Full screen** (**F11**) fills the screen with the pages alone. The toolbar floats over them with the page and zoom controls. Move the pointer to the left or right side of the screen for that side’s pane, or hold **Alt** to see both panes and the toolbar. **F11** or **Esc** leaves.
- **Visual adjustments**, a new button beside the page display, changes how a document’s pages look: brightness, contrast, inverted colours (dark pages with light text) and greyscale. The file itself is not changed, and Lectrix remembers the adjustments for each document.
- A document can have a window of its own: drag its tab away from the others and let go, or right-click the tab and pick **Move to new window**. Drop a tab on another Lectrix window to move it there. Unsaved changes, and where you were in the document, go with it.
- A tab you drag along the others follows the pointer while they make way for it, and its right-click menu has **Move left** and **Move right**.
- The taskbar names each window after the document it shows.
- **File > Exit** closes every window, after asking about unsaved changes in each.
- Links in a document work: click one, such as a line of the table of contents, to go where it leads, and press **Alt+Left** to come back. A web link opens in your browser. The link under the pointer is tinted, and a tooltip says where it leads: the page, or the web address.
- On Linux, pinching on a touchpad zooms the document instead of the whole window, and pages appear sooner while scrolling.
- On Linux, scrolling is much smoother when the display is scaled to a fraction, such as 125% or 150%, and on displays faster than 60 Hz.
- The **Previous page** and **Next page** buttons glide to the page instead of jumping to it. With one page shown at a time, the page turned to rises into place.
- The next and the previous page are prepared while you read, so they are there when you scroll or turn to them. In scanned books, each page used to stay blank for a moment first.
- A document stays where you were when you switch to another tab and come back, instead of returning to its first page.
- Drag the start or the end of selected text to select more or less of it.
- With text selected, **Shift** and the arrow keys select more or less: a character or a line at a time, and a word or a paragraph with **Ctrl** too.
- A double-click selects the whole word and a triple-click the whole line, instead of only the part before the pointer.
- **Esc** in the zoom menu, the page display menu or a list of stroke widths or font sizes closes it only, instead of also clearing the selection or going back to the Select tool.
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
- Marking right-to-left text, such as Hebrew or Arabic, covers all of it, and Lectrix no longer says that such a mark needs repair.
- **Repair annotations** leaves text marks made in Mac apps such as PDFgear as those apps wrote them, and adds only the details they leave out. Lectrix no longer reports the order of their corners as a problem.
- An underline or squiggle from another app stays under its text when you change its colour or type.

### Bookmarks

- Bookmark titles that the program which made the file padded with invisible characters no longer end in a row of � marks; they show as in Acrobat.
- A bookmark that is a web link opens in your browser.
- Clicking a bookmark glides to its place instead of jumping there, and so does clicking a link in the text.

### Page labels

- A page in a range without labels (the style **None** and no prefix) no longer shows its page number as if that were its label: the page box is empty there and its thumbnail has no caption. The position beside the page box still says which page it is.

### Settings

- The annotation toolbar can be at the left or the right of the pages, with its tools stacked: docked there in a bar of its own (**Toolbars > Where it is docked**), or floating (**Where it floats**). What a tool’s controls open, such as the colours, appears beside the toolbar.
- On a new install, the annotation toolbar starts docked in the bar above the pages instead of floating over them. Where Lectrix is installed already, it stays as it was; **Toolbars > Placement** changes it.
- **Preset colours**, under Annotations, shows the 18 colours every tool offers and lets you replace any of them with your own. **Restore colours** puts the original ones back.
- **Open the colours when the pointer is on their button**, under Annotations, can be turned off to open them with a click.
- **Highlight the link under the pointer** and **Show where a link leads**, under General, can be turned off.
- **Show handles on selected text**, under General, puts a handle at the start and at the end of selected text, to drag it by.
- **Open PDF/A files for reading only**, under General, can be turned off to open such files ready to edit.
- The floating toolbar’s edge and **Show only when the pointer is near**, under Toolbars, can be changed while the toolbar is in the bar above the pages too: they are used in full screen, where the toolbar always floats.
- **Shortcuts** lists every keyboard shortcut, with a search field, and lets you change them: press a shortcut, then the keys it should have. Commands without one can get one, and menus and tooltips show the keys you chose.
- **Restore defaults…**, under General, puts every setting back to how Lectrix came, after asking.
- The theme is chosen with a three-way switch (System, Light, Dark) instead of a list.
- **Smooth scrolling to the next and previous page** and **Smooth scrolling to bookmarks and links**, under Appearance, turn those two animations off, each by itself.
- Sliders have a cleaner look, like the switches: no outline, a round handle that stands out from the filled part of the track, and a glide from step to step.

### Installation and updates

- Lectrix checks for updates once a day instead of at every start. The setting is now **Check for updates automatically**.
- Before you update, **See what’s new** in the update notice shows what the new version brings.
- The update notice’s buttons are on one row, and the download’s progress bar moves smoothly instead of in steps.
- On Windows, the setup program and the uninstaller have Lectrix’s icon.
- On Linux, app stores such as GNOME Software show Lectrix with its icon and a description once it is installed from the deb or the rpm, and menus list it under Office.

### General

- Tooltips have Lectrix’s own look in both themes, with a small tail pointing at what they describe. They also show for the control you reach with **Tab** or the arrow keys.
- **Help > What’s new** shows what changed in each version. The first time Lectrix starts after an update, it offers it beside its name on the start screen, or in a notification.
- On Windows, saving no longer fails when an antivirus or Windows Search looks at the file at that moment.
- Saving again after a save failed (for example because the file was open in another program) no longer damages the file. A file damaged this way opened as a blank document with no pages; Lectrix now repairs it when you open it, with everything in it, and **Save As** writes it sound again.
- After a **Save As (optimized)** that failed, the next save no longer loses changes.
- In many documents, **Undo** did nothing the first time after Lectrix had written a crash recovery copy. It undoes your last change again.
- A menu item stays highlighted while its submenu is open, and so does a toolbar button while its menu or panel is open.

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
