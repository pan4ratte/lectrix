# 0004: Send page images as PNG, not raw RGBA

- Status: accepted (Phase 1 review, 2026-10-03)
- Date: 2026-10-03

## Context

AGENTS.md section 3 says to start with PNG at the fastest compression level and to switch
to raw RGBA drawn into a `<canvas>` if encoding exceeds about 30% of render time. Phase 0
measured encoding at 61–71% of render time, so Phase 1 implemented raw RGBA, and (Phase 0
review, decision 3) measured both formats end to end, from request to pixels on screen.

Phase 1 numbers (1,000-page text PDF, 1.5× display scaling, 16 pages at 2.485 px/pt):

| Format | End to end (mean) | Server time (mean) | Bytes per page |
| --- | --- | --- | --- |
| Raw RGBA | 95–103 ms | 5–6 ms | 11.7 MB |
| PNG, fastest compression | 34–37 ms | 12–13 ms | 1.2 MB |

Rendering is not the bottleneck: WebView2's custom-protocol bridge is. Moving 11.7 MB per
page costs more than encoding and decoding a 1.2 MB PNG. Scrolling did not get better with
RGBA either: random jumps were worse (worst 246 ms against 171 ms).

Section 3's 30% rule measured encode against render time on the Rust side only, which is
not what the user sees.

## Decision

- The app requests PNG by default. Rust renders to RGBA, caches the RGBA image, and
  encodes PNG from it per request (`render::encode_rgba_png`, RGB, fastest compression),
  so tiles and whole pages both work and the cache is shared.
- Raw RGBA stays available (`fmt=rgba` in the protocol, or `FOLIO_IMAGE_FORMAT=rgba` for
  the whole app) so that `tests/perf/measure.ps1 -Format rgba` can repeat the comparison.

## Alternatives considered

- **Keep raw RGBA (section 3 as written).** About three times slower to the screen.
- **WebView2 shared buffers** (`ICoreWebView2SharedBuffer`): zero-copy transfer of raw
  pixels, probably the fastest option. Windows-only, needs new `webview2-com` code behind
  the `platform` trait, and is a project of its own. Revisit if large scanned pages prove
  slow with PNG.
- **JPEG or WebP.** Smaller, but lossy (text edges) or slower to encode.
