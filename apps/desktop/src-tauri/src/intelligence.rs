use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use parlance_copilot::{CopilotWorker, SuggestionPolicy};
use parlance_llm::{LanguageModel, LlamaServer, ServerConfig, ServerModel};
use parlance_models::{ModelStore, find};
use parlance_protocol::{AppSettings, Notice, SessionEvent};
use parlance_session::{EventSink, SegmentListener};
use parlance_translation::{LlmTranslator, TranslationWorker, Translator};

const RUNTIME_ID: &str = "llama-server";

struct Running {
    model_id: String,
    server: Arc<LlamaServer>,
}

/// Owns the LLM sidecar and builds per-session translation/copilot listeners.
/// The server is started as soon as it is useful so it is warm by the time a
/// session begins, and restarted when the user switches models.
pub struct Intelligence {
    store: ModelStore,
    running: Mutex<Option<Running>>,
}

impl Intelligence {
    pub fn new(store: ModelStore) -> Self {
        Self { store, running: Mutex::new(None) }
    }

    fn resolve(&self, model_id: &str) -> Option<(PathBuf, PathBuf)> {
        let executable = self.store.find_executable(find(RUNTIME_ID)?, RUNTIME_ID)?;
        let model = self.store.require(find(model_id)?).ok()?;
        Some((executable, model))
    }

    /// Starts, restarts or stops the sidecar to match `settings`. Must be
    /// called within a Tokio runtime.
    pub fn reconcile(&self, settings: &AppSettings) {
        let wanted = (settings.translation_enabled || settings.copilot_enabled)
            .then(|| settings.language_model.clone())
            .filter(|id| self.resolve(id).is_some());
        let mut running = self.running.lock().unwrap_or_else(|p| p.into_inner());
        if running.as_ref().map(|r| &r.model_id) == wanted.as_ref() {
            return;
        }
        if let Some(previous) = running.take() {
            previous.server.shutdown();
        }
        if let Some(model_id) = wanted
            && let Some((executable, model)) = self.resolve(&model_id)
        {
            tracing::info!(%model_id, "starting language model server");
            let server = LlamaServer::spawn(ServerConfig::new(executable, model));
            *running = Some(Running { model_id, server });
        }
    }

    fn model(&self) -> Option<Arc<dyn LanguageModel>> {
        let running = self.running.lock().unwrap_or_else(|p| p.into_inner());
        running.as_ref().map(|r| Arc::new(ServerModel::new(Arc::clone(&r.server))) as Arc<dyn LanguageModel>)
    }

    pub fn listeners(&self, settings: &AppSettings, events: &EventSink) -> Vec<Arc<dyn SegmentListener>> {
        if !settings.translation_enabled && !settings.copilot_enabled {
            return Vec::new();
        }
        let Some(model) = self.model() else {
            events(SessionEvent::Notice {
                notice: Notice::info(
                    "Download the language model and runtime in Settings to enable translation and the copilot",
                ),
            });
            return Vec::new();
        };
        let translator: Arc<dyn Translator> = Arc::new(LlmTranslator::new(Arc::clone(&model)));
        let target = settings.target_language.clone();
        let mut listeners: Vec<Arc<dyn SegmentListener>> = Vec::new();
        if settings.translation_enabled {
            listeners.push(TranslationWorker::spawn(Arc::clone(&translator), target.clone(), Arc::clone(events)));
        }
        if settings.copilot_enabled {
            let policy = SuggestionPolicy::default();
            listeners.push(CopilotWorker::spawn(model, translator, target, Arc::clone(events), policy));
        }
        listeners
    }

    pub fn shutdown(&self) {
        if let Some(running) = self.running.lock().unwrap_or_else(|p| p.into_inner()).take() {
            running.server.shutdown();
        }
    }
}
