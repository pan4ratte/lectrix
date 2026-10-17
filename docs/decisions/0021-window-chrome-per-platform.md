# 0021: The title bar, window buttons and dialogs follow each platform

- Status: accepted (the user's request, 2026-10-18)
- Date: 2026-10-18

## Context

Section 8 drew one title bar for every platform: Tauri's decorations off, and Windows'
minimize, maximize and close at the right, 46 px wide, close turning red. On macOS that
puts the buttons on the wrong side, in the wrong shape, with no window shadow or rounded
corners from the system; on Linux it ignores where the desktop puts them (GNOME shows close
alone, elementary OS puts it at the left). Dialogs looked the same everywhere, centred in
the window over a strong dim, which is not how a Mac shows a question. The user asked for
the title bar, its buttons and, on macOS, the dialogs to look and sit as in the platform's
own apps.

## Decision

1. **macOS keeps the system's frame and buttons.** Windows are made with decorations on,
   `TitleBarStyle::Overlay` and a hidden title (`windows::create`): the window has the
   system's shadow, corners, resizing and green-button full screen, and the traffic lights
   lie over the left end of Lectrix's own 35 px bar, at `trafficLightPosition` (13, 17),
   centred in it. The bar leaves 80 px for them (`TitleBar.svelte`), except while the window
   is in a full-screen space of its own, where the system hides them. If the system ends
   full screen by itself (the green button in the bar it shows at the top of the screen),
   Lectrix's full screen follows (`FullScreen.windowResized`).
2. **Linux draws the buttons the desktop names, where it names them.** Rust reads GTK's
   `gtk-decoration-layout` (`Platform::window_chrome`, `parse_decoration_layout`), which GNOME
   takes from its `button-layout` and KDE writes from its own settings; the frontend asks for
   it (`window_chrome`) and draws those buttons at those ends, in Adwaita's style: round,
   24 px, a faint fill, close like the others. Without a layout, Windows' three at the right.
3. **Windows is unchanged.**
4. **Dialogs on macOS are sheets** (`:root[data-platform='macos']` in `app.css`): they come
   down from under the title bar into place just below it and go back up, over a light dim,
   with 12 px corners, a soft deep shadow and 28 px buttons with 6 px corners. Questions
   (`DialogHost`) are alerts as the system lays them out since macOS 11: narrow, the app's
   icon over a centred title and message, the buttons as wide as the alert, two side by side
   with the main one at the right, more stacked with it on top and Cancel at the bottom; the
   main one has the focus, so Return presses it (`alert.ts`). Other dialogs put their main
   button at the right, as most already did.
5. The frontend knows the platform from the webview's user agent (`src/lib/platform.ts`),
   so the first frame is laid out for it, and marks the root with `data-platform`.
6. **The menus are in macOS's menu bar** (decided after the rest, at the user's request,
   2026-10-18). They are laid out once in the frontend (`menus.svelte.ts`): the title bar
   renders them on Windows and Linux, and on macOS each window sends them, as data, to Rust
   whenever they change (`set_menu`), which builds the system's menus from those of the
   window in use (`menu.rs`) and sends the id of an item chosen back to that window
   (`menu-command`). A change that keeps their shape is made to the items in place. On
   macOS they are arranged as Mac apps arrange theirs: an application menu (About, Check for
   updates, Settings, Services, Hide, Quit), the system's own Cut, Copy, Paste and Select All
   in Edit, a Find submenu, a Window menu, and labels in title case. While a dialog is open,
   nothing in them acts on the window.
7. **Shortcuts on macOS use Command.** Shortcuts were already matched with Ctrl or Command
   alike; now they are shown as the system shows them (⇧⌘S, with ⌃ for Tab, whose Command
   is the system's), Command+H, Option+Command+H and Command+M are left to the system, and
   some commands come with the keys Mac apps use (`mac` in `shortcuts.ts`: ⌘Q, ⇧⌘Z,
   ⌘G, ⌥⌘G, ⌘[, ⌘], ⇧⌘F). Stored shortcuts keep "Ctrl", so a settings file means the same
   on every platform.

## Alternatives considered

- **Native decorations on Linux** (the window manager's or GTK's title bar): native by
  definition, but a second bar above Lectrix's tabs and menus, about 46 px of height lost.
- **Drawing macOS's traffic lights in the webview:** they would not take the system's
  accent, hover behaviour, window-tiling menu on the green button or full screen, and the
  window would still lack the system's shadow and corners.
- **A per-desktop style on Linux** (Breeze on KDE): the layout already follows KDE's
  settings; the look is Adwaita's, the GTK toolkit Lectrix's window is made with.
- **The macOS menus defined in Rust:** a second definition of the same menus to keep equal
  to the title bar's, and what each item's state depends on (the document's undo steps, the
  panes, the selection) is the frontend's.
- **The system's Undo and Redo items** on macOS: they would undo a text field's typing
  without help, but not the document's changes, which are Lectrix's.

## Consequences

- A key with Command reaches the page first (wry passes it on to the menu bar only if the
  page does not take it), so the app's own shortcuts still run the commands; the menu bar
  runs those the page leaves, such as ⌘Z in a text field, which its Undo item then hands
  to the field (`document.execCommand`).
- Edit > Copy is the system's, so it copies a text field's or the page's selection; the
  document's selected text, which is Lectrix's own and not the page's, is copied with ⌘C,
  the selection's bar or its menu, not from the menu bar (the system leaves the item
  disabled for it, and a copy started from the menu bar would not be allowed to write to
  the clipboard).
- Nothing of this has been seen on a Mac or a Linux desktop yet: the traffic lights'
  position, the sheets and the Adwaita buttons need a look there.
- Tahoe (macOS 26) changed its alerts' alignment; the alert follows the macOS 11–15 layout
  until it has been compared on 26.
