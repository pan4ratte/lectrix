# 0032: Text from symbolic Type 1 fonts named by their codes, read as the standard encoding

- Status: accepted (from a user's report, 2026-10-27)
- Date: 2026-10-27

## Context

A book typeset with 3B2 (Casey, *Aramaic Sources of Mark's Gospel*, Cambridge University
Press; user-supplied, not in the corpus) shows its text correctly and lets it be
selected, but search finds nothing and copying gives only U+FFFD. Its fonts are 3B2's
Advent fonts (`Advfont53`, `Advct9109`, `AdvPi1`): embedded Type 1 (CFF, `/FontFile3`),
symbolic (`/Flags 4`), with no `/ToUnicode` and an `/Encoding` whose `/Differences` is
empty, so each code is drawn with the glyph the font's own encoding gives it. Those
glyphs are named after their codes (`/C65`, `/C101`), which say nothing about the
letter.

MuPDF 1.27.2 means to handle exactly this: for a symbolic Type 1 font, a code whose glyph
name it does not know is read as Adobe's standard encoding has it
(`pdf_load_simple_font`, "symbolic Type 1 fonts with an implicit encoding and non-standard
glyph names"). The test for a name it does not know is `!fz_unicode_from_glyph_name(name)`,
but that function returns U+FFFD for such a name, never 0, so the fallback never runs.
Upstream's master has the same code.

## Decision

1. **The fork's `mupdf-sys` patches the test at build time** (`folio_patches.rs`, as ADR
   0029 does; plain patch in `docs/upstream/mupdf-1.27.2-symbolic-type1-standard-names.patch`):
   a name is unknown when `fz_unicode_from_glyph_name` returns U+FFFD. Those codes then get
   the standard encoding's names, so `C65` reads as "A", code 39 as a right quote, 174 as
   "fi" and 177 as an en dash, which is what the reported book's letters are.
2. Only the text changes. The glyphs are chosen before this step, so rendering is the same.

What stays wrong in such files, as in other readers without a ToUnicode to go by:

- Greek and Hebrew set in a "Pi" font (`AdvPi1`, `AdvP44CD89`) read as the Latin letters
  of their codes.
- Accents drawn as glyphs of their own over a letter read as spacing accents after it
  ("Go¨ttingen").

## Alternatives considered

- **Reading `Cnn` names as their number**, as pdf.js does (and macOS Preview, by pdf.js's
  note; Acrobat does not). That gives Latin-1 for codes over 127, where the reported fonts
  follow the standard encoding (174 is "fi" there, "®" in Latin-1), and straight quotes
  for 39 and 96 where the book has curly ones.
- **Nothing, as upstream does.** The text can be selected but neither searched nor
  copied.
