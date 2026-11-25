//! Incremental speech recognition.
//!
//! [`StreamingTranscriber`] turns VAD-gated audio into revisable transcript
//! segments using any [`SpeechRecognizer`]; whisper.cpp is the default
//! adapter. Stabilization follows the LocalAgreement-2 policy from
//! WhisperStreaming (Macháček et al., 2023): a word is committed once two
//! consecutive hypotheses agree on it.

mod agreement;
pub mod metrics;
mod recognizer;
mod streaming;
#[cfg(feature = "whisper")]
mod whisper;

pub use agreement::{LocalAgreement, words_of};
pub use recognizer::{AsrError, DecodeOptions, Hypothesis, SpeechRecognizer};
pub use streaming::{StreamingConfig, StreamingTranscriber};
#[cfg(feature = "whisper")]
pub use whisper::{SileroVad, WhisperConfig, WhisperRecognizer, system_info};
