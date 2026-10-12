# WebKitGTK: comment for bug 199376 ("[GTK][WPE] View is locked to 60 FPS")

To post at https://bugs.webkit.org/show_bug.cgi?id=199376, after the GTK issue
(`gtk3-wayland-gl-frame-clock.md`) is filed, with its link in place of `<GTK issue>`.

---

Two things hold WebKitGTK to 60 fps on a 120 Hz display for the GTK 3 port
(webkit2gtk-4.1), as measured with WebKitGTK 2.54.1, GTK 3.24.52, GNOME 50.5 on Wayland,
Mesa 26.2.3 (AMD), 2560x1600 at 120 Hz:

1. **PreferPageRenderingUpdatesNear60FPS** is on by default. The DRM vblank monitor
   works here (its thread wakes 120 times a second), yet requestAnimationFrame fires
   every 16.7 ms until the feature is turned off with
   `webkit_settings_set_feature_enabled()`.
2. **GTK 3 itself.** A GTK 3 window drawn with GL never requests Wayland frame callbacks,
   so its frame clock runs at a fixed 60 Hz (`FRAME_INTERVAL` in gdkframeclockidle.c):
   <GTK issue>. A bare `GtkGLArea` shows it (59 fps, against 119 for the same window
   drawn with cairo). WebKit's frames reach the screen through `gdk_cairo_draw_from_gl`
   in that frame clock.

A bare web view (a GTK 3 window with only a WebKitWebView, an rAF loop moving a div)
measured:

| | rAF interval, mean |
| --- | --- |
| Default | 16.9 ms |
| Feature off | 16.8 ms |
| `GDK_GL=disable` | 16.6 ms |
| `GDK_GL=disable` and feature off | 9.0 ms (8 ms median) |

`GDK_GL=disable` is no workaround: WebKit's frames are then copied through the CPU, which
at 3840x2400 is far slower. The GTK 4 port was not tried.
