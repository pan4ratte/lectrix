# GTK 3: Wayland windows drawn with GL run their frame clock at a fixed 60 Hz

To file at https://gitlab.gnome.org/GNOME/gtk/-/issues (labels: GTK3, Wayland, 1. Performance).

---

**Title:** GTK 3, Wayland: a window drawn with GL never requests frame callbacks, so its frame clock runs at 60 Hz on faster displays

On Wayland, a GTK 3 window whose frame is drawn with GL (a `GtkGLArea`, or a WebKitGTK
web view, which paints through `gdk_cairo_draw_from_gl`) updates at 60 frames a second
on a 120 Hz display. The same window drawn with cairo follows the display.

**Steps to reproduce.** Run this with `python3 gl.py gl` and with `python3 gl.py cairo` on
a display faster than 60 Hz:

```python
import sys, ctypes, gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk
GL = sys.argv[1:] == ['gl']
w = Gtk.Window(); w.set_default_size(800, 600)
if GL:
    libgl = ctypes.CDLL('libGL.so.1'); libgl.glClearColor.argtypes = [ctypes.c_float] * 4
    area = Gtk.GLArea()
    area.connect('render', lambda a, c: (libgl.glClearColor(n[0] % 60 / 60, 0, 0, 1), libgl.glClear(0x4000), True)[-1])
else:
    area = Gtk.DrawingArea()
    area.connect('draw', lambda a, cr: (cr.set_source_rgb(n[0] % 60 / 60, 0, 0), cr.paint()))
w.add(area)
n, start = [0], [None]
def tick(widget, clock):
    t = clock.get_frame_time(); start[0] = start[0] or t; n[0] += 1
    area.queue_render() if GL else area.queue_draw()
    if t - start[0] > 3_000_000:
        print('frames/s:', round(n[0] / ((t - start[0]) / 1e6), 1)); Gtk.main_quit()
    return True
w.add_tick_callback(tick); w.show_all(); Gtk.main()
```

**Result** (2560x1600 at 120 Hz, scale 133%):

- `gl`: 59 frames/s. Each paint takes about 2 ms, then the next frame starts about 15 ms
  later.
- `cairo`: 113 to 119 frames/s. The next frame starts about 7.4 ms later.

**What happens.** With `WAYLAND_DEBUG=1`, the cairo window sends `wl_surface.frame` and
`commit` for every frame, and `wl_callback.done` comes about 6.4 ms after each commit. The
GL window sends no `wl_surface.frame` at all: only Mesa's `attach`, `damage` and
`commit` from `eglSwapBuffers`, then a `wl_display.sync`.

In gtk-3-24:

- `gdk_window_impl_wayland_end_paint()` (gdk/wayland/gdkwindow-wayland.c) clears
  `pending_commit` when the paint used GL, since `eglSwapBuffers()` commits.
- `on_frame_clock_after_paint()` returns at once when `pending_commit` is false, so no
  frame callback is requested, the frame clock is not frozen until the compositor has
  shown the frame, and no presentation timings reach it.
- `GdkFrameClockIdle` then schedules frames at `FRAME_INTERVAL` (16667 µs,
  gdk/gdkframeclockidle.c), because no frame timings carry a `refresh_interval`.

So every GL-drawn GTK 3 window on Wayland is held to 60 frames a second, whatever the
display's rate. That includes all WebKitGTK apps built on GTK 3 (webkit2gtk-4.1: Tauri and
Wails apps, among others): WebKit's own vblank monitor follows the display at 120 Hz
there, but its frames reach the screen through this frame clock.

A possible fix: in the GL path, request a `wl_surface.frame` callback before
`eglSwapBuffers()` (whose commit then carries it) and freeze the frame clock until it is
done, as the SHM path does.

**Versions:** GTK 3.24.52, Fedora 44, GNOME Shell and Mutter 50.5, Mesa 26.2.3
(radeonsi, AMD "HawkPoint1" integrated GPU), Wayland.
