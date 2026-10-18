# 6.3 Page labels

Part of the Lectrix specification. [AGENTS.md](../../AGENTS.md) holds the rules every change follows and [README.md](README.md) lists these files; sections keep the numbers they share, so "section 5.4" is in AGENTS.md and "section 6.5" in [annotations.md](annotations.md).

- Set in a dialog over the grid of the page organizing tool (section 6.7), as Acrobat's Page Numbering is, and offered only there: the tool's bar has a **Labels** button for it, and the grid's menu has **Page labels…**. While a document is read, no menu has page labels, neither the Document menu nor a page's in the Pages panel. The command ("Page labels, while organizing pages" in Settings > Shortcuts, without a default shortcut) opens the dialog in the grid and does nothing while reading.
- **Pages:** all of them ("All 212 pages"), the pages selected in the grid ("Selected: 3 pages (5–7)"), or from one page to another, by their places in the document. With pages selected the dialog starts with those; with none, Selected is disabled and it starts with the range, set to the page with the keyboard focus in the grid (the page the document was at, until another is picked).
- **Numbering:** **Start a new section**, with a style (1 2 3, i ii iii, I II III, a b c, A B C, none), an optional prefix and a first number (1 or higher), or **Continue the numbering of the page before**, which is how a range is taken away again.
- **Apply** numbers the picked pages, in one undo step; every other page keeps the label it has. Cancel, Esc or closing the dialog changes nothing.
- **Live preview:** while the dialog is open, the labels under the grid's pages show what Apply would give, and a line of the dialog says it in words ("Preview: i, ii, iii … xii").
- Letter styles follow the PDF convention: a…z, then aa…zz, then aaa…zzz.
- Opening a file shows its existing labels exactly as stored; saving writes the `/PageLabels` number tree in the document catalog, with a rule at the first page, which the format requires.

## Settled details

- A new section starts as "1, 2, 3 from 1" each time the dialog opens, like Acrobat's "Begin new section", whatever the pages have now.
- Pages that are not next to each other are numbered through, in page order (pages 2, 3 and 7 become i, ii, iii), and the pages between them keep their labels.
- The pages after the picked ones keep their labels: a rule at the first of them says what it had (`assignLabels` in `labels/rules.ts`). Rules that start inside the picked pages go. A rule this would add that only repeats the rule before it is not written.
- Continue the numbering is disabled while the first page of the document is among the pages: it has no page before it.
- A document numbered 1, 2, 3 throughout by a change is left without `/PageLabels`: all pages as "1, 2, 3 from 1" removes all labels.
- Cancel leaves the grid as it was. After Apply to all pages or to a range, the first numbered page comes into view. Versions up to 1.1 had **Document > Page labels** and **New label range from this page**, in the Document menu and in the Pages panel's; a shortcut stored for the second is ignored.
- Versions up to 1.1 had the labels in a side panel of their own. A pane layout remembered from then loads without that panel.
- An invalid value says why under the fields ("Enter pages between 1 and 8, the first no later than the last.", "Numbers start at 1 or higher."), and Apply is disabled meanwhile.
- Stored rules that start after the last page have no effect, and the next change removes them. A file without a rule at page 1 is numbered 1, 2, 3 from there.
- A page whose label is empty (a range with the style "None" and no prefix) has no label, and is not given its page number in its place: the page box is empty there, with the physical position after it ("(4 of 312)"), and the thumbnail has no caption. The page number typed in the page box still goes there. Where a sentence names the page ("Page 4": a link's tooltip, the annotation list's headings, a new bookmark's title, accessible names), it is named by its number. A document without `/PageLabels` shows page numbers throughout, as before.
- A document without labels gets no `/PageLabels` tree until something is changed.
- Changing labels needs the same permission as rotating pages; in a signed document, the first change shows the signed-document warning. Where labels can't be changed (permissions, or a file open for reading only), the bar's button and the grid's menu item are disabled.
