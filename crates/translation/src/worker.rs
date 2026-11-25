use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use parlance_domain::{LanguageCode, SegmentId, TranscriptSegment, Translation};
use parlance_protocol::{Notice, SessionEvent};
use parlance_session::{EventSink, SegmentListener};
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

use crate::translator::{TranslatedLine, TranslationRequest, Translator};

const QUEUE: usize = 64;
const CONTEXT_SENTENCES: usize = 2;

#[derive(Clone)]
struct Job {
    segment_id: SegmentId,
    revision: u32,
    text: String,
    language: Option<LanguageCode>,
    is_final: bool,
}

impl Job {
    fn from_segment(segment: &TranscriptSegment) -> Self {
        Self {
            segment_id: segment.id,
            revision: segment.revision,
            text: segment.text(),
            language: segment.language.clone(),
            is_final: segment.is_final(),
        }
    }
}

/// Holds only the newest partial hypothesis. While a translation runs, newer
/// partials overwrite older ones, so the worker always translates the latest
/// words instead of working through a backlog of stale ones.
#[derive(Default)]
struct LatestPartial {
    job: Mutex<Option<Job>>,
    last_text: Mutex<Option<(SegmentId, String)>>,
    ready: Notify,
}

impl LatestPartial {
    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn offer(&self, job: Job) {
        let mut last = Self::lock(&self.last_text);
        if last.as_ref().is_some_and(|(id, text)| *id == job.segment_id && *text == job.text) {
            return;
        }
        *last = Some((job.segment_id, job.text.clone()));
        *Self::lock(&self.job) = Some(job);
        self.ready.notify_one();
    }

    fn take(&self) -> Option<Job> {
        Self::lock(&self.job).take()
    }

    /// A final supersedes any pending partial of the same segment.
    fn discard(&self, segment_id: SegmentId) {
        let mut job = Self::lock(&self.job);
        if job.as_ref().is_some_and(|j| j.segment_id == segment_id) {
            job.take();
        }
    }
}

/// Translates as people speak: partial hypotheses are translated
/// latest-wins for immediacy, final segments are translated in order (and
/// take priority) for the definitive text. The ASR thread never blocks.
pub struct TranslationWorker {
    finals: Mutex<Option<mpsc::Sender<Job>>>,
    partial: Arc<LatestPartial>,
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
        let partial = Arc::new(LatestPartial::default());
        let task = Task { translator, target, events: Arc::clone(&events), context: VecDeque::new(), failed: false };
        tokio::spawn(task.run(rx, Arc::clone(&partial), CancellationToken::new()));
        Arc::new(Self { finals: Mutex::new(Some(tx)), partial, events, warned_overflow: AtomicBool::new(false) })
    }
}

