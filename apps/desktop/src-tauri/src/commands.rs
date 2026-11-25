//! Tauri command adapters. Each command validates input, delegates to the
//! core and maps errors to `IpcError`; no business logic lives here.

use parlance_domain::{SessionState, SourceDescriptor};
use parlance_models::find;
use parlance_protocol::{
    AppInfo, AppSettings, CaptureSupport, ModelDescriptor, ModelProgress, SessionEvent, StartSessionRequest,
};
use std::sync::Arc;

use parlance_protocol::{ErrorCode, IpcError};
use parlance_session::{EventSink, SegmentListener, SessionConfig};
use parlance_storage::{HistoryEntry, HistoryLine, HistoryRecorder, HistoryStore};
use tauri::State;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

use crate::error::{CommandResult, from_capture, from_download, from_session, invalid};
use crate::state::AppState;

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let capabilities = state.capture.capabilities();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        capture_backend: state.capture.name().into(),
        capture: CaptureSupport {
            system_output: capabilities.system_output,
            application: capabilities.application,
            microphone: capabilities.microphone,
            browser_tab: capabilities.browser_tab,
        },
        speech_runtime: parlance_asr::system_info().trim().into(),
        models_dir: state.store.root().display().to_string(),
    }
}

/// Device enumeration talks to COM/PipeWire, so it runs off the async runtime.
#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>) -> CommandResult<Vec<SourceDescriptor>> {
    let capture = std::sync::Arc::clone(&state.capture);
    tokio::task::spawn_blocking(move || capture.list_sources())
        .await
        .map_err(|e| invalid(e.to_string()))?
        .map_err(from_capture)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.get()
}

#[tauri::command]
pub async fn update_settings(state: State<'_, AppState>, settings: AppSettings) -> CommandResult<AppSettings> {
    for model in [&settings.speech_model, &settings.language_model] {
        if find(model).is_none() {
            return Err(invalid(format!("unknown model `{model}`")));
        }
    }
    state.settings.replace(settings).map_err(|e| invalid(e.to_string()))?;
    let current = state.settings.get();
    state.intelligence.reconcile(&current);
    Ok(current)
}

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Vec<ModelDescriptor> {
    state.store.descriptors(|id| state.is_downloading(id))
}

#[tauri::command]
pub async fn download_model(
    state: State<'_, AppState>,
    id: String,
    on_progress: Channel<ModelProgress>,
) -> CommandResult<()> {
    let entry = find(&id).ok_or_else(|| invalid(format!("unknown model `{id}`")))?;
    let cancel = CancellationToken::new();
    {
        let mut downloads = state.downloads.lock().map_err(|e| invalid(e.to_string()))?;
        if downloads.contains_key(&id) {
            return Err(invalid(format!("`{id}` is already downloading")));
        }
        downloads.insert(id.clone(), cancel.clone());
    }

    let report = |downloaded_bytes, done, error: Option<String>| {
        let _ = on_progress.send(ModelProgress {
            id: id.clone(),
            downloaded_bytes,
            total_bytes: entry.size_bytes,
            done,
            error,
        });
    };
    let result =
        parlance_models::download(&state.http, &state.store, entry, cancel, |done, _| report(done, false, None)).await;
    if let Ok(mut downloads) = state.downloads.lock() {
        downloads.remove(&id);
    }
    match result {
        Ok(_) => {
            report(entry.size_bytes, true, None);
            state.intelligence.reconcile(&state.settings.get());
            Ok(())
        }
        Err(error) => {
            let error = from_download(error);
            report(0, true, Some(error.message.clone()));
            Err(error)
        }
    }
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, id: String) {
    if let Ok(downloads) = state.downloads.lock()
        && let Some(token) = downloads.get(&id)
    {
        token.cancel();
    }
}

/// Registers the webview's event channel and replays the current state so a
/// reloaded window is immediately consistent.
#[tauri::command]
pub fn subscribe(state: State<'_, AppState>, on_event: Channel<SessionEvent>) {
    state.events.subscribe(on_event);
    state.events.emit(SessionEvent::StateChanged { state: state.controller.state() });
}

#[tauri::command]
pub fn session_state(state: State<'_, AppState>) -> SessionState {
    state.controller.state()
}

#[tauri::command]
pub async fn start_session(state: State<'_, AppState>, request: StartSessionRequest) -> CommandResult<()> {
    let _request = state.session_requests.lock().await;
    if state.closing.load(std::sync::atomic::Ordering::Acquire) {
        return Err(invalid("Parlance is closing"));
    }
    let settings = state.settings.get();
    let mut sources = vec![request.source_id];
    sources.extend(request.microphone_id);
    let recorder = settings
        .history_enabled
        .then(|| Arc::new(HistoryRecorder::start(state.history_db.clone(), settings.target_language.to_string())));
    if let Some(completion) = recorder.as_ref().and_then(|r| r.take_completion()) {
        let mut writers = state.history_writers.lock().unwrap_or_else(|p| p.into_inner());
        writers.retain(|writer| !writer.is_finished());
        writers.push(completion);
    }
    let events = record_translations(state.events.sink(), recorder.clone());
    let mut listeners = state.intelligence.listeners(&settings, &events);
    listeners.extend(recorder.map(|r| r as Arc<dyn SegmentListener>));
    let config = SessionConfig::new(sources, settings.source_language.clone());
    state.controller.start(config, listeners).await.map_err(from_session)
}

/// Tees translation events into the history recorder when history is on.
fn record_translations(sink: EventSink, recorder: Option<Arc<HistoryRecorder>>) -> EventSink {
    let Some(recorder) = recorder else { return sink };
    Arc::new(move |event| {
        if let SessionEvent::Translation { translation } = &event {
            recorder.record_translation(translation);
        }
        sink(event);
    })
}

fn history(state: &AppState) -> CommandResult<HistoryStore> {
    HistoryStore::open(&state.history_db).map_err(|e| IpcError::new(ErrorCode::Storage, e.to_string()))
}

#[tauri::command]
pub fn list_history(state: State<'_, AppState>) -> CommandResult<Vec<HistoryEntry>> {
    history(&state)?.list(50).map_err(|e| IpcError::new(ErrorCode::Storage, e.to_string()))
}

#[tauri::command]
pub fn history_transcript(state: State<'_, AppState>, id: i64) -> CommandResult<Vec<HistoryLine>> {
    history(&state)?.transcript(id).map_err(|e| IpcError::new(ErrorCode::Storage, e.to_string()))
}

#[tauri::command]
pub fn delete_history(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    history(&state)?.delete(id).map(drop).map_err(|e| IpcError::new(ErrorCode::Storage, e.to_string()))
}

#[tauri::command]
pub async fn stop_session(state: State<'_, AppState>) -> CommandResult<()> {
    let _request = state.session_requests.lock().await;
    state.controller.stop().await.map_err(from_session)
}

#[tauri::command]
pub fn pause_session(state: State<'_, AppState>) -> CommandResult<()> {
    state.controller.pause().map_err(from_session)
}

#[tauri::command]
pub fn resume_session(state: State<'_, AppState>) -> CommandResult<()> {
    state.controller.resume().map_err(from_session)
}
