# 0010: The accent is Lectrix blue, not the system accent

- Status: accepted (the user's decision, 2026-10-05)
- Date: 2026-10-05

## Context

AGENTS.md section 8 had the app follow the Windows accent color, shaded per theme where
needed for 3:1 contrast (`accentShades` in `src/lib/theme.ts`, read from the registry by
the app's `platform` module). The Lectrix brand kit's primary color is a soft blue,
`#6FA3EF`, and the user asked for it to be the app's accent.

`#6FA3EF` cannot be used as it is in the light theme: white text on it reaches about 2.6:1
(4.5:1 is needed), and as a focus ring or outline on the light surfaces it falls below the
3:1 that WCAG 1.4.11 asks for. The brand kit's own light and dark themes (`lectrix.css`)
already pair it with a deeper shade for light backgrounds.

## Decision

1. **The accent follows the brand kit's themes:**
   - light theme: `#2F5DAA` (the kit's blue-500) with white text, 6.4:1;
   - dark theme: `#6FA3EF` (the brand blue itself) with ink text `#0F1A33`, 6.7:1.

   Both are at least 3:1 on every surface of their theme (5.2:1 at worst, on the light
   canvas). They are fixed tokens in `src/app.css`, and `src/lib/contrast.test.ts` checks
   them like every other token, including the active tool's border on its tint.
2. **The system accent is no longer read.** The registry read (`Platform::accent_color`),
   the `accentColor` startup field and the per-accent shading in `theme.ts` are removed,
   with the tests over Windows' 48 accent presets.
3. **Everything that used the accent follows**: primary buttons, focus rings, selected
   rows and tabs, text selection, field focus, radio buttons and checkboxes, annotation
   outlines, and the active annotation tool, which now has an accent border on every side
   instead of an accent bottom edge.

## Alternatives considered

- **`#6FA3EF` in both themes, with dark text on buttons.** Buttons would read, but focus
  rings and outlines on the light surfaces would fall below 3:1.
- **A Settings choice between Lectrix blue and the Windows accent.** Not asked for; it
  can be added later by restoring the removed registry read and shading.
