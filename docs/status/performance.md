# Performance

Part of the status: [docs/status.md](../status.md) is the index a session reads, with one line for each item; this file has the measurements in full. Keep both current.

Last measured 2026-10-13, with the page organizing tool in: release build, PNG page
images, a mid-range laptop on battery (Balanced power plan) with its own display,
2560 x 1600 at 120 Hz and 1.5x scaling (page images at 2.28 px per point).

| Target (AGENTS.md section 2) | Measured | Met? |
| --- | --- | --- |
| Window visible < 1 s | 38 to 106 ms; 146 to 224 ms for the first start of a series | Yes |
| First page visible < 1 s | 1,000-page file 96 to 139 ms; 2,881-page dictionary 133 to 136 ms; the 2,597-page book with 5,023 bookmarks 360 to 364 ms; a 33-megapixel JPEG 2000 scan 627 to 639 ms | Yes |
| No blank page > 200 ms while scrolling | 1,000-page file: none at steady speed, worst 22 to 26 ms fast, 104 to 125 ms jumping | Yes, except JPEG 2000 scans (below) |
| `lectrix.exe` < 200 MB idle (ADR 0002) | 58 to 59 MB with the 1,000-page file, 63 to 64 MB with the dictionary, 98 to 99 MB with the 5,023 bookmarks, 115 MB with the JPEG 2000 scan | Yes |
| WebView2 growth rule (ADR 0002) | Whole process tree 525 to 545 MB idle with the 1,000-page file, 530 to 533 MB with a one-page document (the document's size adds nothing), 418 to 446 MB for an empty window. The three rounds of scrolling were not run again | Yes, as far as checked |

Version 1.1.0, installed and measured the same way on the same day, gives the same
numbers (first page of the 1,000-page file 101 to 153 ms, worst blank after a jump
107 ms, the JPEG 2000 scan 606 to 630 ms, the dictionary 136 to 140 ms), so the
organizing tool changed none of them. One differs for the better: the book with 5,023
bookmarks showed its first page after 413 to 417 ms, since its bookmarks' names were
looked up one by one (ADR 0018).

Three numbers are higher than in the table of 2026-10-04. Looked into on 2026-10-13 with
the release build of that evening (`folio.exe`, still in `target/release`), measured
again with that day's script:

- **The 1,000-page file's first page (73 to 80 ms then) and a jump's worst blank (46 to
  54 ms then) moved with the conditions of the run, not with the code.** The old build
  gives today's numbers today: 93 to 134 ms and 103 ms. Today's run was on battery, on
  the Balanced power plan, on the laptop's own display (2560 x 1600 at 120 Hz); what
  the run of 2026-10-04 had was not recorded, beyond 1.5x display scaling. Which
  condition makes the difference was not found out. `measure.ps1` prints the display
  and the power state from now on.
- **The JPEG 2000 scan's first page (376 to 382 ms then, 372 to 406 ms in the old build
  today) takes 606 to 639 ms in 1.1.0 and now, and that is the page being shown once,
  fitted, where it was shown twice.** Until 2026-10-05 (d93e588) the first page was
  requested at 100% before the viewer knew its size, shown, then fitted and rendered
  again, a visible jump on every open; the number was the time to the 100% page. At
  1.5x display scaling 100% is 2.0 px per point, and fit width in the window is about
  2.3 (2.28 for the 1,000-page file's pages).
  This scan's image decodes at a quarter of its resolution up to 2.05 px per point
  and at half from 2.1 on (ADR 0008: the resolution drawn, in halvings), which takes
  230 ms and 455 ms (`pdf-cli bench --scale`). So the page that used to show first was
  the cheaper one, and the fitted page came after another render of about 0.46 s; now
  the fitted page is the first and only one. Nothing was lost, and by these numbers the
  fitted page is there sooner than it was (that was worked out, not timed in the old
  build), but the number is the higher one from now on. It could be
  brought down only by showing the coarser decode first and the finer one after it,
  which is a change to rendering that has not been asked for.

JPEG decoded by libjpeg-turbo (ADR 0024), measured 2026-10-19 on mains against the build
before it, the same evening: `15-largest-file.pdf` (328 pages scanned as JPEG, 5.9
megapixels each) shows its first page after 202 to 231 ms instead of 240 to 262, a page
renders in 35 ms instead of 60 on average, and a page jumped to is blank for 115 ms at
the median and 154 at worst, instead of 151 and 188; `lectrix.exe` is 0.4 MB larger.

Scripts: `tests/perf/measure.ps1` and `tests/perf/memory-over-time.ps1`; for the grid of
pages, `tests/perf/organize.mjs` (its numbers are under "Organizing pages" in [known-gaps.md](known-gaps.md)).
