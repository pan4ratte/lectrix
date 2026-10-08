//! Linux: WebKitGTK in a GTK 3 window.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use tauri::Emitter;

use super::Platform;
use crate::ipc::TouchpadPinch;

/// Event carrying `TouchpadPinch`.
const TOUCHPAD_PINCH: &str = "touchpad-pinch";

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
                if let Err(e) = target.emit(TOUCHPAD_PINCH, event) {
                    crate::applog::warn(format!("could not pass on a touchpad pinch: {e}"));
                }
                glib::Propagation::Stop
            });
        });
        if let Err(e) = hooked {
            crate::applog::warn(format!("touchpad pinch zoom is off: {e}"));
        }
    }
}
