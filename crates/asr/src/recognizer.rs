use parlance_domain::LanguageCode;

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("failed to load speech model `{path}`: {reason}")]
    ModelLoad { path: String, reason: String },
    #[error("speech recognition failed: {0}")]
    Inference(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecodeOptions {
    /// `None` lets the model detect the spoken language.
    pub language: Option<LanguageCode>,
    /// Trade accuracy for latency; used for partial hypotheses only.
    pub fast: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hypothesis {
    pub text: String,
    pub language: Option<LanguageCode>,
}

/// Port for any batch recognizer that can decode a short mono 16 kHz buffer.
pub trait SpeechRecognizer: Send {
    fn transcribe(&mut self, audio: &[f32], options: &DecodeOptions) -> Result<Hypothesis, AsrError>;
}

impl<R: SpeechRecognizer + ?Sized> SpeechRecognizer for Box<R> {
    fn transcribe(&mut self, audio: &[f32], options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
        (**self).transcribe(audio, options)
    }
}
