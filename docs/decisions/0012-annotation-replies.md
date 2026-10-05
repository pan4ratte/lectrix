# 0012: Replies to annotations

- Status: accepted (asked for by the user on 2026-10-05)
- Date: 2026-10-05

## Context

AGENTS.md section 5.2 had Lectrix show other apps' replies (`/IRT`) under their parent,
read-only, and Lectrix wrote none. The user asked for a Reply command in the annotation
list. A reply is an annotation in the file, so it falls under the write profile (section
5.1), whose rules assume an annotation is something drawn on the page: rule 2 (an
appearance that shows), rule 4 (`/Rect` contains what is drawn) and rule 6 (notes get a
`/Popup`). A reply is the opposite: readers show it in its parent's thread, not on the
page.

## Findings

- **PDF 32000-1 12.5.6.2:** a reply is a markup annotation with `/IRT` pointing at the
  annotation it answers (`/RT /R`, the default). Acrobat writes replies as `/Text` notes
  and lists them in the parent's thread. iText's guidance, followed by other libraries,
  gives a reply its parent's `/Rect`.
- **pdf.js 6.3** (Firefox) does nothing special with replies. It reads `/IRT` into
  `inReplyTo`, but its annotation layer treats a reply like any note: without an
  appearance it draws its own 22 pt note icon; with one it draws that. Either way it
  gives the reply an element over its `/Rect` with its own hover pop-up. A reply with its
  parent's `/Rect`, coming later in `/Annots`, would sit over the parent and take its
  hover: pointing at a sticky note would show the reply instead of the note. pdf.js's
  annotation layer skips any annotation whose `/Rect` has no area.
- **MuPDF and PDFium** draw a reply's appearance like any other annotation's; without
  one, they draw a note icon of their own. MuPDF also redraws the appearance of a new or
  edited annotation at its next update unless the annotation is no longer marked dirty.
- **Acrobat and Foxit** find a reply by `/IRT` and show it in the parent's thread. This
  could not be run here; the manual release checklist covers it.

## Decision

A reply Lectrix writes is a `/Text` annotation with:

- `/IRT` (the parent), `/Name /Comment`, `/Open false`, the parent's `/C` (yellow when it
  has none), `/Contents`, and rule 6's metadata (`/NM`, `/T`, `/CreationDate`, `/M`,
  `/F 4`, `/P`);
- a **`/Rect` of no area at the parent's top-left corner** (user space), so pdf.js's
  annotation layer gives it no element that could cover its parent;
- an **empty normal appearance** (an empty form with `/BBox [0 0 0 0]`), so MuPDF, PDFium
  and pdf.js draw nothing for it, and pdf.js draws no icon of its own;
- **no `/Popup`**: readers show the reply in its parent's thread.

These are exceptions to rules 2, 4 and 6 for replies only (`docs/interop-profile.md`,
"Replies"). Editing a reply changes only its text or author, plus `/M`; nothing redraws
its appearance. Repair gives another app's reply that has no appearance an empty one,
not a note icon over its parent. Another app's reply that already draws something keeps
it (rule 10). Deleting an annotation deletes its replies, as before. Lectrix writes
replies only to annotations shown as rows (not to replies), as Acrobat's list does.

In Lectrix, replies are shown in their parent's thread in the annotation list, and the
viewer does not hit-test them.

## Alternatives considered

1. **The parent's `/Rect`, as iText and others write.** pdf.js would put the reply's
   element over the parent and take its hover; with no appearance, every engine but
   Acrobat would also draw a second note icon over the parent.
2. **The Hidden flag (`/F 2`).** Every engine would skip the reply, but whether Acrobat and
   Foxit list hidden annotations in their comment threads is not something that could be
   checked here, and a reply that does not show in the thread is lost.
3. **A `/Popup` for each reply.** It would give pdf.js a pop-up element of its own, with
   the same problem as alternative 1, and Acrobat draws replies in the parent's pop-up.

## Verification

- `pdf-core` tests (`annotations::replies_are_threaded_notes_that_nothing_draws`,
  `replies_are_undoable_steps_through_the_session`): every key above after save and
  reopen; `qpdf --check`; MuPDF draws the page pixel for pixel as without the reply, also
  after editing and after repair; text and author edits only; one undo step each.
- Interop harness `phase5` (`replies.pdf`): replies to a highlight and a sticky note.
  MuPDF, PDFium and pdf.js draw both pages exactly as before (0 pixels changed); pdf.js
  links each reply to its parent and sees a `/Rect` of no area; the parents still pass
  every visibility check.
- Manual checklist: Acrobat Reader, Foxit, Edge and Firefox show the replies in the
  threads of `replies.pdf` and no stray icon. If Acrobat or Foxit don't list them, the
  fallback is alternative 1 with the empty appearance, and Firefox's hover issue is
  recorded in `docs/status.md`.
