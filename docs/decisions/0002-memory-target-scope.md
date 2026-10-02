# 0002: Scope of the idle-memory target

- Status: accepted (after Phase 0 review)
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
  - Opening a 1,000-page document must not grow the WebView2 processes by more than 100 MB
    over an empty window. The frontend must not hold rendered pages it does not show.
  - In Phase 6, use WebView2's memory-reduction settings (for example lowering the memory
    target level while the window is minimized) and report the result.

## Alternatives considered

- **Whole tree under 200 MB:** not achievable with WebView2. An empty WebView2 window is
  already above it, so the target could never pass and would stop guiding decisions.
- **No target for WebView2:** this would let frontend leaks (images kept after scrolling
  away) go unnoticed, so the growth rule stays.
