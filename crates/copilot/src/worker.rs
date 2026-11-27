use crate::{CopilotPreferences, CopilotService, SuggestionPolicy};
use parlance_domain::{LanguageCode, TranscriptSegment};
use parlance_llm::LanguageModel;
use parlance_session::{EventSink, SegmentListener};
use parlance_translation::Translator;
use std::sync::Arc;

/// Compatibility adapter for callers whose copilot lifetime is still a session.
pub struct CopilotWorker {
    listener: Arc<dyn SegmentListener>,
}
impl std::fmt::Debug for CopilotWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopilotWorker").finish_non_exhaustive()
    }
}
impl CopilotWorker {
    pub fn spawn(
        model: Arc<dyn LanguageModel>,
        translator: Arc<dyn Translator>,
        user_language: LanguageCode,
        events: EventSink,
        policy: SuggestionPolicy,
    ) -> Arc<Self> {
        let service = CopilotService::new(
            model,
            translator,
            events,
            policy,
            CopilotPreferences { enabled: true, source_language: None, user_language },
        );
        Arc::new(Self { listener: service.session_listener(Arc::new(|| None)) })
    }
}
impl SegmentListener for CopilotWorker {
    fn on_segment(&self, segment: &TranscriptSegment) {
        self.listener.on_segment(segment);
    }
    fn on_session_end(&self) {
        self.listener.on_session_end();
    }
}
