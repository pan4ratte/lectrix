/// Every IPC command the app defines. Declaring them makes each one require an explicit
/// permission in capabilities/, so the webview can call only what it is granted.
const COMMANDS: &[&str] = &[
    "open_with_dialog",
    "open_startup_documents",
    "open_recent",
    "remove_recent",
    "set_recent_pinned",
    "show_recent_in_folder",
    "list_recent_files",
    "list_recovered",
    "restore_recovered",
    "discard_recovered",
    "exit_confirmed",
    "unlock_document",
    "cancel_unlock",
    "unprotect_pick",
    "unprotect_password",
    "unprotect_save",
    "unprotect_cancel",
    "unprotect_document",
    "compress_pick",
    "compress_password",
    "compress_document",
    "compress_run",
    "compress_stop",
    "compress_save",
    "compress_cancel",
    "open_compare_source",
    "open_compare_source_of",
    "compare_settings",
    "compare_run",
    "compare_stop",
    "compare_forget",
    "compare_save_annotated",
    "compare_save_report",
    "print_start",
    "print_printer",
    "print_printer_settings",
    "print_layout",
    "print_run",
    "print_stop",
    "protect_pick",
    "protect_password",
    "protect_save",
    "protect_cancel",
    "set_document_protection",
    "get_document_protection",
    "get_document_properties",
    "get_document_fonts",
    "close_document",
    "reopen_closed_document",
    "get_document_info",
    "list_open_documents",
    "get_page_text",
    "get_page_presentation",
    "open_audience_screen",
    "swap_presentation_screens",
    "close_audience_screen",
    "search_text",
    "apply_operation",
    "open_merge_sources",
    "open_insert_sources",
    "set_drop_target",
    "plan_merge",
    "execute_merge",
    "extract_pages",
    "cancel_merge",
    "set_bookmark_open",
    "enable_editing",
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
    "reset_settings",
    "app_ready",
    "set_pane_layout",
    "log_metric",
    "log_error",
    "open_web_page",
    "window_chrome",
    "set_menu",
    "open_link",
    "move_document",
    "list_windows",
    "close_other_windows",
    "check_for_update",
    "skip_update",
    "download_update",
    "cancel_update_download",
    "restart_to_update",
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
