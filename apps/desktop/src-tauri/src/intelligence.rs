use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use parlance_copilot::{CopilotError, CopilotPreferences, CopilotService, SuggestionPolicy};
use parlance_domain::ProfileSnapshot;
use parlance_llm::{ChatRequest, LanguageModel, LlamaServer, LlmError, OnToken, ServerConfig, ServerModel};
use parlance_models::{ModelStore, find};
use parlance_protocol::{AppSettings, AskCopilotRequest, Notice, SessionEvent};
use parlance_session::{EventSink, SegmentListener};
use parlance_translation::{LlmTranslator, TranslationWorker, Translator};
use tokio_util::sync::CancellationToken;

const RUNTIME_ID: &str = "llama-server";

struct Running {
    model_id: String,
    server: Arc<LlamaServer>,
    model: Arc<ServerModel>,
}

struct CurrentModel {
    running: Arc<Mutex<Option<Running>>>,
}

#[async_trait]
impl LanguageModel for CurrentModel {
    async fn generate(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
        on_token: &OnToken<'_>,
    ) -> Result<String, LlmError> {
        let model =
            self.running.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map(|running| Arc::clone(&running.model));
        let model =
            model.ok_or_else(|| LlmError::Unavailable("Download the language model and runtime in Settings".into()))?;
        model.generate(request, cancel, on_token).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IntelligenceError {
    #[error("Download the selected language model and runtime in Settings to use the copilot")]
    ModelMissing,
    #[error("Parlance is closing")]
    Closed,
    #[error(transparent)]
    Copilot(#[from] CopilotError),
}

/// Owns application-level requests independently of the audio pipeline.
pub struct Intelligence {
    store: ModelStore,
    running: Arc<Mutex<Option<Running>>>,
    service: Arc<CopilotService>,
    translator: Arc<dyn Translator>,
    closed: AtomicBool,
    manual_started: AtomicBool,
}

impl Intelligence {
    pub fn new(store: ModelStore, events: EventSink, settings: &AppSettings) -> Self {
        let running = Arc::new(Mutex::new(None));
        let model: Arc<dyn LanguageModel> = Arc::new(CurrentModel { running: Arc::clone(&running) });
        let translator: Arc<dyn Translator> = Arc::new(LlmTranslator::new(Arc::clone(&model)));
        let service = CopilotService::new(
            model,
            Arc::clone(&translator),
            events,
            SuggestionPolicy::default(),
            preferences(settings),
        );
        Self {
            store,
            running,
            service,
            translator,
            closed: AtomicBool::new(false),
            manual_started: AtomicBool::new(false),
        }
    }

    fn resolve(&self, model_id: &str) -> Option<(PathBuf, PathBuf)> {
        let executable = self.store.find_executable(find(RUNTIME_ID)?, RUNTIME_ID)?;
        let model = self.store.require(find(model_id)?).ok()?;
        Some((executable, model))
    }

    /// Must be called within a Tokio runtime. Disabling automatic detection leaves manual work alive.
    pub fn reconcile(&self, settings: &AppSettings) {
        self.service.update_preferences(preferences(settings));
        let wanted =
            (settings.translation_enabled || settings.copilot_enabled || self.manual_started.load(Ordering::Acquire))
                .then(|| settings.language_model.clone())
                .filter(|id| self.resolve(id).is_some());
        let mut running = self.running.lock().unwrap_or_else(|p| p.into_inner());
        if self.closed.load(Ordering::Acquire) || running.as_ref().map(|r| &r.model_id) == wanted.as_ref() {
            return;
        }
        self.service.cancel_all();
        if let Some(previous) = running.take() {
            previous.server.shutdown();
        }
        if let Some(model_id) = wanted
            && let Some((executable, model)) = self.resolve(&model_id)
        {
            tracing::info!(%model_id, "starting language model server");
            let server = LlamaServer::spawn(ServerConfig::new(executable, model));
            let model = Arc::new(ServerModel::new(Arc::clone(&server)));
            *running = Some(Running { model_id, server, model });
        }
    }

    pub fn submit_manual(
        &self,
        settings: &AppSettings,
        request: AskCopilotRequest,
        profile: Option<ProfileSnapshot>,
    ) -> Result<(), IntelligenceError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(IntelligenceError::Closed);
        }
        if self.resolve(&settings.language_model).is_none() {
            return Err(IntelligenceError::ModelMissing);
        }
        self.manual_started.store(true, Ordering::Release);
        self.reconcile(settings);
        self.service.submit_manual(request, profile)?;
        Ok(())
    }

    pub fn cancel_request(&self, request_id: &str) {
        self.service.cancel_request(request_id);
    }

    pub fn listeners(
        &self,
        settings: &AppSettings,
        events: &EventSink,
        active_profile: Arc<dyn Fn() -> Option<ProfileSnapshot> + Send + Sync>,
    ) -> Vec<Arc<dyn SegmentListener>> {
        let mut listeners = vec![self.service.session_listener(active_profile)];
        if self.running.lock().unwrap_or_else(|p| p.into_inner()).is_none()
            && (settings.translation_enabled || settings.copilot_enabled)
        {
            events(SessionEvent::Notice {
                notice: Notice::info(
                    "Download the language model and runtime in Settings to enable translation and the copilot",
                ),
            });
        }
        if settings.translation_enabled {
            listeners.push(TranslationWorker::spawn(
                Arc::clone(&self.translator),
                settings.target_language.clone(),
                Arc::clone(events),
            ));
        }
        listeners
    }

    pub fn stop_session(&self) {
        self.service.stop_session();
    }

    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.service.shutdown().await;
        let running = self.running.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(running) = running {
            running.server.shutdown_and_wait().await;
        }
    }
}

fn preferences(settings: &AppSettings) -> CopilotPreferences {
    CopilotPreferences {
        enabled: settings.copilot_enabled,
        source_language: settings.source_language.clone(),
        user_language: settings.target_language.clone(),
    }
}
