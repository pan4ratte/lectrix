# 0022: Feature specifications live in docs/spec/, outside AGENTS.md

- Status: accepted (the user's request, 2026-10-18)
- Date: 2026-10-18

## Context

AGENTS.md was the single source of truth, to be read in full at the start of every
session. By 2026-10-18 it had grown to 131,000 characters (about 33,000 tokens). Sixty
per cent of that was section 6, the behavior of each feature down to its timings, sizes
and wording, and much of section 8 described single components the same way. Every
session carried all of it, whatever it worked on. The rules that hold for every change
(scope, licence, security, the interop contract, saving, the working rules) were buried
among animation timings. The file is also loaded automatically into every agent's
context, so its size was paid on every request.

## Decision

1. **Section 6 moves to `docs/spec/`,** one file per feature, keeping its number:
   `viewer.md` (6.1), `bookmarks.md` (6.2), `page-labels.md` (6.3), `combine.md` (6.4),
   `annotations.md` (6.5), `settings.md` (6.6) and `organize.md` (6.7). Their text moved
   unchanged, apart from each file's title and opening line. `docs/spec/README.md` lists
   them.
2. **The detailed parts of section 8 move to `docs/spec/ui.md`:** the layout (8.1), the
   app's own controls (8.2), how notifications, dialogs and progress bars move (8.3), and
   shortcuts on non-Latin keyboards and on macOS (8.4). AGENTS.md keeps the rules every
   piece of interface follows: the look, behavior, accessibility and the default
   shortcuts.
3. **AGENTS.md keeps a short index in section 6:** each feature in a few lines, with
   the constraints that matter beyond the feature (what is never written to the file,
   what Rust alone opens, where settings and shortcuts are declared), and a link to its
   spec.
4. **Section numbers stay as they were,** so the references in code, tests, ADRs and the
   specs themselves ("AGENTS.md section 6.5", "section 8") still lead to the right text
   through the index. They were not rewritten.
5. **The working rules change with it:** a session reads AGENTS.md and `docs/status.md`,
   then the spec of the feature it changes; a change in behavior is written into that
   spec in the same commit; details of a feature go in its spec, not in AGENTS.md.

## Alternatives considered

- **Leave AGENTS.md whole.** Simplest, and nothing could be missed, but its cost grows
  with every feature, and it was already larger than most of the code a session reads.
- **Delete the settled details and let the code speak.** Many of them name the code that
  holds them, but they record what the user agreed to and why; the code alone does not
  say which of its behaviors were decided and which are accidents.
- **Renumber, or point every reference at the new files.** More exact, but it touches
  some hundred files for no change in meaning, and the old numbers would still be in git
  history and commit messages.

## Consequences

AGENTS.md is about 45,000 characters (some 11,000 tokens). A session working on one feature reads that plus
one spec, instead of every feature's details. The risk is a session that changes a
feature without reading its spec; the rule in section 10 and the index in section 6 are
there against it. A behavior shared by several features is described in the spec of the
one that owns it, and the others refer to it by its section, as they did before.
