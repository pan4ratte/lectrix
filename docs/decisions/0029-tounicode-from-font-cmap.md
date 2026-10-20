# 0029: Text from fonts without a ToUnicode map, read off the font's own cmap

- Status: accepted (from a user's report, 2026-10-20)
- Date: 2026-10-20

## Context

A one-page certificate from Georgia's Revenue Service (`vat.pdf`, user-supplied, not in
the corpus) shows its Georgian text correctly, but selecting it copies only U+FFFD,
double-clicking a word selects a single letter, and search finds nothing. Its fonts are a
subset of Sylfaen embedded as TrueType CIDFonts (`/Encoding /Identity-H`,
`/CIDToGIDMap /Identity`) with no `/ToUnicode`. Older tools write Georgian, Armenian and
Indic text this way: those scripts have no Adobe character collection a reader could fall
back on.

MuPDF 1.27.2 then knows no Unicode for any character: `pdf_load_to_unicode` reads a
ToUnicode CMap or a collection's map and nothing else (upstream's master has a TODO there:
"synthesize a ToUnicode if it's a freetype font with cmap and/or post tables"). PDFium
(Edge, Chrome), PyMuPDF 1.28 and Acrobat read the text off the embedded font's own
Unicode cmap, which maps each code point to the glyph drawn for it; inverted, it gives
each glyph its text.

## Decision

1. **The fork's `mupdf-sys` patches `load_cid_font` at build time** (`folio_patches.rs`,
   as ADR 0008 and 0024 do; plain patch in
   `docs/upstream/mupdf-1.27.2-tounicode-from-cmap.patch`). When an embedded CIDFont
   still has no `to_unicode` after MuPDF's own loading, it gets one built from the
   font's Unicode cmap (FreeType's `FT_Select_Charmap(FT_ENCODING_UNICODE)`, so a
   (3,1), (3,10) or (0,x) subtable): each CID gets the code point that the cmap maps to
   the glyph the CID is drawn with (`pdf_font_cid_to_gid`, the path rendering takes, so a
   `/CIDToGIDMap` stream is followed too). Where several code points map to one glyph,
   the lowest wins, and a private use one only if there is no other. Control codes are
   left out. The face's charmap is selected again afterwards.
   - Fonts that are not embedded (substitutes) are left as MuPDF has them: their glyphs
     come from another font, so its cmap says nothing about the file's.
   - A font with a ToUnicode, even a partial one, is left alone.
   - Rendering does not change (a page of the reported file renders to the same bytes).
2. **Characters that still have no Unicode and no width are left out of a page's text**
   (`shown` in `text.rs`, for the text sent to the frontend and the quads of a selected
   range alike). The reported file has one: a zero-width glyph its font maps from U+0002
   alone, which came out as U+FFFD inside a word. PDFium leaves such glyphs out too.
   Characters with a width keep their U+FFFD, so their boxes can still be selected.

## Alternatives considered

- **Mapping in Rust after extraction**, with MuPDF's `FZ_STEXT_USE_GID_FOR_UNKNOWN_UNICODE`
  and the font's cmap inverted in `pdf-core`. MuPDF's text device would then take glyph
  ids for code points while it builds lines: glyph 32 would be a space and glyph 0x2000
  an en space, both normalized to U+0020 before Rust could map them back. MuPDF's own
  search would still see U+FFFD.
- **Fixing the fonts from Rust before each display list is built** (load each font of a
  page's resources and give its `pdf_font_desc` a map). This means walking every page's
  resources, its form XObjects, patterns and annotation appearances, and doing it again
  whenever the resource store evicts a font. The patch does the same in one place, as
  each font is loaded.
- **Nothing, as upstream does.** The text cannot be copied, searched or selected by
  word, while every other reader handles it.
