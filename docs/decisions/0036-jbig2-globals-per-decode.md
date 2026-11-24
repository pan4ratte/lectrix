# 0036: JBIG2 images decoded against globals of their own, not a shared copy

- Status: accepted (from a user's report, 2026-11-24)
- Date: 2026-11-24

## Context

A 1,698-page scan (*The Oxford Classical Dictionary*, made by PDFBeads; user-supplied,
not in the corpus) crashed Lectrix 1.4.0 when a search went to its first result, and
then again on reopening it, mostly right away, sometimes only at the next search. The
eight crash dumps Windows kept all stop on heap corruption (`0xC0000374`) on a
`tokio-rt-worker` thread, where the `lectrix://page` handler renders pages. The same
corruption happens headless whenever two threads render the file's pages at once, with no
search at all; one thread never fails.

Every page of the file is a JBIG2 image, and the pages share four `/JBIG2Globals`
streams, about 425 pages each (172 KB of symbol dictionary apiece), as JBIG2 scans
usually do. MuPDF 1.27.2 decodes such a stream once into a jbig2dec global context, keeps
it in the store, and decodes every image that names it against that one context, from
whichever thread draws it (`fz_open_jbig2d`, `filter-jbig2.c`). Decoding a text region
takes a reference on each glyph it places from the dictionary and drops it afterwards
(`jbig2_text.c`), and jbig2dec counts references with a plain `refcount++` and
`refcount--` (`jbig2_image.c`). Two threads doing that on the same glyphs lose counts;
a glyph reaches zero while the dictionary still holds it, is freed, and is freed again.
Upstream's master is the same. Other readers serialize JBIG2 decoding or decode globals
per image.

## Decision

1. **The fork's `mupdf-sys` patches `fz_open_jbig2d` at build time** (`folio_patches.rs`,
   as ADR 0029 and 0032 do; plain patch in
   `docs/upstream/mupdf-1.27.2-jbig2-globals-per-decode.patch`): each decode reads the
   globals into a jbig2dec context of its own, from the bytes the shared one keeps
   (`globals->data`), and frees it when the decode is done. No jbig2dec object is then
   used by two threads, and pages still render in parallel.
2. The shared globals stay in the store as before, as the holder of those bytes; nothing
   decodes against them any more.
3. A local-corpus test draws each file's first pages on four threads at once
   (`local_corpus_pages_render_on_several_threads_at_once`), and the survey and selection
   pick a file whose pages share JBIG2 globals (`jbig2_globals_pages`,
   `jbig2-shared-globals`). The corpus is still empty and no generated sample has JBIG2
   (`testgen.rs` has no encoder for it), so CI cannot test it.

## Results

Measured on the reported file, release build, on battery:

- Before: rendering pages on two to four threads at once, with or without a search
  running, corrupted the heap within seconds, every run.
- After: no failure in 20 s of rendering on 2, 4 and 8 threads, nor in three searches of
  the whole book with four threads rendering meanwhile.
- `pdf-cli bench --samples 200` (one thread): mean render 20.5 ms before, 18.4 and 17.9 ms
  after. Decoding the globals again for each image costs nothing measurable here, so the
  pool below was not needed.

## Alternatives considered

- **A lock around JBIG2 decoding.** As small a patch, but every page of a scanned book
  would then decode one at a time, where the viewer draws several at once.
- **Atomic reference counts in jbig2dec.** Fixes the counts this file breaks, but it
  would need every other write to the shared context audited (symbol dictionaries that
  refer to others, retained coding contexts), and a mistake there corrupts the heap as
  silently as this did.
- **A pool of private copies per globals stream**, each decoded once and lent to one
  decode at a time. Cheaper per page than decoding the globals every time, at the cost of
  more code in MuPDF's filter and a copy kept per thread; worth it only if the decoding
  shows up in the measurements above.
