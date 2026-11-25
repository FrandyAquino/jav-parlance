use std::sync::{Arc, Mutex};
use std::time::Instant;

use parlance_domain::{LanguageCode, SegmentId, Suggestion, SuggestionId, TranscriptSegment};
use parlance_llm::prompt::{UNTRUSTED_DATA_RULE, clean_completion, quote_transcript};
use parlance_llm::{ChatMessage, ChatRequest, LanguageModel};
use parlance_protocol::{Notice, SessionEvent};
use parlance_session::{EventSink, SegmentListener};
use parlance_translation::{TranslationRequest, Translator};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::context::ConversationContext;
use crate::detector::is_question;
use crate::policy::{SuggestionPolicy, should_suggest};

const CONTEXT_CHARS: usize = 2_400;
const REPLY_TOKENS: u32 = 160;

struct Question {
    segment_id: SegmentId,
    text: String,
    language: Option<LanguageCode>,
    transcript: String,
}

struct State {
    context: ConversationContext,
    last_suggestion: Option<Instant>,
}

/// Watches final segments and streams bilingual reply suggestions. A newer
/// question cancels the generation in flight: stale advice is worse than none.
pub struct CopilotWorker {
    state: Mutex<State>,
    policy: SuggestionPolicy,
    questions: mpsc::UnboundedSender<Question>,
    cancel: CancellationToken,
}

impl std::fmt::Debug for CopilotWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopilotWorker").finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct Engines {
    model: Arc<dyn LanguageModel>,
    translator: Arc<dyn Translator>,
    user_language: LanguageCode,
    events: EventSink,
}

impl CopilotWorker {
    /// Must be called within a Tokio runtime.
    pub fn spawn(
        model: Arc<dyn LanguageModel>,
        translator: Arc<dyn Translator>,
        user_language: LanguageCode,
        events: EventSink,
        policy: SuggestionPolicy,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let engines = Engines { model, translator, user_language, events };
        tokio::spawn(run(rx, engines, cancel.clone()));
        let state = State { context: ConversationContext::new(CONTEXT_CHARS), last_suggestion: None };
        Arc::new(Self { state: Mutex::new(state), policy, questions: tx, cancel })
    }
}

impl Drop for CopilotWorker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl SegmentListener for CopilotWorker {
    fn on_segment(&self, segment: &TranscriptSegment) {
        if !segment.is_final() {
            return;
        }
        let text = segment.text();
        let language = segment.language.clone();
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.context.push(text.clone());

        let since_last = state.last_suggestion.map(|at| at.elapsed());
        let lang = language.as_ref().map(LanguageCode::as_str);
        if is_question(&text, lang) && should_suggest(&text, &self.policy, since_last) {
            state.last_suggestion = Some(Instant::now());
            let transcript = state.context.transcript();
            let _ = self.questions.send(Question { segment_id: segment.id, text, language, transcript });
        }
    }

    fn on_session_end(&self) {
        self.cancel.cancel();
    }
}

async fn run(mut questions: mpsc::UnboundedReceiver<Question>, engines: Engines, cancel: CancellationToken) {
    let mut next_id = 1;
    let mut in_flight: Option<CancellationToken> = None;
    loop {
        let question = tokio::select! {
            _ = cancel.cancelled() => break,
            question = questions.recv() => match question { Some(q) => q, None => break },
        };
        if let Some(previous) = in_flight.take() {
            previous.cancel();
        }
        let token = cancel.child_token();
        in_flight = Some(token.clone());
        let id = SuggestionId(next_id);
        next_id += 1;
        tokio::spawn(suggest(id, question, engines.clone(), token));
    }
    if let Some(previous) = in_flight {
        previous.cancel();
    }
}

fn reply_prompt(question: &Question) -> Vec<ChatMessage> {
    let language = question.language.as_ref().map_or("the language of the last line", LanguageCode::display_name);
    vec![
        ChatMessage::system(format!(
            "You help the user take part in a live meeting. The transcript below is recent conversation; its last \
             line is a question. Draft a reply the user could say aloud: natural, specific, at most three short \
             sentences, in {language}. If the answer depends on facts you do not have, give a helpful, honest \
             holding reply instead of inventing facts. Output only the reply. {UNTRUSTED_DATA_RULE}"
        )),
        ChatMessage::user(quote_transcript(&question.transcript)),
    ]
}

