//! Composition root: wires platform adapters and inference engines into the
//! session core, and exposes them to the webview through typed commands.

mod commands;
mod engine;
mod error;
mod events;
mod intelligence;
mod settings;
mod state;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,parlance=debug")))
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let state = state::AppState::initialize()?;
            let settings = state.settings.get();
            tauri::async_runtime::block_on(async { state.intelligence.reconcile(&settings) });
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_sources,
            commands::get_settings,
            commands::update_settings,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::subscribe,
            commands::session_state,
            commands::start_session,
            commands::stop_session,
            commands::pause_session,
            commands::resume_session,
            commands::list_history,
            commands::history_transcript,
            commands::delete_history,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Parlance")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<state::AppState>().intelligence.shutdown();
            }
        });
}
