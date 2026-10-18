# 0023: A document can have several views; a tab can hold several documents

- Status: accepted (the user's request, 2026-10-18)
- Date: 2026-10-18

## Context

The user asked for documents side by side in one window, for the same document side by
side, and for a lock that scrolls them together. Before this, one `DocTab` in the frontend
was both what Rust reported about a document (pages, annotations, outline, labels,
revision, the text cache) and how the user was viewing it (zoom, place, rotation, page
display, selection, search, history), and one tab of the row was one `DocTab`. Showing a
document twice needs two views of one document; showing documents side by side needs a tab
that holds several views.

The user chose, among the alternatives offered: joined tabs in the title bar's row (as Edge
and Chrome do), up to four columns, one view bar and one set of side panels that follow the
pane in focus, a slim header on each pane, and synchronized scrolling that keeps the panes'
distance (zoom and rotation too, between views of one document).

## Decision

1. **`DocModel` and `DocTab`** (`stores/doc.svelte.ts`). `DocModel` holds what Rust
   reported about a document, once per window; `DocTab` is one view of it. `DocTab` keeps
   the old surface (`tab.pages`, `tab.annotations`, `tab.state`, `tab.applyChange()`…)
   through accessors to its `doc`, so the components and actions written for one view
   needed no change. A change applied through any view reaches every view: the model
   updates what it holds, then each view what it keeps by page (current page, pages turned,
   history, selected annotation, its place when pages move) and what a new revision ends
   (selection, text cursor, search hits). Each view has a `key`; what was keyed by the
   document id and belongs to a view (the selected annotation's panel, `{#key}` blocks, the
   blank-page tracker) is keyed by it. Page images stay keyed by document, so two views of
   one document share them.
2. **`TabGroup`** (`stores/group.svelte.ts`): a tab of the row, one to four views, their
   widths, the one in focus, and the lock. The app store's row is `groups`; `tabs` is now
   every view, derived from them, and `activeId` the document of the pane in focus. The
   commands, the menus and the side panels act on `app.active`, the view in focus, as they
   acted on the tab shown.
3. **Rust is unchanged.** A document has one session however many views show it; closing a
   view closes the document only when it was the last. Moving a tab to another window moves
   documents, and the views there are made from the handoff as before, so a joined tab
   arrives as tabs of their own (docs/status.md, Split view).
4. **Synchronized scrolling** counts pages: a pane's place is its page and how far down it
   the view is. The pane scrolled by other means than following leads; the others go as
   far from where they were as it went (`features/split/sync.ts`). A pane's own following
   is told from the scrolls that lead by the place it set (`Viewer.svelte`, `followPlace`).

## Alternatives considered

- **Two `DocTab`s with the same document id, kept in step by the store.** No model to
  share, but every update would have to be applied twice and every lookup by id would find
  one of them at random; drift between the two would be a bug class of its own.
- **Moving the view state out of `DocTab` into a new class, leaving `DocTab` the document.**
  Cleaner names, but `tab.zoom`, `tab.selection` and the rest are read in some two hundred
  places; the accessors keep that code as it was.
- **Panes as a layer above tabs (VS Code's editor groups),** each with its own row of tabs.
  More flexible, but it takes the tabs out of the title bar, which the user preferred to
  keep.
- **Syncing by points or by fraction of the document.** Points mislead between documents of
  different page sizes; a fraction moves a short document a page for a long one's ten. The
  user chose keeping the distance, which Acrobat and Word do.

## Consequences

The frontend has two notions where it had one, and code that changes a document goes
through a view as before. A component that must act on every view of a document (reload,
which keeps each view's place) iterates `tab.doc.views`. Joined tabs are not handed over
between windows; that would need the handoff to carry the group.
