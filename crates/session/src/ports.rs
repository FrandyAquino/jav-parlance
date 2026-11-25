use std::sync::Arc;

use parlance_asr::{AsrError, SpeechRecognizer};
use parlance_audio::vad::VoiceActivityDetector;
use parlance_domain::TranscriptSegment;
use parlance_protocol::SessionEvent;

/// Receives every event destined for the UI. Must not block.
pub type EventSink = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Supplies inference components for a new session. Implementations may
/// cache loaded models so restarting a session is cheap.
pub trait SpeechEngine: Send + Sync {
    fn recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, AsrError>;
    fn voice_detector(&self) -> Result<Box<dyn VoiceActivityDetector>, AsrError>;
}

/// Downstream consumer of transcript updates (translation, copilot,
/// persistence). Called from the ASR thread, so it must hand work off
/// instead of doing it inline.
pub trait SegmentListener: Send + Sync {
    fn on_segment(&self, segment: &TranscriptSegment);
    /// The session ended; flush or cancel pending work.
    fn on_session_end(&self) {}
}
