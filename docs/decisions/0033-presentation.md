# 0033: Presentation: the document's transitions and timings, and an audience screen

- Status: accepted
- Date: 2026-10-30

## Context

View > Presentation (section 6.1) shows a document one page at a time on the whole screen.
A PDF can say how each page comes on screen and for how long it shows: `/Trans`, a
transition dictionary (ISO 32000-2 section 12.4.4.1: Split, Blinds, Box, Wipe, Dissolve,
Glitter, Fly, Push, Cover, Uncover, Fade, and Replace, with a duration, a direction and
more), and `/Dur`, the page's display duration, after which the next page comes by itself.
Acrobat plays both. Decks exported from PowerPoint and Beamer often carry them.

Three rules stand in the way:

1. AGENTS.md section 8: animations stay under 150 ms. A transition lasts what the
   document says, 1 s by default.
2. MuPDF reads `/Trans` with `pdf_page_presentation`, but it loads each page to do it, and
   drops what it doesn't implement: a Fly's scale (`/SS`) and opacity (`/B`), and a
   direction of `/None`. It draws transitions into pixmaps (`fz_generate_transition`), a
   whole screen of pixels per frame, on the CPU, in Rust, across IPC.
3. Every Lectrix window is the whole app (section 3). A presenter view needs the pages on
   another screen, in a window that is not.

## Decision

1. **The document's own transitions play at the document's durations** (at most 10 s),
   as content of the document, not as motion of the app's own: the 150 ms rule stays for
   the app. They don't play with reduced motion, nor when Settings > Viewing >
   Presentation > Play page transitions is off; a key or click during one ends it, and the
   next page comes at once.
2. **`pdf-core` reads `/Trans` and `/Dur` from the page dictionaries itself**
   (`presentation.rs`), without loading pages, every entry included; a value of the wrong
   type is its default, an unknown style is none. One command gives the frontend every
   page's at the start of a presentation.
3. **Transitions are drawn by the compositor** (`transitions.ts`): opacity, transforms,
   clip paths and gradient masks on the two pages' layers, one frame per display frame,
   no pixels read or written. Wipe, Split, Box, Blinds, Push, Cover, Uncover and Fly are
   as the standard describes. Two are approximated: **Dissolve** fades (the standard's
   scattered pixels would be a mask redrawn every frame), and **Glitter** is a wipe with a
   soft edge 15% wide. A Fly whose `/B` is false (only what is on the page moves) fades
   as it flies. Movement is in screen space, not the page's own, so a page turned on
   screen moves as it is seen.
4. **A page shows once its pixels are drawn**: the page before stays until then (or 0.8 s),
   so a transition never plays onto a blank page.
5. **`/Dur` turns to the next page** that long after the page came, its transition
   included; not while the screen is blank, and not from the last page. Settings can turn
   it off.
6. **An audience screen is a window of its own** (`audience.rs`), `audience-<label>`, full
   screen on the first screen that is not the presenting window's: a webview of the same
   app that renders only the pages (`Audience.svelte`). It owns no documents, has a
   capability of its own that allows events and logging only, and is left out of
   everything Lectrix's windows share: focus, placement, drops, Move to window, quitting,
   Settings. It goes with its window, and ending it (Alt+F4 there) ends the presentation.
   The window presenting sends it what to show (`StageState`: the document, the pages
   kept, the visual adjustments, the change and its transition) by events, and it sends
   back the keys, clicks and swipes it gets. Page images come from the same `lectrix://`
   protocol, which doesn't ask which window wants them.

## Alternatives

- **Cap transitions at 150 ms.** A deck's 1 s fade would flash by; the document asked for
  something else.
- **`pdf_page_presentation` and `fz_generate_transition`.** Loses settings, loads pages,
  and sends a screenful of pixels per frame over IPC: a 4K frame is 33 MB.
- **Dissolve by a random mask.** A canvas mask redrawn each frame at the screen's size
  works but costs a full-screen draw per frame on the CPU-backed canvases the pages use
  (ADR 0002); a fade reads much the same at a deck's speed.
- **The presenter view as a second full app window.** It would own documents and need
  to be told apart from a real one everywhere; a window that shows pages only is simpler
  and safer.
- **Mirroring with one window spanning both screens.** Screens differ in size and scale;
  each would get a stretched or cut page.
