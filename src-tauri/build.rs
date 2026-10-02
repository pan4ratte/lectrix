fn main() {
    // Declaring the app's commands makes each one require an explicit permission in
    // capabilities/, so the webview can call only what it is granted.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "open_with_dialog",
        "open_startup_document",
        "close_document",
        "app_ready",
        "log_metric",
    ]);
    if let Err(e) = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)) {
        panic!("tauri-build failed: {e:#}");
    }
}
