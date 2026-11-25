use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use parlance_domain::{LanguageCode, SegmentId, TranscriptSegment, Translation};
use parlance_protocol::{Notice, SessionEvent};
use parlance_session::{EventSink, SegmentListener};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::translator::{TranslatedLine, TranslationRequest, Translator};

const QUEUE: usize = 64;
const CONTEXT_SENTENCES: usize = 2;

struct Job {
    segment_id: SegmentId,
    revision: u32,
    text: String,
    language: Option<LanguageCode>,
}

/// Translates final segments in order on a background task. The ASR thread
/// only performs a non-blocking `try_send`.
pub struct TranslationWorker {
    queue: Mutex<Option<mpsc::Sender<Job>>>,
    events: EventSink,
    warned_overflow: AtomicBool,
}

impl std::fmt::Debug for TranslationWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranslationWorker").finish_non_exhaustive()
    }
}

impl TranslationWorker {
    /// Must be called within a Tokio runtime.
    pub fn spawn(translator: Arc<dyn Translator>, target: LanguageCode, events: EventSink) -> Arc<Self> {
        let (tx, rx) = mpsc::channel(QUEUE);
        tokio::spawn(run(rx, translator, target, Arc::clone(&events), CancellationToken::new()));
        Arc::new(Self { queue: Mutex::new(Some(tx)), events, warned_overflow: AtomicBool::new(false) })
    }
}

impl SegmentListener for TranslationWorker {
    fn on_segment(&self, segment: &TranscriptSegment) {
        if !segment.is_final() || segment.stable_text.trim().is_empty() {
            return;
        }
        let job = Job {
            segment_id: segment.id,
            revision: segment.revision,
            text: segment.text(),
            language: segment.language.clone(),
        };
        let queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        let Some(sender) = queue.as_ref() else { return };
        if sender.try_send(job).is_err() && !self.warned_overflow.swap(true, Ordering::Relaxed) {
            (self.events)(SessionEvent::Notice {
                notice: Notice::warning("Translation is falling behind; some lines were skipped"),
            });
        }
    }

    /// Closing the queue lets the task finish what is queued, then exit. The
    /// worker is deliberately not cancelled on drop: the ASR stage drops it
    /// right after this call, and the last lines of a meeting must still be
    /// translated.
    fn on_session_end(&self) {
        self.queue.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

async fn run(
    mut jobs: mpsc::Receiver<Job>,
    translator: Arc<dyn Translator>,
    target: LanguageCode,
    events: EventSink,
    cancel: CancellationToken,
) {
    let mut context: VecDeque<TranslatedLine> = VecDeque::new();
    let mut reported_failure = false;
    while let Some(job) = tokio::select! {
        _ = cancel.cancelled() => None,
        job = jobs.recv() => job,
    } {
        let emit = |text: String, is_final: bool| {
            events(SessionEvent::Translation {
                translation: Translation {
                    segment_id: job.segment_id,
                    source_revision: job.revision,
                    target_language: target.clone(),
                    text,
                    is_final,
                },
            });
        };
        if job.language.as_ref() == Some(&target) {
            emit(job.text.clone(), true);
            continue;
        }
        let streamed = Mutex::new(String::new());
        let on_token = |token: &str| {
            let mut text = streamed.lock().unwrap_or_else(|p| p.into_inner());
            text.push_str(token);
            emit(text.trim().to_owned(), false);
        };
        let request = TranslationRequest {
            text: job.text.clone(),
            source: job.language.clone(),
            target: target.clone(),
            context: context.iter().cloned().collect(),
        };
        match translator.translate(request, cancel.child_token(), &on_token).await {
            Ok(text) => {
                emit(text.clone(), true);
                context.push_back(TranslatedLine { source: job.text.clone(), translation: text });
                if context.len() > CONTEXT_SENTENCES {
                    context.pop_front();
                }
            }
            Err(error) if !reported_failure => {
                reported_failure = true;
                tracing::warn!(%error, "translation failed");
                events(SessionEvent::Notice { notice: Notice::warning(format!("Translation unavailable: {error}")) });
            }
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translator::TranslationError;
    use async_trait::async_trait;
    use parlance_domain::Stability;
    use parlance_llm::OnToken;

    struct Upper;

    #[async_trait]
    impl Translator for Upper {
        async fn translate(
            &self,
            request: TranslationRequest,
            _cancel: CancellationToken,
            on_token: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            on_token("partial");
            Ok(format!("{}|ctx={}", request.text.to_uppercase(), request.context.len()))
        }
    }

    fn segment(id: u64, text: &str, language: &str, stability: Stability) -> TranscriptSegment {
        TranscriptSegment {
            id: SegmentId(id),
            revision: 3,
            stable_text: text.into(),
            unstable_text: String::new(),
            language: Some(LanguageCode::new(language).unwrap()),
            start_ms: 0,
            end_ms: 0,
            stability,
        }
    }

    fn recorder() -> (EventSink, Arc<Mutex<Vec<Translation>>>) {
        let log: Arc<Mutex<Vec<Translation>>> = Arc::default();
        let sink_log = Arc::clone(&log);
        let sink: EventSink = Arc::new(move |event| {
            if let SessionEvent::Translation { translation } = event {
                sink_log.lock().unwrap().push(translation);
            }
        });
        (sink, log)
    }

    #[tokio::test]
    async fn translates_finals_in_order_with_context_and_skips_partials() {
        let (sink, log) = recorder();
        let worker = TranslationWorker::spawn(Arc::new(Upper), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, "partial line", "en", Stability::Partial));
        worker.on_segment(&segment(1, "hello", "en", Stability::Final));
        worker.on_segment(&segment(2, "bye", "en", Stability::Final));
        worker.on_segment(&segment(3, "hola", "es", Stability::Final));
        worker.on_session_end();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let finals: Vec<_> = log.lock().unwrap().iter().filter(|t| t.is_final).cloned().collect();
        let texts: Vec<_> = finals.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["HELLO|ctx=0", "BYE|ctx=1", "hola"]);
        assert!(finals.iter().all(|t| t.source_revision == 3 && t.target_language.as_str() == "es"));
        assert!(log.lock().unwrap().iter().any(|t| !t.is_final && t.text == "partial"));
    }
}
