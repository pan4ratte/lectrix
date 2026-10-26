# Platform gaps (macOS and Linux)

Part of the status: [docs/status.md](../status.md) is the index a session reads, with one line for each item; this file has the platform gaps in full. Keep both current.

CI builds the app and its tests on both, and runs the tests without failing the job.
Nothing has been run interactively there.

- **Printing** (section 6.12, ADR 0028) goes through CUPS (`lpstat`, `lpoptions`, `lp`),
  which both have; built and its parsing tested on Windows, but not yet run on either. CUPS
  does not report a printer's margins, so pages are fitted inside 12 pt ones, and paper
  sizes are read from the PPD's names (the common ones and PWG names); a printer offering
  only others prints on A4 as far as the preview goes.
- **Fonts:** no index of installed fonts (ADR 0005). Non-embedded fonts other than the
  base 14 render with MuPDF's substitutes, and non-embedded CJK text has no glyphs. This
  needs fontconfig (Linux) and Core Text (macOS) behind the existing `SystemFonts` trait.
- **Window:** the title bar follows each platform (ADR 0021): the system's traffic lights
  over its left end on macOS, GTK's button layout in Adwaita's style on Linux, and sheets and
  alerts for dialogs on macOS. None of it has been looked at on a Mac or a Linux desktop:
  the traffic lights' place in the 35 px bar, the alert on macOS 26 (whose alerts changed
  alignment) and the buttons under KDE (Adwaita's look in Breeze's place) need a look. The
  menus are in macOS's menu bar and shortcuts use Command there; neither has been tried on
  a Mac, nor has ⌘Z undoing a text field's typing through the menu bar
  (`document.execCommand`). The selected text is handed to the system as the webview's own
  selection (section 6.1, 2026-10-20): read through UI Automation on Windows, but macOS's
  accessibility API (Easydict), Edit > Copy and Services with it, and Linux's primary
  selection have not been tried. A tab dropped where two Lectrix windows overlap goes by the windows' frames, not
  by which one is on top (only the Windows build asks the system for the window under the
  cursor), and moving tabs between windows has not been run there. Nor has full screen
  (on macOS the system's own, with its animation and its menu bar at the top edge).
- **Show in folder** (2026-10-18, built, not run there): a recent file's Show in Finder is
  `open -R`; Linux's Show in folder asks the file manager over D-Bus (`dbus-send`,
  FileManager1's ShowItems) and opens the folder alone without it. Neither has been tried,
  nor how a desktop without `dbus-send` falls back.
- **Touchpad pinch on Linux** (2026-10-08, built, not yet tried with a real touchpad):
  WebKitGTK magnified the whole app. `platform/linux.rs` now stops the pinch on the
  webview widget's `event` signal (before WebKit's bubble-phase zoom gesture) and emits
  `touchpad-pinch`; `viewer/pinch.ts` hands it to the page as Ctrl+wheel. macOS still
  unhandled.
