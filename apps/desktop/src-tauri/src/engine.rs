use std::sync::Arc;

use parlance_asr::{AsrError, SileroVad, SpeechRecognizer, WhisperConfig, WhisperRecognizer};
use parlance_audio::vad::{EnergyVad, VoiceActivityDetector};
use parlance_models::{ModelStore, find};
use parlance_session::SpeechEngine;

use crate::settings::SettingsRepository;

const VAD_MODEL: &str = "silero-vad-5.1.2";

/// Resolves the user's selected speech model from the local store.
pub struct LocalSpeechEngine {
    store: ModelStore,
    settings: Arc<SettingsRepository>,
}

impl LocalSpeechEngine {
    pub fn new(store: ModelStore, settings: Arc<SettingsRepository>) -> Self {
        Self { store, settings }
    }
}

fn missing(id: &str) -> AsrError {
    AsrError::ModelLoad {
        path: id.into(), reason: "model is not installed; download it from Settings → Models".into()
    }
}

impl SpeechEngine for LocalSpeechEngine {
    fn recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, AsrError> {
        let id = self.settings.get().speech_model;
        let entry = find(&id).ok_or_else(|| missing(&id))?;
        let path = self.store.require(entry).map_err(|_| missing(&id))?;
        Ok(Box::new(WhisperRecognizer::load(&path, WhisperConfig::default())?))
    }

    /// Silero when installed; the energy detector keeps the app usable
    /// (with more false triggers) before the model is downloaded.
    fn voice_detector(&self) -> Result<Box<dyn VoiceActivityDetector>, AsrError> {
        let installed = find(VAD_MODEL).and_then(|entry| self.store.require(entry).ok());
        match installed {
            Some(path) => Ok(Box::new(SileroVad::load(&path)?)),
            None => {
                tracing::warn!("Silero VAD not installed; falling back to energy VAD");
                Ok(Box::new(EnergyVad::default()))
            }
        }
    }
}
