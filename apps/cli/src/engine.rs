use std::path::PathBuf;

use anyhow::Context;
use parlance_asr::{AsrError, SileroVad, SpeechRecognizer, WhisperConfig, WhisperRecognizer};
use parlance_audio::vad::VoiceActivityDetector;
use parlance_models::{ModelStore, find};
use parlance_session::SpeechEngine;

/// Speech engine over fixed, already-installed model files.
pub struct FileEngine {
    whisper: PathBuf,
    vad: PathBuf,
}

impl FileEngine {
    pub fn from_catalog(speech_model: &str) -> anyhow::Result<Self> {
        let store = ModelStore::default_location()?;
        let resolve = |id: &str| -> anyhow::Result<PathBuf> {
            let entry = find(id).with_context(|| format!("unknown model `{id}`"))?;
            store.require(entry).with_context(|| format!("download `{id}` first (see the desktop app's Settings)"))
        };
        Ok(Self { whisper: resolve(speech_model)?, vad: resolve("silero-vad-5.1.2")? })
    }
}

impl SpeechEngine for FileEngine {
    fn recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, AsrError> {
        Ok(Box::new(WhisperRecognizer::load(&self.whisper, WhisperConfig::default())?))
    }

    fn voice_detector(&self) -> Result<Box<dyn VoiceActivityDetector>, AsrError> {
        Ok(Box::new(SileroVad::load(&self.vad)?))
    }
}
