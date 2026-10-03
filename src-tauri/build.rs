/// Every IPC command the app defines. Declaring them makes each one require an explicit
/// permission in capabilities/, so the webview can call only what it is granted.
const COMMANDS: &[&str] = &[
    "open_with_dialog",
    "open_startup_documents",
    "open_recent",
    "remove_recent",
    "list_recent_files",
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
    "app_ready",
    "log_metric",
    "log_error",
];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    if let Err(e) = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)) {
        panic!("tauri-build failed: {e:#}");
    }
}
