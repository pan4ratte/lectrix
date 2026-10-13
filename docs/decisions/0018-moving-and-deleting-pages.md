# 0018: Moving and deleting pages edit the page tree in place; extracting copies them

- Status: accepted (the feature and its behaviour asked for by the user, 2026-10-13)
- Date: 2026-10-13

## Context

The user asked for a tool to organize a document's pages: reorder them, delete them,
extract them, rotate them and insert pages from another file, with page labels edited in
the same place. Rotating and inserting were operations already. This ADR covers the two
new ones, `MovePages` and `DeletePages` (`crates/pdf-core/src/pages.rs`), and why they are
written in Lectrix and not taken from MuPDF; and extracting pages into a new file
(`merge::extract`, `Session::extract`), which changes nothing in the document. The tool
itself and its interface are in AGENTS.md section 6.

The user decided the behaviour:

- Each change applies to the document at once and is one undo step.
- Page labels belong to positions: a moved page takes the label of its new place, and
  deleting pages closes the numbering up.
- Links and bookmarks that lead to deleted pages are removed, and the user is told how
  many. The same notification says that the pages stay in the file until it is saved
  with Save As (optimized), and offers that (decided 2026-10-13; see Consequences).
- Extracting writes the selected pages, as the document is now with its unsaved changes,
  to a new file that opens in a tab; the document keeps its pages, and their bookmarks
  and labels go along.

## Findings

- **MuPDF's `pdf_rearrange_pages` writes a new catalog.** It keeps `/Type`, `/Pages`,
  `/Outlines`, `/OCProperties`, the named destinations and (optionally) the structure
  tree, and nothing else (`pdf-clean-file.c` in the pinned 1.27.2). `/PageLabels`,
  `/AcroForm`, `/Metadata`, `/ViewerPreferences`, `/PageLayout`, the open action and
  attached files are dropped. It is what `mutool clean` needs, and not an edit of an open
  document: a user who moves one page would lose the form and the page labels.
- **`pdf_delete_page` and `pdf_insert_page` work page by page.** Each walks the tree from
  the root, shifts the page labels by one, and starts a nested journal operation.
  Inserting a 2,881-page book that way took minutes (the comment on `TreeSlot` in
  `merge/mod.rs`). They also know nothing of what leads to a page.
- **MuPDF reads inherited page keys through `/Parent`.** A page that is given another
  parent takes that node's `/Rotate`, `/CropBox`, `/MediaBox` and `/Resources` where it
  has none of its own, and loses the ones it took from its old node.
- **A deleted page stays reachable.** Its object is no longer in the tree, but a tagged
  file's structure tree names it (`/Pg`), and so can an article thread. An optimized save
  drops only what nothing refers to, so the page's content would stay in the file.
- **MuPDF drops its page map by itself** when an object that is linked into the document
  changes (`pdf_dict_put` and friends in `pdf-object.c`), so writing `/Kids` directly
  needs no call to refresh it. Inserting pages has relied on that since phase 4.

## Decision

