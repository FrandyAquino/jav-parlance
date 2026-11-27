use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use std::path::PathBuf;

use parlance_capture::bridge::BridgeBackend;
use parlance_capture::{CaptureBackend, CompositeBackend, platform_backend};
use parlance_models::ModelStore;
use parlance_session::SessionController;
use parlance_storage::{JsonProfileRepository, ProfileRepository};
use tokio_util::sync::CancellationToken;

use crate::engine::LocalSpeechEngine;
use crate::events::EventHub;
use crate::intelligence::Intelligence;
use crate::settings::SettingsRepository;

pub struct AppState {
    pub capture: Arc<dyn CaptureBackend>,
    pub controller: SessionController,
    pub events: Arc<EventHub>,
    pub settings: Arc<SettingsRepository>,
    pub profiles: Result<Arc<JsonProfileRepository>, String>,
    pub store: ModelStore,
    pub http: reqwest::Client,
    pub downloads: Mutex<HashMap<String, CancellationToken>>,
    pub intelligence: Intelligence,
    pub history_db: PathBuf,
    pub history_writers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    pub closing: AtomicBool,
    pub shutdown_complete: AtomicBool,
    pub session_requests: tokio::sync::Mutex<()>,
    pub intelligence_requests: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn initialize() -> Result<Self, Box<dyn std::error::Error>> {
        let dirs = directories::BaseDirs::new().ok_or("no home directory")?;
        let settings = Arc::new(SettingsRepository::load(dirs.config_dir().join("parlance").join("settings.json")));
        let profiles = JsonProfileRepository::open(dirs.config_dir().join("parlance").join("profiles.json"))
            .map(Arc::new)
            .map_err(|error| error.to_string());
        let store = ModelStore::default_location()?;
        let data_dir = dirs.data_local_dir().join("parlance");
        let capture = capture_backends(&data_dir);
        let events = Arc::new(EventHub::default());
        let engine = Arc::new(LocalSpeechEngine::new(store.clone(), Arc::clone(&settings)));
        let controller = SessionController::new(Arc::clone(&capture), engine, events.sink());
        let http = reqwest::Client::builder().user_agent(concat!("parlance/", env!("CARGO_PKG_VERSION"))).build()?;

        let intelligence = Intelligence::new(store.clone(), events.sink(), &settings.get());
        let history_db = data_dir.join("history.db");
        Ok(Self {
            capture,
            controller,
            events,
            settings,
            profiles,
            store,
            http,
            downloads: Mutex::new(HashMap::new()),
            intelligence,
            history_db,
            history_writers: Mutex::new(Vec::new()),
            closing: AtomicBool::new(false),
            shutdown_complete: AtomicBool::new(false),
            session_requests: tokio::sync::Mutex::new(()),
            intelligence_requests: tokio::sync::Mutex::new(()),
        })
    }

    pub fn is_downloading(&self, id: &str) -> bool {
        self.downloads.lock().is_ok_and(|downloads| downloads.contains_key(id))
    }

    pub fn active_profile_source(&self) -> Arc<dyn Fn() -> Option<parlance_domain::ProfileSnapshot> + Send + Sync> {
        let profiles = self.profiles.as_ref().ok().cloned();
        Arc::new(move || profiles.as_ref().and_then(|profiles| profiles.active_snapshot()))
    }
}

/// Native OS capture plus the browser-tab bridge. Either may be unavailable
/// (unsupported OS, port in use); the app still runs with the other.
fn capture_backends(data_dir: &std::path::Path) -> Arc<dyn CaptureBackend> {
    let mut backends: Vec<Arc<dyn CaptureBackend>> = Vec::new();
    match platform_backend() {
        Ok(native) => backends.push(native),
        Err(error) => tracing::warn!(%error, "native capture unavailable"),
    }
    match BridgeBackend::start(data_dir.join("bridge.json")) {
        Ok(bridge) => backends.push(Arc::new(bridge)),
        Err(error) => tracing::warn!(%error, "browser bridge unavailable"),
    }
    Arc::new(CompositeBackend::new(backends))
}
