# 0014: Neo's colours, solid instead of Mica

- Status: accepted (the user's decision, 2026-10-06)
- Date: 2026-10-06

## Context

The user likes Neo, a theme for Obsidian (x0aa7i/obsidian-neo, v0.7.1, MIT), and asked for
its light theme and its Midnight dark theme in Lectrix, keeping the brand identity first.

Neo is mostly a palette: twelve neutral steps (`base-05` to `base-60`) tinted to hue 230,
a purple accent (hue 250), Inter as the font, plus a few touches of its own: pill tabs,
flat menus, scrollbars shown on hover and a hand cursor on clickable things. Midnight is
its near-black variant (5 to 12% lightness backgrounds).

AGENTS.md section 8 had a Mica window background, with the chrome transparent so the
system material shows through. Mica is tinted by the wallpaper and the system, so it
cannot show Neo's colours.

## Decision

1. **Neo's neutrals, tinted to Lectrix blue's hue (217) instead of 230**, which is Neo's
   own "accent affects background" option, and close to the brand kit's neutrals. The
   tokens in `src/app.css` map Neo's roles: the title bar and panes are Neo's secondary
   background, the view bar and fields its primary background, menus and notices its
   menu colour, dialogs and floating panels its prompt colour in Midnight. The canvas
   behind the pages is darker than the panes in both themes, so white pages stand out
   (light: `base-50`; Midnight: `base-60`, Neo's editor).
2. **The brand stays:** the accent is Lectrix blue (ADR 0010), selected rows and the
   active tool keep their accent tint, the font stays Google Sans (ADR 0009), reds are the
   brand's signal red (`#C9303A`; `#E5484D` lightened to `#E85A5F` for text in the dark
   theme). The Combine view's file marks are Neo's named colours, deepened in the light
   theme where needed for 3:1.
3. **Neo's text tones**, with titles and headings in its brightest step
   (`--lectrix-fg-strong`). Midnight's muted text is Neo's `base-20` two lightness points
   up (`#929baa`), so it reads at 4.5:1 on a selected row in a floating panel.
4. **No Mica.** The window is opaque, and its own background is the chrome colour of the
   theme in use (`paint_window_background` in `src-tauri/src/lib.rs`, kept in step with
   `src/app.css` by a test), so it is not white before the page paints. The `Backdrop`
   platform call, the `backdrop` startup field and the Windows build check are removed.
5. **Neo's touches:** document tabs are pills, the active one raised (ringed with a line
   in the light theme); menus, the colour panel, the comment tooltip and notices are flat
   (no shadow, a thin edge; light menus have the canvas's colour, so their edge is a step
   darker than other lines); buttons use Neo's control fill; clickable controls show the
   hand cursor, except the window buttons, which keep the arrow as Windows' own do.
   Dialogs and the panels floating over the pages (toolbar, inspectors, search) keep their
   shadow, as Obsidian's modals do.
6. **No native lists or pickers.** The webview draws a native `<select>`'s list and the
   native colour picker in the system's colours, outside the theme. Dropdowns are Bits UI
   Selects styled as menus (`src/lib/components/Dropdown.svelte`), and the custom colour
   is Lectrix's own picker (`HexPicker.svelte`), which takes hex values only. Native radio
   buttons and sliders take the accent through `accent-color`.
7. **More contrast than Neo where neighbours blurred** (2026-10-06): lines are a step
   darker (light `#ccd3dd`, Midnight `#303540`), Midnight's menus have a lighter edge
   (`#3a404a`), and hover and pressed fills are stronger (light 14% and 20%, Midnight 10%
   and 12%), so a highlighted menu item stands out at about 1.2:1 where Neo's was 1.1:1.
   Midnight's active tab and buttons are a step lighter. Text still reads at 4.5:1 on
   every fill (`src/lib/contrast.test.ts`).
8. **Controls have one even edge** (the user's choice, 2026-10-06): buttons, fields and
   dropdowns are outlined with the theme's line on every side, a step stronger
   (`--lectrix-line-strong`) under the pointer, and an even 2 px accent ring when focused
   or open. This replaces the Windows 11 style of a darker bottom edge at 3:1 and an accent
   underline on focus. Fields are still told apart by their own fill, their outline and
   their labels; their edge no longer reaches 3:1 on its own.

## Alternatives considered

- **Neo's purple accent.** Fully faithful, but replaces the brand colour (ADR 0010), and
  its light shade carries white text at about 2.7:1.
- **Neo's greys at hue 230.** Slightly more indigo than Lectrix blue.
- **Mica on the title bar and panes, Neo elsewhere**, or Mica as a Settings option. Neo's
  look would show only in part and change with the wallpaper.
- **Neo's editor colour behind the pages** in the light theme (near white): pages would
  stand apart only by their shadow.
- **Neo's scrollbars shown only on hover.** Not taken (the user's choice); the thin
  scrollbars stay.
