# 0030: The start screen's tools are cards, with motion longer than 150 ms

- Status: accepted (the design handed over by the user, 2026-10-25)
- Date: 2026-10-25

## Context

The user handed over a redesign of the start screen (a design file and a handoff note):
the tools as cards with a line on what each does, which lift under the pointer, pop their
icon tile with a small spring and send a ripple out from a click; the recent files' rows
highlighted with rounded corners. Its timings are 150 ms for colours and fades, 250 ms for
a card's lift and the arrow, and 400 ms for the icon's spring and the ripple.

AGENTS.md section 8 keeps animations under 150 ms. The handoff's colours are named after
the brand kit's stylesheet (`design/lectrix.css`: its blues, its navy dark theme), while the
app's greys are Neo's (ADR 0014), and its focus ring is a translucent 3 px glow.

## Decision

1. **The handoff's timings, on the start screen only.** Tokens in `src/app.css`
   (`--lectrix-duration-fast`, `-base`, `-slow`, `--lectrix-ease-out`, `-spring`) carry
   them; reduced motion sets the durations to 0 and turns every transition and animation
   off, so nothing lifts gradually and no ripple runs. The rest of the app keeps 150 ms and
   less: a card is a large target the pointer rests on, and a slower lift reads as weight
   there, where a menu or a tab that took 250 ms would feel slow.
2. **The handoff's colours mapped onto the app's tokens.** The brand blues it names become
   `--lectrix-tint` (blue-50), `--lectrix-tint-strong` (blue-100), `--lectrix-tint-line`
   (blue-200) and `--lectrix-card-hover-line` (blue-300) in the light theme, and the accent
   mixed into the raised surface (and the accent) in the dark theme, as the handoff says for
   dark. Its surface, border, text and muted text are the app's raised surface, line, strong
   text and muted text; its soft red is `--lectrix-danger-bg`. Shadows are tokens too, black
   in the dark theme, where the kit's navy shadow would not show on Neo's near-black.
   A recent file's row under the pointer takes the tool cards' colour and edge rather than
   the blue-50 tint with a blue-100 ring, and keeps Lectrix's PDF file icon (both the user's
   decision). The switch between the table and the grid of recent files keeps the mark every group of
   tools in the app has (an accent ring over a tint of the accent, gliding to the one
   picked), not the handoff's blue-100 fill with a blue-200 ring (the user's decision).
3. **The app's focus ring.** A card or a row with the keyboard focus looks as it does under
   the pointer, with the app's 2 px accent outline (AGENTS.md section 8), not the handoff's
   translucent glow, which falls short of 3:1.
4. **Lucide icons** in place of the design's Material Symbols, as the app has them:
   FolderOpen, Layers, LayoutGrid, LockOpen, Lock, Shrink, ArrowUpRight, Star, X.

## Alternatives

- Squeezing the handoff's timings under 150 ms: the spring and the ripple lose their shape
  in that time, and the lift reads as a jump.
- Taking the brand kit's palette for the start screen: its navy dark theme beside Neo's
  greys would make the start screen a different app from the documents' screen.