impl SegmentListener for TranslationWorker {
    fn on_segment(&self, segment: &TranscriptSegment) {
        if segment.text().trim().is_empty() {
            return;
        }
        let job = Job::from_segment(segment);
        if !job.is_final {
            self.partial.offer(job);
            return;
        }
        self.partial.discard(job.segment_id);
        let finals = self.finals.lock().unwrap_or_else(|p| p.into_inner());
        let Some(sender) = finals.as_ref() else { return };
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
        self.finals.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

struct Task {
    translator: Arc<dyn Translator>,
    target: LanguageCode,
    events: EventSink,
    context: VecDeque<TranslatedLine>,
    failed: bool,
}

impl Task {
    async fn run(mut self, mut finals: mpsc::Receiver<Job>, partial: Arc<LatestPartial>, cancel: CancellationToken) {
        loop {
            let job = tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                job = finals.recv() => match job {
                    Some(job) => job,
                    None => break,
                },
                _ = partial.ready.notified() => match partial.take() {
                    Some(job) => job,
                    None => continue,
                },
            };
            self.translate(job, &cancel).await;
        }
    }

    fn emit(&self, job: &Job, text: String, is_final: bool) {
        (self.events)(SessionEvent::Translation {
            translation: Translation {
                segment_id: job.segment_id,
                source_revision: job.revision,
                target_language: self.target.clone(),
                text,
                is_final,
            },
        });
    }

    async fn translate(&mut self, job: Job, cancel: &CancellationToken) {
        if job.language.as_ref() == Some(&self.target) {
            self.emit(&job, job.text.clone(), job.is_final);
            return;
        }
        let streamed = Mutex::new(String::new());
        let on_token = |token: &str| {
            let mut text = streamed.lock().unwrap_or_else(|p| p.into_inner());
            text.push_str(token);
            self.emit(&job, text.trim().to_owned(), false);
        };
        let request = TranslationRequest {
            text: job.text.clone(),
            source: job.language.clone(),
            target: self.target.clone(),
            context: self.context.iter().cloned().collect(),
        };
        let result = self.translator.translate(request, cancel.child_token(), &on_token).await;
        match result {
            Ok(text) => {
                self.emit(&job, text.clone(), job.is_final);
                if job.is_final {
                    self.remember(TranslatedLine { source: job.text, translation: text });
                }
            }
            Err(error) if !self.failed => {
                self.failed = true;
                tracing::warn!(%error, "translation failed");
                (self.events)(SessionEvent::Notice {
                    notice: Notice::warning(format!("Translation unavailable: {error}")),
                });
            }
            Err(_) => {}
        }
    }

    fn remember(&mut self, line: TranslatedLine) {
        self.context.push_back(line);
        if self.context.len() > CONTEXT_SENTENCES {
            self.context.pop_front();
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

    fn segment(id: u64, revision: u32, text: &str, language: &str, stability: Stability) -> TranscriptSegment {
        TranscriptSegment {
            id: SegmentId(id),
            revision,
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

    fn spawn(sink: EventSink) -> Arc<TranslationWorker> {
        TranslationWorker::spawn(Arc::new(Upper), LanguageCode::new("es").unwrap(), sink)
    }

    #[tokio::test]
    async fn translates_finals_in_order_with_context() {
        let (sink, log) = recorder();
        let worker = spawn(sink);
        worker.on_segment(&segment(1, 3, "hello", "en", Stability::Final));
        worker.on_segment(&segment(2, 3, "bye", "en", Stability::Final));
        worker.on_segment(&segment(3, 3, "hola", "es", Stability::Final));
        worker.on_session_end();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let finals: Vec<_> = log.lock().unwrap().iter().filter(|t| t.is_final).cloned().collect();
        let texts: Vec<_> = finals.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["HELLO|ctx=0", "BYE|ctx=1", "hola"]);
        assert!(finals.iter().all(|t| t.source_revision == 3 && t.target_language.as_str() == "es"));
    }

    #[tokio::test]
    async fn translates_partials_while_speaking_without_using_them_as_context() {
        let (sink, log) = recorder();
        let worker = spawn(sink);
        worker.on_segment(&segment(1, 0, "could you", "en", Stability::Partial));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        worker.on_segment(&segment(1, 1, "could you", "en", Stability::Partial));
        worker.on_segment(&segment(1, 2, "could you send", "en", Stability::Final));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let log = log.lock().unwrap();
        let partial = log.iter().find(|t| !t.is_final && t.text == "COULD YOU|ctx=0").expect("partial translation");
        assert_eq!(partial.source_revision, 0);
        let attempts = log.iter().filter(|t| t.text.starts_with("COULD YOU|")).count();
        assert_eq!(attempts, 1, "unchanged partial text is not retranslated");
        let last = log.last().unwrap();
        assert!(last.is_final);
        assert_eq!(last.text, "COULD YOU SEND|ctx=0");
    }

    #[test]
    fn latest_partial_keeps_only_the_newest_job() {
        let slot = LatestPartial::default();
        slot.offer(Job::from_segment(&segment(1, 0, "a", "en", Stability::Partial)));
        slot.offer(Job::from_segment(&segment(1, 1, "a b", "en", Stability::Partial)));
        assert_eq!(slot.take().map(|j| j.revision), Some(1));
        assert!(slot.take().is_none());
        slot.offer(Job::from_segment(&segment(2, 0, "c", "en", Stability::Partial)));
        slot.discard(SegmentId(2));
        assert!(slot.take().is_none());
    }
}
