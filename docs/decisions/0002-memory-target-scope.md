# 0002: Scope of the idle-memory target

- Status: accepted (after Phase 0 review); growth rule revised after Phase 2 review
- Date: 2026-10-03

## Context

AGENTS.md section 2 sets "idle memory under 200 MB with one large document open", but does
not say which processes count. In Phase 0, Folio's own process (`folio.exe`: MuPDF, the
document, render caches) used about 60 MB. The WebView2 runtime adds its own browser, GPU,
renderer and utility processes, about 440 MB in total. Most of that is the cost of
embedding a Chromium webview at all, and it barely changes with the document.

## Decision

- **The 200 MB target applies to `folio.exe`'s working set.** That process holds
  everything that grows with documents (MuPDF objects, display lists, the image cache), so
  it is the number our design choices control.
- **The whole process tree (including WebView2) is measured and reported every phase** by
  `tests/perf/measure.ps1`, with two rules:
  - ~~Opening a 1,000-page document must not grow the WebView2 processes by more than
    100 MB over an empty window.~~ Replaced after the Phase 2 review (2026-10-03), because
    most of that growth is the cost of showing any page at all:
    - **Document size:** with the 1,000-page sample open and idle, the WebView2 processes
      may use at most 30 MB more than with a one-page document open.
    - **No unbounded growth:** after three rounds of the scroll tests, the whole tree may
      use at most 10% more than after one round.
    The frontend must not hold rendered pages it does not show.
  - In Phase 6, use WebView2's memory-reduction settings (for example lowering the memory
    target level while the window is minimized) and report the result. (Minimized: done
    in Phase 2.)

## Alternatives considered

- **Whole tree under 200 MB:** not achievable with WebView2. An empty WebView2 window is
  already above it, so the target could never pass and would stop guiding decisions.
- **No target for WebView2:** this would let frontend leaks (images kept after scrolling
  away) go unnoticed, so the growth rule stays.

## Phase 2 findings (2026-10-03)

Measured on the generated 1,000-page file, Windows 11, 1.5x display scaling
(`tests/perf/measure.ps1 -Empty`, `-Pdf`, and `tests/perf/memory-over-time.ps1`):

- **Where the growth is.** Mostly the WebView2 GPU process. An accelerated 2D canvas keeps
  each page's pixels there as well as in the renderer.
- **What reduced it.** CPU-backed page canvases (`willReadFrequently`) and mounting a
  quarter screen of pages around the viewport instead of a full screen: growth on opening
  went from about +230 MB to about +120 MB. A one-page document costs the same +120 MB,
  so what remains is the price of showing a page at all, not of the document's size.
- **What did not.** `<img>` elements instead of canvases (similar totals), Chromium's GPU
  memory flags (`--force-gpu-mem-available-mb`, `--force-gpu-mem-discardable-limit-mb`),
  and `--disable-gpu` (the same memory moves into the renderer).
- **Scrolling.** Memory after scrolling plateaus at about 1.0 GB for the whole tree,
  the same after one or three rounds of the scroll tests: a bounded pool, not a leak.
  The Phase 1 figure of 1.6 to 2.0 GB included about 800 MB from the image-format
  comparison that ran first; `FOLIO_PERF=scroll` now leaves it out.
- **Minimized.** Folio sets WebView2's memory target level to Low while the window is
  minimized (planned for Phase 6, done now): the renderer drops from about 126 MB to
  8 MB; the GPU process keeps most of its pool.

The original growth rule (+100 MB) was not met (+120 MB). The user accepted the revised
rules above on 2026-10-03; both hold (one-page and 1,000-page documents grow WebView2 by
about the same 120 MB; one and three rounds of scrolling end at 1,052 to 1,105 and
1,059 MB).
