/// Every app command must be listed here; Tauri then generates an
/// `allow-<command>` permission per command, and only those granted in
/// `capabilities/` are callable from the webview.
const COMMANDS: &[&str] = &[
    "app_info",
    "list_sources",
    "get_settings",
    "update_settings",
    "list_models",
    "download_model",
    "cancel_download",
    "subscribe",
    "session_state",
    "start_session",
    "stop_session",
    "pause_session",
    "resume_session",
    "list_history",
    "history_transcript",
    "delete_history",
    "list_project_profiles",
    "save_project_profile",
    "delete_project_profile",
    "set_active_project_profile",
    "ask_copilot",
    "cancel_copilot",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
