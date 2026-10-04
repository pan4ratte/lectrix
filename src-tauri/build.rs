/// Every IPC command the app defines. Declaring them makes each one require an explicit
/// permission in capabilities/, so the webview can call only what it is granted.
const COMMANDS: &[&str] = &[
    "open_with_dialog",
    "open_startup_documents",
    "open_recent",
    "remove_recent",
    "list_recent_files",
    "list_recovered",
    "restore_recovered",
    "discard_recovered",
    "exit_confirmed",
    "unlock_document",
    "cancel_unlock",
    "close_document",
    "get_document_info",
    "list_open_documents",
    "get_page_text",
    "search_text",
    "apply_operation",
    "open_merge_sources",
    "open_insert_source",
    "set_drop_target",
    "plan_merge",
    "execute_merge",
    "cancel_merge",
    "set_bookmark_open",
    "undo",
    "redo",
    "save",
    "save_as",
    "reload_document",
    "remember_view",
    "scan_annotations_for_repair",
    "repair_annotations",
    "get_settings",
    "set_settings",
    "app_ready",
    "log_metric",
    "log_error",
];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    let mut attributes = tauri_build::Attributes::new().app_manifest(manifest);
    // tauri-build's Windows manifest holds only a Common Controls v6 dependency, and it is
    // embedded in the app binary alone. Test binaries that link the dialog plugin import
    // TaskDialogIndirect, which only Common Controls v6 has, and fail to start without it
    // (STATUS_ENTRYPOINT_NOT_FOUND). So the linker writes the same dependency into every
    // binary of this crate instead (two manifests in the app binary would clash).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {e:#}");
    }
}
