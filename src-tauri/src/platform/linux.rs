//! Linux: WebKitGTK in a GTK 3 window.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use tauri::Emitter;
use webkit2gtk::{LoadEvent, WebViewExt};

use super::Platform;
use crate::ipc::TouchpadPinch;

/// Event carrying `TouchpadPinch`.
const TOUCHPAD_PINCH: &str = "touchpad-pinch";

/// How long after a load or a size change the window still paints its background: until
/// the webview has drawn a frame of the page at the window's size.
const COVER_DELAY: Duration = Duration::from_millis(500);

pub struct Linux;

impl Platform for Linux {
    /// WebKitGTK turns a touchpad pinch into page magnification, which scales the whole
    /// app, and gives the page no event for it. Its zoom gesture is a bubble-phase GTK
    /// gesture on the webview, and GTK 3 emits the widget's `event` signal for a touchpad
    /// pinch before running those; a handler that stops the event there keeps it from the
    /// gesture and passes it to the frontend, which zooms the document instead.
    fn route_touchpad_pinch(&self, window: &tauri::WebviewWindow) {
        let target = window.clone();
        let hooked = window.with_webview(move |webview| {
            webview.inner().connect_event(move |_, event| {
                let Some(pinch) = event.downcast_ref::<gdk::EventTouchpadPinch>() else {
                    return glib::Propagation::Proceed;
                };
                // gdk 0.18 reads the phase as a bool (`is_phase`), losing Update from End.
                let phase =
                    i32::from(AsRef::<gdk::ffi::GdkEventTouchpadPinch>::as_ref(pinch).phase);
                let begin = match phase {
                    gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_BEGIN => true,
                    gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_UPDATE => false,
                    // The end changes nothing: the viewer settles once the events stop.
                    _ => return glib::Propagation::Stop,
                };
                // Relative to the webview's own window; at the webview's zoom of 1 these
                // are the page's CSS pixels.
                let (x, y) = pinch.position();
                let event = TouchpadPinch {
                    begin,
                    scale: pinch.scale(),
                    x,
                    y,
                };
                if let Err(e) = target.emit_to(target.label(), TOUCHPAD_PINCH, event) {
                    crate::applog::warn(format!("could not pass on a touchpad pinch: {e}"));
                }
                glib::Propagation::Stop
            });
        });
        if let Err(e) = hooked {
            crate::applog::warn(format!("touchpad pinch zoom is off: {e}"));
        }
    }

    /// GTK 3 paints a window's frame into a new window-sized image (in memory, not on the
    /// GPU) and fills the window's background colour there before WebKit draws its frame
    /// over it through GL. On a 3840 × 2400 screen at 133%, which GTK draws at 200%, that
    /// fills 83 MB of fresh memory per frame and kept scrolling near 18 frames a second
    /// (docs/status.md).
    /// So the window's own draw handler draws only the children and stops GTK's, except
    /// while the webview may not cover the window: until its page has loaded, and during a
    /// resize, where WebKit draws its last frame at the old size. GTK still takes the
    /// colour for the window's opaque region, so the compositor needs no blending for it.
    fn skip_covered_window_background(&self, window: &tauri::WebviewWindow) {
        let hooked = window.with_webview(|webview| {
            let webview = webview.inner();
            let Some(gtk_window) = webview.toplevel().and_downcast::<gtk::Window>() else {
                return;
            };
            // Painting the background is needed until then; None while a page loads.
            let cover_until: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
            if !webview.is_loading() {
                cover_until.set(Some(Instant::now() + COVER_DELAY));
            }
            let until = cover_until.clone();
            webview.connect_load_changed(move |_, event| {
                until.set(match event {
                    LoadEvent::Finished => Some(Instant::now() + COVER_DELAY),
                    _ => None,
                });
            });
            let size = Cell::new((0, 0));
            let until = cover_until.clone();
            gtk_window.connect_size_allocate(move |_, allocation| {
                let new = (allocation.width(), allocation.height());
                if size.replace(new) != new && until.get().is_some() {
                    until.set(Some(Instant::now() + COVER_DELAY));
                }
            });
            gtk_window.connect_draw(move |window, cr| {
                if cover_until.get().is_none_or(|t| Instant::now() < t) {
                    return glib::Propagation::Proceed;
                }
                window.forall(|child| window.propagate_draw(child, cr));
                glib::Propagation::Stop
            });
        });
        if let Err(e) = hooked {
            crate::applog::warn(format!(
                "the window paints its background on each frame: {e}"
            ));
        }
    }
}
