# 0031: Compare is a word-level diff of the documents' text, in a view of its own, as FineReader's

- Status: accepted (the plan and its four choices decided by the user, 2026-10-27)
- Date: 2026-10-27

## Context

Compare is the third of the tools planned after v1 (`docs/status.md`, "Planned tools";
section 6.13). The user chose ABBYY FineReader's Compare Documents as the model for what it
does and how it looks: two documents side by side, a right-hand pane with a Compare tab
(options) and a Differences tab (grouped into body text, headers and footers, numbering),
differences that can be ignored and copied, synchronized scrolling, and results saved as a
PDF with each difference "highlighted and commented" or as a report. The plan offered four
choices, and the user decided:

1. Start as FineReader does: an empty view with two slots and the options in the panel,
   not a dialog that asks for both files first.
2. Mark the annotated copy as FineReader does (rather than with Acrobat's Caret
   annotations, which the interop profile does not allow).
3. Save the report as a PDF, not as a Word table (DOCX conversion was dropped, and writing
   DOCX would need a new dependency).
4. Keep the compared documents read only, as FineReader does, rather than letting them be
   edited and the comparison marked as out of date.

MuPDF has no diff of its own. The pinned 1.27.2 has what the rest needs: structured text
with each character's box (`text.rs`), display lists rendered to grey pixmaps, and the
story engine that lays HTML out on pages (`fz_new_story`, `fz_place_story`), which the
report uses through a shim function (`lectrix_write_html_pdf`).

## Decision

1. **The engine** is `pdf-core/src/compare/`, no new dependency:
   - `words.rs`: each page's text without its annotations as words (UAX #29 word
     boundaries through `unicode-segmentation`, already a dependency), with quotes, dashes
     and ellipses normalized, hyphenated line breaks joined, and headers, footers and list
     numbers marked.
   - `diff.rs`: the two word sequences lined up over the whole documents by a patience diff
     (words unique in both, then words repeated as often in both, up to 64 times, as
     anchors) with Myers' algorithm between anchors, within a budget of length times edit
     distance (40 million) past which a stretch counts as changed whole. Headers and
     footers are a second sequence of their own.
   - `mod.rs`: the stretches between matches as insertions, deletions and changes, merged
     where a few equal words split one rewritten passage; filtered by the options;
     grouped; with each side's rectangles, characters and, for the side lacking the text,
     a caret; and the anchors (matched words' places) for scrolling together.
   - `visual.rs` and `run.rs`: pages without text compared as images; the whole
     comparison of two open sessions, with progress and stopping, the text read on a few
     threads per document and pages compared as images on more.
   - `annotate.rs` and `report.rs`: the annotated copy and the report.
2. **The documents** are opened as sources (as the Combine view's files are, section 6.4)
   and shown by ordinary `DocTab` views that belong to the Compare view, not to the tab
   row. `DocTab.comparing` marks them: the viewer draws the differences' marks
   (`PageView`), keeps the Select tool, leaves annotations alone, and hands scrolls to the
   Compare view's own synchronization (`split/sync.ts`) instead of a joined tab's.
3. **Scrolling together** maps a place in one document to the other by the anchors,
   linearly between them, keeping the offset the following pane had when the leading pane
   took the lead. Split view's lock counts pages, which would drift across an inserted
   page.
4. **The annotated copy** marks the text a document has with a Highlight in the kind's
   colour and a comment quoting the other document, and the place where it lacks the other
   document's text with a Text annotation (sticky note) with the same comment. Highlights'
   quads are computed from the same text Compare read (the page without its
   annotations), so annotations already on the page do not shift them. It is written
   incrementally to a copy, through a session of its own; the session the view shows is
   never changed.
5. **The report** is HTML laid out by MuPDF's story engine on A4 pages, with the system's
   fonts for scripts the built-in fonts lack (`fonts::install`).

## Alternatives considered

- **Page-by-page comparison** (FineReader's own help pairs pages): simpler, but an inserted
  paragraph makes every later page different. Comparing the whole text and then placing
  each difference on its pages keeps one difference one difference.
- **A diff crate** (`similar`, Apache-2.0/MIT): it would have done the Myers part, but not
  the anchoring on repeated words, the budget, or the cleanup for words; the code it saves
  is about 150 lines.
- **Character-level diff** (diff-match-patch): finer marks inside words, but noisy on real
  documents and slower; FineReader reports words.
- **Showing the documents as a joined tab** (split view): its panes would get the side
  panels, the commands and the annotation tools, which act on documents in tabs, and its
  lock counts pages. FineReader's view is a window of its own.
- **Acrobat's Caret annotations for insertions:** they say "inserted here" more precisely
  than a note, but Caret is outside the interop profile (section 5.1, rule 1), and the user
  chose FineReader's way.
- **A Word report** as FineReader's: dropped with DOCX conversion; a PDF report needs
  nothing new.

## Consequences

Comparing needs text: scans compare only as pictures until OCR comes (the next planned
tool), which will give them text to compare. Moved passages show as deleted in one place
and inserted in another, as in FineReader. A two-column page reads in MuPDF's reading
order, and its anchors go back at each column top, which scrolling together skips.