1. **Lectrix edits the page tree itself.** The tree is read into a model from the root's
   `/Kids` (not from the pages' `/Parent`, which some files have wrong), changed there, and
   written back: `/Kids` of the nodes that lost or gained pages, `/Count` of those nodes
   and the ones above them where it differs, and `/Parent` of pages that changed node. The
   catalog is not touched. A move inside one node writes that node alone.
2. **Moved pages join the node of the page they land next to**, directly in its `/Kids`.
   They are not wrapped in a new node, as inserted pages are: each move would add a level,
   and repeated moves to the same place would deepen the tree without limit.
3. **A node left without pages is taken out of the tree** (never the root), and so on
   upwards, so no `/Pages` node is left with an empty `/Kids`.
4. **A page that changes node keeps what it inherited.** Before its `/Parent` changes,
   the four inheritable keys it took from its old node are written on it. Where it took
   none and the new node defines one, it gets the value a page without one has (no
   rotation, the media box as crop box, no resources).
5. **The model must agree with MuPDF**, which decides which page has which number: the
   pages found must be the ones `pdf_lookup_page_obj` returns, in that order. A tree that
   differs, a cycle, a page listed twice, or a page or node that is not an indirect object
   is refused with `Error::DamagedPageTree`, and nothing is written. The user is told to
   save an optimized copy and try there.
6. **Deleting takes out what led to the pages:** links on other pages, bookmarks (one
   with children stays as a heading, as when combining leaves a page out), named
   destinations, and the form fields whose widgets were on them (a field with a widget on
   another page keeps that widget; the calculation order loses the removed fields). A
   damaged outline is left alone, as every edit leaves it.
7. **A deleted page's object is emptied** down to its `/Type`, so that whatever still
   refers to it does not keep its content in the file after an optimized save.
8. **Page labels:** moving leaves the rules as they are. Deleting moves each rule up by
   the number of deleted pages before it; a rule left without pages is dropped. The rules
   are written only if one of them changed.
9. **Destinations written with a page number** instead of a page reference (some
   producers write them; MuPDF follows them) get the reference in their place whenever
   pages move or are deleted, so they keep leading to their page.
10. **PDF/A:** moving pages keeps a file's claim, as rotating them does. Deleting pages
    counts as a change that may break it (section 5.4), since the structure tree is left
    describing pages that are gone.
11. **Every change reports where each page was before** (`DocumentChange::page_map`),
    worked out from the pages' object numbers, so it is right after an undo or a redo as
    well. The frontend moves what it keeps by page index with it.

### Extracting

12. **Extracting runs the engine that combines files, with the open document as its one
    source.** The engine only reads its sources, so it takes the document as it is in
    memory: an edit that is not saved yet is in the extract. It runs on the document's
    own thread, where its MuPDF document lives; that document renders nothing new
    meanwhile, as while a recovery copy is written. Combining opens its sources again from
    disk on a thread of its own; that would leave unsaved changes out.
13. **What comes along is what combining carries:** annotations, links among the
    extracted pages, named destinations, form fields, layers and attached files. Links to
    pages that stay behind are left out, and the report says how many.
14. **Bookmarks come along at the top level**, as they are in the document, without a
    bookmark for the file above them: there is one file. One whose page stays behind is
    left out, unless bookmarks inside it come along; then it stays as a heading.
15. **Each page keeps the label it had.** A document without `/PageLabels` gives none:
    its pages had no labels, so the extract numbers its own pages 1, 2, 3. (Combining
    numbers such a file's pages from their place in it, since there several files meet.)
16. **The new file is a plain full save**, not encrypted, as a combined file is. A
    document whose permissions forbid copying its content is refused, as it is as a
    source for combining. The document's own file is refused as the target.
17. **Stopping** works as for combining: at the next page, or once the file is written
    and before it takes its name, so a stopped extraction leaves nothing behind and an
    existing file of that name stays as it was.

### Speed in files with thousands of bookmarks

Measured on the local corpus's file with the most bookmarks (2,597 pages, 5,023
bookmarks, 4,999 of them by name), an edit took 0.22 to 0.26 s: 110 ms for the session to
read the whole outline again, as it did after every change of any kind, and 70 ms for a
move to go through every link, bookmark and named destination (decision 9).

18. **A document is gone through for destinations by page number once.** A move or a
    deletion that has gone through them leaves none, and nothing Lectrix writes adds one.
    The session keeps the journal position from which that holds
    (`Actor::by_reference_since`), and a move while it holds changes the page tree and
    looks at nothing else. Undoing to before that position forgets it, since the numbers
    are back; so does inserting pages, whose links and bookmarks come from another file.
    A save keeps it: the file holds what the document did.
19. **The session reads the whole outline again only after a step that can have changed
    anything in it** (`OutlineEffect` in `ops.rs`): bookmark edits, and pages deleted or
    inserted. After a move, the bookmarks it holds take their pages' new numbers from the
    page map of decision 11, which is right because the destinations in the file name
    the pages themselves. After a rotation, only the bookmarks that lead to the turned
    pages are read again (the point a destination names is kept in view space, which
    turns with the page). Annotations and page labels are no part of a bookmark. Undo and
    redo do the same for the step they cross, which the session records per journal
    position. A damaged outline, which no edit touches, is always read again.
20. **Reading an outline looks its names up in one index** of the document's named
    destinations, built on first use, in place of a search of the name tree for each
    bookmark (links on one page are still looked up one by one). On the local corpus the
    bookmarks read are the same either way (5,333 of them use names).

With these, rotating a page there takes 26 to 34 ms, moving pages 29 to 36 ms (the first
move, which goes through the destinations, 80 to 100 ms), and undoing either 27 to 32 ms:
what is left is the session reading every page's size, label and annotations again, as in
any file of that length. Deleting a page there still takes 0.19 s, 44 ms of it writing
the name tree again without the names that led to the page.

## Consequences

- An incremental save after a move appends a few small objects. After moving many pages
  between nodes it appends each of those page dictionaries once.
- **Deleting a page does not take its content out of the file on an ordinary save.** The
  default save is incremental (section 5.4): the original bytes stay, and the deleted
  page can be recovered from them. Only "Save As (optimized)" writes a file without it.
  This is true of every deletion Lectrix makes (an annotation, a bookmark); with pages it
  matters more. The user decided that the notification after a deletion says so and
  offers Save As (optimized); writing every save after a deletion in full was not chosen
  (slower saves of large files, and not possible for a signed one).
- A tagged file keeps structure elements for deleted pages, pointing at empty page
  objects. Lectrix does not edit the structure tree (it does not when combining either).
- Article threads (`/B`) and an open action that leads to a deleted page are left as they
  are.
- A signed document gets the usual warning before the first edit and is saved
  incrementally; readers then show the pages as changed after signing.
- An extract has what a combined file has and lacks what it lacks (section 6.4): no
  structure tree, article threads, document JavaScript, open action, viewer preferences
  or XMP metadata, so no PDF/A claim either; signatures copied with their pages are not
  valid in it. Every attached file of the document comes along, whatever pages are
  extracted.

## Alternatives considered

- **`pdf_rearrange_pages`.** Rejected: it drops catalog entries (above). Saving the
  entries and putting them back afterwards would depend on a list of every catalog key
  MuPDF might drop, now and in later versions.
- **`pdf_delete_page` and `pdf_insert_page` for each page.** Correct for the tree, but
  slow for many pages, a nested undo step per page, and labels shifted by MuPDF's rule
  (the page after an inserted one continues the inserted one's numbering) and not the
  user's.
- **Rebuild the tree flat** (every page a kid of the root). Simple, and it would mend odd
  trees, but it rewrites every page object and every node on the first move: a large
  incremental update for moving one page, and it changes objects the user did not touch
  (section 5.1, rule 10, in spirit).
- **Leave the deleted page's object as it is.** Its content would survive an optimized
  save of a tagged file, which a user who deleted the page would not expect.
- **Keep bookmarks and links to deleted pages, leading nowhere,** as Acrobat does. Offered
  to the user, who chose removal.

## Tests

`crates/pdf-core/tests/pages.rs`, on a six-page file with two nodes that turn and crop
their pages, labels, links (explicit, by name, by page number), bookmarks, a form and a
structure tree: moved pages keep their size and rotation, links and bookmarks follow them,
labels stay with the positions, a move inside a node writes only that node, a move that
changes nothing writes nothing, an emptied node leaves the tree; deleting removes links,
bookmarks, names and fields and reports their counts, closes the numbering up, and an
optimized save holds none of the deleted pages' content (the test fails without decision
7); undo and redo restore each state; `qpdf --check` passes on every file written. Unit
tests in `pages.rs` and `ops.rs` cover the label rules and validation; `tests/pdfa.rs`
covers decision 10. Extracting, in the same file: pages come in the order asked for, with
an edit that was not saved, their size and rotation, their labels, the links among them
and the bookmarks that lead to them; the document, its undo state and its file are as
they were; a stopped extraction leaves no file and keeps an existing one; a document
without labels gives none. `src-tauri/src/combine.rs` tests the progress the app reports
and stopping. The interop suite `pages` (`tests/interop/run.py`) has PDFium and pdf.js
read the pages, labels, bookmarks and links of a moved file, a file with pages deleted and
an extract, and checks that a highlight on a moved page shows in all three engines.

For decisions 18 and 19, in the same file: after each of a rotation, two moves, a change
of labels, the undo and the redo of them all, and a deletion, the bookmarks the session
holds are the ones a fresh reading of the document gives (a recovery copy of it, opened),
and a change reports them exactly when they differ; a link written with a page number
leads to its page through a move that rewrites it, a second move that looks for nothing,
both undone, and a third move that has to find it again (the test fails if undoing does
not forget), and across a save.

On the local corpus (`local_corpus_pages_move_and_are_deleted_as_they_are`, 30 real
files, run on demand): the last page moved to the front, three pages extracted from the
document with that move unsaved, the first half moved to the end, a bookmarked page
turned and turned back, undo and redo, a save,
a page deleted and a second save. Every page keeps its size,
rotation and number of annotations wherever it is, the bookmarks the session holds after
the moves and after the rotation are the ones a fresh reading gives, no file is refused,
and `qpdf --check` finds nothing it did not find in the original.
