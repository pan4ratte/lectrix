//! Linux: WebKitGTK in a GTK 3 window.

use std::cell::Cell;
use std::ffi::{CStr, c_char, c_void};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::gdk;
use gtk::glib;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use tauri::Emitter;
use webkit2gtk::{LoadEvent, WebViewExt};

use super::Platform;
use crate::ipc::{TouchpadPinch, WindowButton, WindowChrome};

/// Event carrying `TouchpadPinch`.
const TOUCHPAD_PINCH: &str = "touchpad-pinch";

/// How long after a load or a size change the window still paints its background: until
/// the webview has drawn a frame of the page at the window's size.
const COVER_DELAY: Duration = Duration::from_millis(500);

pub struct Linux;

impl Platform for Linux {
    /// The file manager's ShowItems (the freedesktop FileManager1 interface, which Nautilus,
    /// Dolphin, Nemo and others offer over D-Bus) opens the folder with the file selected;
    /// without it, the folder alone opens.
    fn show_in_folder(&self, file: &std::path::Path) -> std::io::Result<()> {
        let shown = std::process::Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!(
                "array:string:{}",
                super::file_uri(std::os::unix::ffi::OsStrExt::as_bytes(file.as_os_str()))
            ))
            .arg("string:")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if shown {
            return Ok(());
        }
        match file.parent() {
            Some(folder) => self.open_folder(folder),
            None => Err(std::io::Error::other("the file has no folder")),
        }
    }

    /// The buttons and the side the desktop puts them on (GNOME: close alone, at the
    /// right), as GTK's own title bars have them. Windows' three at the right where GTK
    /// has no layout to give.
    fn window_chrome(&self) -> WindowChrome {
        gtk::Settings::default()
            .and_then(|s| {
                // `property` panics on a name the object lacks or a type it does not hold.
                const LAYOUT: &str = "gtk-decoration-layout";
                s.has_property(LAYOUT, Some(String::static_type()))
                    .then(|| s.property::<Option<String>>(LAYOUT))
                    .flatten()
            })
            .and_then(|layout| super::parse_decoration_layout(&layout))
            .unwrap_or_else(|| WindowChrome {
                left: Vec::new(),
                right: vec![
                    WindowButton::Minimize,
                    WindowButton::Maximize,
                    WindowButton::Close,
                ],
            })
    }

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
            render_at_display_rate(&webview);
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

/// WebKit holds page updates near 60 fps on faster displays
/// (`PreferPageRenderingUpdatesNear60FPS`, on by default), which kept scrolling at 60 fps
/// on a 120 Hz screen (docs/status.md). Turning it off lets frames follow the display.
///
/// The feature API arrived in WebKitGTK 2.42, after the `webkit2gtk` crate's bindings, so
/// its functions are looked up at run time; with an older WebKitGTK nothing changes.
fn render_at_display_rate(webview: &webkit2gtk::WebView) {
    const FEATURE: &CStr = c"PreferPageRenderingUpdatesNear60FPS";
    let Some(api) = FeatureApi::load() else {
        crate::applog::info("WebKitGTK has no feature API: pages update near 60 fps");
        return;
    };
    let Some(settings) = WebViewExt::settings(webview) else {
        return;
    };
    let settings: *mut webkit2gtk::ffi::WebKitSettings = settings.to_glib_none().0;
    // SAFETY: the functions have the signatures declared in `FeatureApi` (webkit/WebKitFeature.h,
    // webkit/WebKitSettings.h). The list is owned here and unreffed once; the features it
    // holds and their identifiers stay valid while it lives, and `settings` is a live
    // WebKitSettings borrowed from the webview for the duration of the call.
    unsafe {
        let list = (api.get_all_features)();
        if list.is_null() {
            return;
        }
        for i in 0..(api.list_get_length)(list) {
            let feature = (api.list_get)(list, i);
            if feature.is_null() {
                continue;
            }
            let id = (api.get_identifier)(feature);
            if !id.is_null() && CStr::from_ptr(id) == FEATURE {
                (api.set_feature_enabled)(settings.cast(), feature, glib::ffi::GFALSE);
            }
        }
        (api.list_unref)(list);
    }
}

type GetAllFeatures = unsafe extern "C" fn() -> *mut c_void;
type ListGetLength = unsafe extern "C" fn(*mut c_void) -> usize;
type ListGet = unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void;
type ListUnref = unsafe extern "C" fn(*mut c_void);
type GetIdentifier = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type SetFeatureEnabled = unsafe extern "C" fn(*mut c_void, *mut c_void, glib::ffi::gboolean);

/// WebKitGTK 2.42's feature functions, found in the loaded library.
struct FeatureApi {
    get_all_features: GetAllFeatures,
    list_get_length: ListGetLength,
    list_get: ListGet,
    list_unref: ListUnref,
    get_identifier: GetIdentifier,
    set_feature_enabled: SetFeatureEnabled,
}

impl FeatureApi {
    fn load() -> Option<Self> {
        fn symbol(name: &CStr) -> Option<*mut c_void> {
            // SAFETY: dlsym with RTLD_DEFAULT only looks a name up in the libraries already
            // loaded (WebKitGTK is, through wry); `name` is NUL-terminated.
            let ptr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
            (!ptr.is_null()).then_some(ptr)
        }
        use std::mem::transmute;
        // SAFETY: each pointer is the named WebKitGTK function, whose C signature the
        // type it is transmuted to declares (webkit/WebKitFeature.h, WebKitSettings.h).
        unsafe {
            Some(Self {
                get_all_features: transmute::<*mut c_void, GetAllFeatures>(symbol(
                    c"webkit_settings_get_all_features",
                )?),
                list_get_length: transmute::<*mut c_void, ListGetLength>(symbol(
                    c"webkit_feature_list_get_length",
                )?),
                list_get: transmute::<*mut c_void, ListGet>(symbol(c"webkit_feature_list_get")?),
                list_unref: transmute::<*mut c_void, ListUnref>(symbol(
                    c"webkit_feature_list_unref",
                )?),
                get_identifier: transmute::<*mut c_void, GetIdentifier>(symbol(
                    c"webkit_feature_get_identifier",
                )?),
                set_feature_enabled: transmute::<*mut c_void, SetFeatureEnabled>(symbol(
                    c"webkit_settings_set_feature_enabled",
                )?),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn webkit_feature_api_is_found() {
        // The test binary links WebKitGTK through wry, as the app does.
        let _ = <webkit2gtk::WebView as gtk::glib::StaticType>::static_type;
        assert!(super::FeatureApi::load().is_some());
    }
}