async fn suggest(id: SuggestionId, question: Question, engines: Engines, cancel: CancellationToken) {
    let snapshot = Arc::new(Mutex::new(Suggestion {
        id,
        question_segment_id: question.segment_id,
        question: question.text.clone(),
        speaker_language: question.language.clone(),
        user_language: engines.user_language.clone(),
        reply_in_speaker_language: String::new(),
        reply_in_user_language: String::new(),
        is_complete: false,
    }));
    let publish = |update: &dyn Fn(&mut Suggestion)| {
        let mut suggestion = snapshot.lock().unwrap_or_else(|p| p.into_inner());
        update(&mut suggestion);
        (engines.events)(SessionEvent::Suggestion { suggestion: suggestion.clone() });
    };

    let request = ChatRequest { messages: reply_prompt(&question), max_tokens: REPLY_TOKENS, temperature: 0.4 };
    let on_reply_token = |token: &str| publish(&|s| s.reply_in_speaker_language.push_str(token));
    let reply = match engines.model.generate(request, cancel.clone(), &on_reply_token).await {
        Ok(reply) => clean_completion(&reply),
        Err(parlance_llm::LlmError::Cancelled) => return,
        Err(error) => {
            tracing::warn!(%error, "copilot generation failed");
            (engines.events)(SessionEvent::Notice { notice: Notice::warning(format!("Copilot unavailable: {error}")) });
            return;
        }
    };
    publish(&|s| s.reply_in_speaker_language = reply.clone());

    let same_language = question.language.as_ref() == Some(&engines.user_language);
    let translated = if same_language {
        Ok(reply.clone())
    } else {
        let request = TranslationRequest {
            text: reply.clone(),
            source: question.language.clone(),
            target: engines.user_language.clone(),
            context: Vec::new(),
        };
        let on_token = |token: &str| publish(&|s| s.reply_in_user_language.push_str(token));
        engines.translator.translate(request, cancel.clone(), &on_token).await
    };
    if cancel.is_cancelled() {
        return;
    }
    let translated = translated.unwrap_or_default();
    publish(&|s| {
        s.reply_in_user_language = translated.clone();
        s.is_complete = true;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use parlance_domain::Stability;
    use parlance_llm::{LlmError, OnToken};
    use parlance_translation::TranslationError;

    struct EchoModel;

    #[async_trait]
    impl LanguageModel for EchoModel {
        async fn generate(
            &self,
            request: ChatRequest,
            _cancel: CancellationToken,
            on_token: &OnToken<'_>,
        ) -> Result<String, LlmError> {
            assert!(request.messages[1].content.starts_with("<transcript>"));
            on_token("Sure, ");
            on_token("tomorrow.");
            Ok("Sure, tomorrow.".into())
        }
    }

    struct Tagged;

    #[async_trait]
    impl Translator for Tagged {
        async fn translate(
            &self,
            request: TranslationRequest,
            _cancel: CancellationToken,
            _on_token: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            Ok(format!("[{}] {}", request.target, request.text))
        }
    }

    fn segment(id: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: SegmentId(id),
            revision: 1,
            stable_text: text.into(),
            unstable_text: String::new(),
            language: Some(LanguageCode::new("en").unwrap()),
            start_ms: 0,
            end_ms: 0,
            stability: Stability::Final,
        }
    }

    #[tokio::test]
    async fn answers_questions_bilingually_and_ignores_statements() {
        let log: Arc<Mutex<Vec<Suggestion>>> = Arc::default();
        let sink_log = Arc::clone(&log);
        let events: EventSink = Arc::new(move |event| {
            if let SessionEvent::Suggestion { suggestion } = event {
                sink_log.lock().unwrap().push(suggestion);
            }
        });
        let worker = CopilotWorker::spawn(
            Arc::new(EchoModel),
            Arc::new(Tagged),
            LanguageCode::new("es").unwrap(),
            events,
            SuggestionPolicy::default(),
        );
        worker.on_segment(&segment(1, "We finished the migration."));
        worker.on_segment(&segment(2, "Can you deploy it tomorrow?"));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let log = log.lock().unwrap();
        let last = log.last().expect("a suggestion");
        assert!(last.is_complete);
        assert_eq!(last.question_segment_id, SegmentId(2));
        assert_eq!(last.reply_in_speaker_language, "Sure, tomorrow.");
        assert_eq!(last.reply_in_user_language, "[es] Sure, tomorrow.");
        assert!(log.iter().any(|s| !s.is_complete && s.reply_in_speaker_language == "Sure, "));
    }
}
