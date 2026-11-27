//! Composition root: wires platform adapters and inference engines into the
//! session core, and exposes them to the webview through typed commands.

mod commands;
mod engine;
mod error;
mod events;
mod intelligence;
mod settings;
mod state;

use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// Let an already-starting/stopping pipeline settle before requesting shutdown.
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
/// Final translations may still own the history recorder after ASR has stopped.
const HISTORY_DRAIN_TIMEOUT: Duration = Duration::from_secs(15);
/// A stuck native device or sidecar must not prevent macOS from terminating the app.
const EXIT_TIMEOUT: Duration = Duration::from_secs(30);

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,parlance=debug,whisper_rs=warn")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let state = tauri::async_runtime::block_on(async {
                let state = state::AppState::initialize()?;
                state.intelligence.reconcile(&state.settings.get());
                Ok::<_, Box<dyn std::error::Error>>(state)
            })?;
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
            commands::list_project_profiles,
            commands::save_project_profile,
            commands::delete_project_profile,
            commands::set_active_project_profile,
            commands::ask_copilot,
            commands::cancel_copilot,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Parlance")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<state::AppState>();
                if state.shutdown_complete.load(Ordering::Acquire) {
                    return;
                }
                api.prevent_exit();
                if !state.closing.swap(true, Ordering::AcqRel) {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        finish_shutdown(&app).await;
                        app.exit(0);
                    });
                }
            } else if let tauri::RunEvent::Exit = event {
                // macOS Cmd+Q/Dock Quit can bypass ExitRequested via NSApp terminate:.
                if !app.state::<state::AppState>().shutdown_complete.load(Ordering::Acquire) {
                    tauri::async_runtime::block_on(finish_shutdown(app));
                }
            }
        });
}

async fn finish_shutdown(app: &tauri::AppHandle) {
    let state = app.state::<state::AppState>();
    state.closing.store(true, Ordering::Release);
    if tokio::time::timeout(EXIT_TIMEOUT, shutdown(app)).await.is_err() {
        tracing::error!("shutdown deadline reached; some pending history may not have finished");
    }
    state.shutdown_complete.store(true, Ordering::Release);
}

async fn shutdown(app: &tauri::AppHandle) {
    use parlance_domain::SessionState;

    let state = app.state::<state::AppState>();
    for download in state.downloads.lock().unwrap_or_else(|p| p.into_inner()).values() {
        download.cancel();
    }
    let _request = state.session_requests.lock().await;
    if state.shutdown_complete.load(Ordering::Acquire) {
        return;
    }
    loop {
        match state.controller.state() {
            SessionState::Starting | SessionState::Stopping => tokio::time::sleep(EXIT_POLL_INTERVAL).await,
            SessionState::Running | SessionState::Paused => {
                if let Err(error) = state.controller.stop().await {
                    tracing::warn!(%error, "session stop during exit");
                }
            }
            SessionState::Idle | SessionState::Failed { .. } => break,
        }
    }
    let writers = std::mem::take(&mut *state.history_writers.lock().unwrap_or_else(|p| p.into_inner()));
    let mut drained = tokio::task::spawn_blocking(move || {
        for writer in writers {
            if writer.join().is_err() {
                tracing::error!("history writer panicked during exit");
            }
        }
    });
    let pending = tokio::time::timeout(HISTORY_DRAIN_TIMEOUT, &mut drained).await.is_err();
    if pending {
        tracing::warn!("final translations exceeded the history drain deadline");
    }
    state.intelligence.shutdown().await;
    if pending {
        let _ = drained.await;
    }
}