- **Scrolling on Linux at a fractional display scale** (2026-10-12). The user's laptop:
  2560x1600 at 120 Hz, scale 1.333 (1920x1200 logical; GTK draws the window at 200%,
  3840x2400). Measured on that screen with `tests/perf/wheel-linux.py` (real two-finger
  scroll through Mutter's remote desktop API, `LECTRIX_PERF=watch`), maximized, the
  user's own state: frames 22.0 ms on average (45 fps) before the fix below, 16.8-17.1 ms
  after (a steady 60 fps, p95 20-22 ms, none over 50 ms). Smooth on a 120 Hz screen needs
  8.3 ms; two caps outside Lectrix hold it at 60 (below). An earlier headless Mutter at
  3840x2400 and 60 Hz (not the laptop's screen, as first thought) gave 49-60 ms before,
  19 ms after; headless Mutter cannot show more than 60 fps (no CRTC, so WebKit's vblank
  timer falls back to a fixed 60 Hz).
  - Fixed, the cause: GTK 3 paints each frame of the window into a new window-sized image in memory
    (5760x3600 there, 83 MB) and fills the window's background colour into it before
    WebKit draws its frame over it through GL. Filling fresh memory every frame kept the
    UI main thread at 16% user and 60% system time (page faults). `perf` (user time only,
    `cpu-clock:u`; it works inside the toolbox on processes it starts, not attached to
    host ones) showed `gtk_window_draw` -> `pixman_fill`; per-thread `/proc` times showed
    the system share.
  - Fix: `platform/linux.rs` (`skip_covered_window_background`) draws the window's
    children itself and skips GTK's background, except until the page has loaded and for
    0.5 s after a size change, when WebKit's frame may not cover the window. GTK still
    sets the opaque region from the colour. Startup and a live resize (Alt+F8), recorded
    through Mutter's screen cast, look as before.
  - Earlier: WebKit gets an accelerated canvas (`willReadFrequently` only on Windows,
    `RenderedImage.svelte`): blank pages worst 397 ms -> about 90 ms, first page
    345 -> 150 ms. No effect: passive wheel listeners, `will-change: transform`, skipping
    text geometry, `img.decode()`, `GDK_GL=always`. Worse:
    `WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_SKIA_ENABLE_CPU_RENDERING=1`.
  - **Cap 1, WebKit:** `PreferPageRenderingUpdatesNear60FPS` (a WebKitGTK feature,
    on by default) holds page updates near 60 fps although WebKit's `VBlankMonitor`
    follows the display at 120 Hz through DRM. Turned off since 2026-10-12
    (`render_at_display_rate` in `platform/linux.rs`; WebKitGTK 2.42+, looked up at run
    time, since the `webkit2gtk` crate has no binding): 14.1-14.3 ms on average, 24-29% of
    frames one refresh apart (p50 16 ms). The user judged it better by eye. These are
    animation-frame times; the screen still gets at most 60 new frames a second (cap 2),
    so the gain is likely fresher frames rather than more of them.
  - **Cap 2, GTK 3:** a GTK 3 window drawn with GL never requests Wayland frame
    callbacks: `gdk_window_impl_wayland_end_paint` drops `pending_commit` when the paint
    used GL (`eglSwapBuffers` commits), so `on_frame_clock_after_paint` requests none,
    and `GdkFrameClockIdle` falls back to `FRAME_INTERVAL` (16667 µs). A bare
    `GtkGLArea` runs at 59-60 fps, the same window drawn with cairo at 113-119;
    `WAYLAND_DEBUG` shows no `wl_surface.frame` from the GL window. WebKitGTK's GTK 3
    window paints its frames through GL in that frame clock. GTK 4 is not affected the
    same way (Vulkan 119 fps; its GL paths measured 58-87 here, not investigated), and
    Papers, Fedora's PDF reader, is a GTK 4 app on Vulkan. `GDK_GL=disable` lifts the cap
    but copies WebKit's frames through the CPU: scrolling falls to about 10 fps. No
    public GTK 3 call restores the frame callbacks, so Lectrix cannot work around it.
    Reported 2026-10-12: GTK issue 8461
    (https://gitlab.gnome.org/GNOME/gtk/-/work_items/8461, text in
    `docs/upstream/gtk3-wayland-gl-frame-clock.md`) and a comment on WebKit bug 199376
    (https://bugs.webkit.org/show_bug.cgi?id=199376, `docs/upstream/webkitgtk-60fps-comment.md`).
  - What is left: WebKit's web-process main thread is now the busiest (about 60% of a
    core while scrolling). About a fifth of it decodes page PNGs (`createImageBitmap`
    decodes on that thread in WebKit), a quarter updates rendering and compositing layers.
  - Build environment used: toolbox `lectrix-dev` (Rust 1.99.0, WebKitGTK/GTK 3 devel,
    clang, perf), with `RUSTUP_HOME`/`CARGO_HOME` in `~/.cache/lectrix-toolbox`.
- **Platform services:** the default author comes from `USER`. WebView2's memory target
  has no equivalent.
- **Packages** for macOS (dmg, ad-hoc signed, not notarized) and Linux (AppImage and deb,
  plus rpm for final versions; built on Ubuntu 24.04, so glibc 2.39 or later) are built and released (ADR 0011)
  but have not been checked as the Windows installers are (the manual checklist covers
  only the Windows ones). Their PDF association (2026-10-18, after users' reports that
  Ubuntu did not offer Lectrix for PDFs and that macOS opened nothing from Finder) is
  untested by hand: the deb, rpm and AppImage desktop file now declares
  `MimeType=application/pdf;` and takes files (`Exec=lectrix %F`,
  `src-tauri/linux/lectrix.desktop`), the metainfo names the media type, and the macOS
  bundle declares PDFs (`CFBundleDocumentTypes`, rank Alternate, `tauri.macos.conf.json`)
  and opens the files macOS hands over (`RunEvent::Opened`, which only compiles on macOS,
  so CI's macOS job is its only build check).
  Crash recovery, the single-instance hand-off and the in-app updater are
  platform-neutral but untested there.
- **macOS versions** (read from the 1.0.0 bundles, 2026-10-12): both declare macOS 10.13
  as their minimum (`LSMinimumSystemVersion`, Tauri's default; the Apple Silicon binary
  itself asks for 11.0), which nothing backs. The Apple Silicon build is made on macOS 26
  (`macos-latest`), where CI also runs the tests; the Intel build is made on macOS 15
  (`macos-15-intel`) and no test runs on Intel. The app has not been started by hand on
  either, and no older macOS has been tried: the README names 26 and 15. Tailwind 4's CSS
  needs Safari 16.4's WebKit, which macOS has from 13.3, so the interface is unlikely to
  draw properly before that; `bundle.macOS.minimumSystemVersion` is not set to say so.
- **Linux software stores** (2026-10-12, not yet seen in a store): the 1.0.0 rpm, installed
  on Fedora, showed no icon in GNOME Software. The package had the desktop file and the
  icons (32, 128 and 256@2 px in hicolor) but no AppStream metainfo, and GNOME Software
  lists installed apps from metainfo alone (`component/description/..` in its
  `gs_appstream_add_installed`), so it showed the bare package. The deb and the rpm now
  install `org.lectrix.pdf.metainfo.xml` (`appstreamcli validate-tree` passes on the
  1.0.0 deb's files with it added; its only remark, a pedantic one, is the missing
  `<releases>`). The first release that carries it is the first real test. It has no
  screenshots, so a store's page shows none. The AppImage does not carry it.
- **License notices:** `THIRD_PARTY_LICENSES.md` lists the crates of the Windows build
  only; the macOS and Linux builds include more (GTK and WebKitGTK bindings, objc2 and
  others), which it does not name.
- **Tests:** some are Windows-only by design (DirectWrite fonts, locked files). Every
  test program runs (`--no-fail-fast`), and qpdf is installed on both, so the structural
  check runs there too.
