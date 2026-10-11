# 0016: Several windows that exchange tabs

- Status: accepted (built at the user's request, 2026-10-11; the choices below were made
  while building and are the user's to change)
- Date: 2026-10-11

## Context

Lectrix had one window. The user asked for document tabs that can be separated into
windows of their own.

Documents already live in Rust (AGENTS.md section 7): a tab is only a view of a session,
which holds the unsaved changes and the undo history. What a window adds is a webview with
its own copy of the frontend's state.

## Decision

1. **One process, several webview windows.** Every window runs the whole frontend and
   shares the process, its documents, its caches and its app data. Lectrix stays
   single-instance. The alternative, a process per window, would have meant handing a
   document with unsaved changes from one process to another, which MuPDF's journal
   cannot do (ADR 0001), and a second copy of every cache.
2. **Rust keeps which window shows which document** (`src-tauri/src/windows.rs`). Moving a
   tab changes that entry and nothing else: the session is untouched, so unsaved changes
   and undo history go with it, and nothing has to ask about saving. The window that gets
   the tab asks Rust for its documents (`list_open_documents`), at startup or when told
   tabs were moved in. What a tab keeps outside the document goes along: the remembered
   view (page, zoom, rotation) through the store, and its page display in a small typed
   handoff.
3. **Events go to the window they concern** (`emit_to`), and the frontend listens through
   its own window. A plain `listen` hears every window's events (its target is "any"),
   so files dropped on one window would open in all of them.
4. **No window is the main one.** A window asks about its own unsaved documents when it
   closes, and Rust closes them when it is destroyed. Lectrix quits with the last window.
   File > Exit and restarting into an update ask the other windows to close first, each
   with its own question; one that is cancelled stops the quit. The label `main` only
   says which window Lectrix started with: that one offers crash recovery and what an
   update brought.
5. **A tab is dropped where the system's cursor is.** The page cannot see outside its
   window, so on release Rust asks the system which window is under the cursor (on
   Windows `WindowFromPoint`, behind the `Platform` trait; elsewhere by the windows'
   frames, without knowing which is on top). One of Lectrix's: the tab moves there. None:
   a new window is made there. As in browsers; unlike them, nothing is drawn under the
   pointer outside the window, and the new window appears on release, not during the
   drag.
6. **A window made for a tab is not closed until its page has the tab** (at most 15
   seconds): before that it could not ask about the tab's unsaved changes, and closing it
   would lose them.
7. **Each window's title is what it shows** ("report.pdf - Lectrix"), so the taskbar and
   Alt+Tab tell the windows apart. It was "Lectrix" for the one window.

## Consequences

- WebView2 uses more memory with each window (ADR 0002's rule was measured with one).
- App data has one window place, one pane layout and one thumbnail size: the window used
  last decides them, and the next start opens one window.
- The gaps are listed in `docs/status.md` ("Windows").

## Alternatives considered

- **Making the new window when the tab leaves its row and moving it with the pointer**, as
  Chrome does. A webview window takes a few hundred milliseconds to appear, and following
  the pointer from another window's events lags; left for later.
- **A "New window" command** for an empty window. Not asked for; a window comes from a tab.
