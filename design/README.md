# Lectrix design sources

The Lectrix brand kit's sources: the app icon, the wordmark, the lockup, the PDF file icon
and the Yaru versions, with the brand's colours and type (ADR 0013). Nothing here is built
into the app or shipped as it is. The app keeps its own copies where it uses them:

| Used in the app | Copied from |
| --- | --- |
| `src-tauri/icons/`, `src-tauri/app-icon.png` | the kit's platform exports of `app-icon/` |
| `src/lib/assets/lectrix-icon.svg` (About dialog) | `app-icon/lectrix-icon.svg` |
| `src/lib/assets/fonts/` (ADR 0009) | the kit's Latin, Cyrillic and Greek font build |
| accent colours in `src/app.css` (ADR 0010) | `tokens.json` |

When a file here changes, copy it to the app in the same change and say so in the commit.

## Licence

Lectrix's own artwork in this folder (everything except `yaru/`) is © 2026 the Lectrix
authors, **all rights reserved**. It is not covered by the repository's AGPL-3.0-or-later
licence, and it may not be reused or adapted without permission.

`yaru/` is derived from the Yaru icon theme and is licensed under **CC BY-SA 4.0**
(`yaru/LICENSE.md`).

## Contents

| Path | What it is |
| --- | --- |
| `tokens.json` | Colours, gradient, type, icon proportions and radii |
| `lectrix.css` | The kit's stylesheet (tokens, light and dark theme, helpers), for reference only. Its `@font-face` rules point at a `fonts/` folder that is not kept here; the app's font build is in `src/lib/assets/fonts/`. |
| `app-icon/` | Master SVGs of the app icon (below) and the macOS Icon Composer layers |
| `wordmark/` | "Lectrix" as outlines: ink on light, white on dark |
| `lockup/` | Icon and wordmark together, light and dark, as SVG and a 4× PNG |
| `file-icon/` | The PDF file icon: master SVG, small-size SVG, and `windows/lectrix-pdf.ico` with its PNGs |
| `yaru/` | Ubuntu Yaru versions of the app icon and PDF file icon (CC BY-SA 4.0, below) |

## Colours

| Token | Hex | Use |
| --- | --- | --- |
| blue-200 | `#A3C8F9` | Top of the icon gradient |
| blue-300 | `#6FA3EF` | **Primary**, soft blue; the app's accent in the dark theme |
| blue-400 | `#4A82E0` | Bottom of the icon gradient |
| blue-500 | `#2F5DAA` | The app's accent in the light theme (ADR 0010) |
| blue-700 | `#142247` | **Ink**: text on light backgrounds |
| blue-800 | `#0F1A33` | Dark background; text on the dark theme's accent |
| red-400 | `#E5484D` | Signal red, used sparingly for alerts |
| red-600 | `#C9303A` | Signal red for text on white |

The full palette, with the neutrals, is in `tokens.json`. The icon gradient runs top to
bottom, `#A3C8F9` → `#4A82E0`.

`#6FA3EF` is too light for white text or for outlines on light surfaces, so the app's
light theme uses `#2F5DAA` instead (ADR 0010). New annotations take Acrobat's default
colours, not the brand red (AGENTS.md section 6.5).

## Type

- **Google Sans**, © 2025 The Google Sans Project Authors, SIL Open Font License 1.1.
  The app bundles the Latin, Cyrillic and Greek build, about 200 KB per style (ADR 0009).
- Weights: 400 for body text, 500 for menus and secondary labels, 600 for buttons, 700
  for headings and the wordmark.
- The wordmark is weight 700 at −0.02em letter-spacing, converted to outlines.
- Scripts outside the bundled build fall back to the system fonts. Russian strings run
  15–30% longer than English, so leave room in toolbars and tabs.

## App icon

| File | What it is |
| --- | --- |
| `lectrix-icon.svg` | The master: Apple's icon grid, with the glass effect drawn in |
| `lectrix-icon-full.svg` | A larger body, with glass |
| `lectrix-icon-flat.svg` | No glass |
| `lectrix-icon-small.svg` | Thick stroke for 32 px and below |
| `lectrix-symbol.svg` | The letter alone, in `currentColor` |
| `lectrix-symbolic-16.svg` | 16 px symbolic icon |
| `icon-composer/` | Flat, unmasked 1024 × 1024 background and foreground layers for macOS 26 Icon Composer |

- **macOS 26 and later:** build the icon in Icon Composer from the two layers; the system
  adds the Liquid Glass effect, the shape mask and the dark, clear and tinted looks.
  `AppIcon.icns` is the fallback for older macOS.
- **Windows, Linux, web:** the glass effect is drawn in, because these systems don't add it.
- **32 px and below:** separate, simplified drawings, with a thicker stroke (2× at 16 px)
  and no glass, so the letter stays readable. Use them; don't scale the large icon down.
- **Proportions** (`tokens.json`): corner radius 22.5% of the icon's size, macOS body
  80.47% of the canvas, stroke 4.69%.

## Rules

- **Clear space** around the icon and the lockup: at least 25% of the icon's height.
- Don't recolour the icon, add outlines to it, or place it on the blue gradient.

## PDF file icon

- The outline and folded corner of Acrobat's PDF file icon. The frame and the swan share
  one gradient, `#6FA3EF` → `#4A82E0`, top to bottom. The swan runs off the page edge and
  tucks under the fold, as it runs off the app icon. The page is `#F7FAFE`, and the "PDF"
  label is Google Sans Bold at −0.02em in ink `#142247`, converted to outlines.
- **32 px and below:** `lectrix-file-icon-small.svg`, with no label, a thicker frame and a
  larger swan. `windows/lectrix-pdf.ico` (16 to 256 px) switches to it at 32 px and below.
- **Windows:** `lectrix-pdf.ico` is meant for the `Lectrix.Document` file type.

## Yaru versions

`yaru/` holds versions drawn on the Ubuntu Yaru icon theme's templates, in Yaru's folder
layout, each a source sheet plus PNGs from 16 to 256 px at 1× and 2×:

- `apps/square/` (208 × 208 at 256 px) and `apps/vertical/` (vertical oblong, 176 × 224
  at 256 px): the app icon in two shapes, named `lectrix` like the desktop file's `Icon=`.
  They keep the brand gradient, lighter at the top, although Yaru's guide prefers the
  darker shade there. From 32 px down they use the brand's simplified swan.
- `mimetypes/`: Yaru's `application-pdf` icon in Lectrix blues (darker at the top, as
  Yaru asks), with a white swan and no label, drawn separately at 48, 32, 24 and 16 px.

They are design sources only. The Linux packages don't install them: `application-pdf`
is the icon theme's own icon for every PDF, whichever app opens it, and the Yaru package
owns that path. Both app icon shapes are kept until one is picked.

Unlike the rest of this folder, they are derived from Yaru artwork, so they are
**CC BY-SA 4.0**, © the Yaru authors; `yaru/LICENSE.md` names the originals and the
changes.
