use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use parlance_domain::{LanguageCode, SegmentId, TranscriptSegment, Translation};
use parlance_protocol::{Notice, SessionEvent};
use parlance_session::{EventSink, SegmentListener};
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

use crate::translator::{TranslatedLine, TranslationRequest, Translator};

const QUEUE: usize = 64;
const CONTEXT_SENTENCES: usize = 2;
/// Pending finals plus the one active partial bound retained display previews.
const MAX_PREVIEWS: usize = QUEUE + 1;
/// Finish queued finals while leaving five seconds of the desktop's 15 s history drain for recorder I/O.
const FINAL_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

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
    last_text: Mutex<Option<(SegmentId, String, Option<LanguageCode>)>>,
    finalized: Mutex<Option<SegmentId>>,
    ready: Notify,
}

impl LatestPartial {
    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn offer(&self, job: Job) {
        let mut last = Self::lock(&self.last_text);
        if last
            .as_ref()
            .is_some_and(|(id, text, language)| *id == job.segment_id && *text == job.text && *language == job.language)
        {
            return;
        }
        *last = Some((job.segment_id, job.text.clone(), job.language.clone()));
        *Self::lock(&self.job) = Some(job);
        self.ready.notify_one();
    }

    fn take(&self) -> Option<Job> {
        Self::lock(&self.job).take()
    }

    /// A final supersedes any pending partial of the same segment.
    fn discard(&self, segment_id: SegmentId) {
        {
            let mut finalized = Self::lock(&self.finalized);
            if finalized.is_none_or(|id| id.0 < segment_id.0) {
                *finalized = Some(segment_id);
            }
        }
        let mut job = Self::lock(&self.job);
        if job.as_ref().is_some_and(|j| j.segment_id == segment_id) {
            job.take();
        }
        self.ready.notify_one();
    }

    fn supersedes(&self, id: SegmentId, language: &Option<LanguageCode>) -> bool {
        if Self::lock(&self.finalized).is_some_and(|closed| closed.0 >= id.0) {
            return true;
        }
        Self::lock(&self.job).as_ref().is_some_and(|job| job.segment_id != id || &job.language != language)
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
    stop: CancellationToken,
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
        let task = Task {
            translator,
            target,
            events: Arc::clone(&events),
            context: VecDeque::new(),
            previews: Mutex::new(BTreeMap::new()),
            failed: false,
        };
        let stop = CancellationToken::new();
        let stopped = stop.clone();
        let task_partial = Arc::clone(&partial);
        let drain_events = Arc::clone(&events);
        // Session end runs on the ASR OS thread, so install its async lifecycle here.
        tokio::spawn(async move {
            let cancel = CancellationToken::new();
            let running = task.run(rx, task_partial, cancel.clone());
            tokio::pin!(running);
            tokio::select! {
                _ = &mut running => return,
                _ = stopped.cancelled() => {}
            }
            if tokio::time::timeout(FINAL_DRAIN_TIMEOUT, &mut running).await.is_err() {
                cancel.cancel();
                tracing::warn!("final translation drain timed out");
                drain_events(SessionEvent::Notice {
                    notice: Notice::warning("Stopped waiting for final translations; some lines may be incomplete."),
                });
                // Dropping the future also releases the sink if a translator ignores cancellation.
            }
        });
        Arc::new(Self { finals: Mutex::new(Some(tx)), partial, events, warned_overflow: AtomicBool::new(false), stop })
    }
}

impl SegmentListener for TranslationWorker {
    fn on_segment(&self, segment: &TranscriptSegment) {
        if segment.is_final() {
            self.partial.discard(segment.id);
        }
        if segment.text().trim().is_empty() {
            return;
        }
        let job = Job::from_segment(segment);
        if !job.is_final {
            self.partial.offer(job);
            return;
        }
        let finals = self.finals.lock().unwrap_or_else(|p| p.into_inner());
        let Some(sender) = finals.as_ref() else { return };
        if sender.try_send(job).is_err() && !self.warned_overflow.swap(true, Ordering::Relaxed) {
            (self.events)(SessionEvent::Notice {
                notice: Notice::warning("Translation is falling behind; some lines were skipped"),
            });
        }
    }

    /// Closing the queue drains healthy finals within a bounded grace period.
    /// The ASR stage drops this worker immediately afterward; the task owns the
    /// sink until the queued work finishes or its drain deadline expires.
    fn on_session_end(&self) {
        self.finals.lock().unwrap_or_else(|p| p.into_inner()).take();
        self.stop.cancel();
    }
}

struct Task {
    translator: Arc<dyn Translator>,
    target: LanguageCode,
    events: EventSink,
    context: VecDeque<TranslatedLine>,
    previews: Mutex<BTreeMap<u64, String>>,
    failed: bool,
}

impl Task {
    async fn run(mut self, mut finals: mpsc::Receiver<Job>, partial: Arc<LatestPartial>, cancel: CancellationToken) {
        let mut next_final = None;
        let mut prefer_live = false;
        loop {
            let ready = if prefer_live { partial.take() } else { None };
            let job =
                match ready.or_else(|| next_final.take()).or_else(|| finals.try_recv().ok()).or_else(|| partial.take())
                {
                    Some(job) => job,
                    None => tokio::select! {
                        biased;
                        _ = cancel.cancelled() => break,
                        job = finals.recv() => match job { Some(job) => job, None => break },
                        _ = partial.ready.notified() => continue,
                    },
                };
            prefer_live = job.is_final;
            if job.is_final {
                self.translate(job, &cancel).await;
                continue;
            }
            let (id, language) = (job.segment_id, job.language.clone());
            if partial.supersedes(id, &language) {
                continue;
            }
            let generation = cancel.child_token();
            let work = self.translate(job, &generation);
            tokio::pin!(work);
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return,
                    _ = &mut work => break,
                    job = finals.recv(), if next_final.is_none() => {
                        let Some(job) = job else { generation.cancel(); return; };
                        let superseded = job.segment_id.0 >= id.0;
                        next_final = Some(job);
                        if superseded { generation.cancel(); break; }
                    },
                    _ = partial.ready.notified() => {
                        if partial.supersedes(id, &language) { generation.cancel(); break; }
                    },
                }
            }
        }
    }

    fn emit(&self, job: &Job, text: String, is_final: bool) {
        {
            let mut previews = self.previews.lock().unwrap_or_else(|p| p.into_inner());
            if is_final {
                previews.remove(&job.segment_id.0);
            } else {
                previews.insert(job.segment_id.0, text.clone());
                while previews.len() > MAX_PREVIEWS {
                    previews.pop_first();
                }
            }
        }
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
        let previous =
            self.previews.lock().unwrap_or_else(|p| p.into_inner()).get(&job.segment_id.0).cloned().unwrap_or_default();
        let streamed = Mutex::new(String::new());
        let on_token = |token: &str| {
            let mut text = streamed.lock().unwrap_or_else(|p| p.into_inner());
            text.push_str(token);
            let next = text.trim();
            if next.starts_with(&previous) {
                self.emit(&job, next.to_owned(), false);
            }
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
            Err(error) => {
                let is_final = previous.is_empty() && job.is_final;
                self.emit(&job, previous, is_final);
                if job.is_final {
                    self.previews.lock().unwrap_or_else(|p| p.into_inner()).remove(&job.segment_id.0);
                }
                if !self.failed {
                    self.failed = true;
                    tracing::warn!(%error, "translation failed");
                    (self.events)(SessionEvent::Notice {
                        notice: Notice::warning(format!("Translation unavailable: {error}")),
                    });
                }
            }
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

    struct StalledFinal {
        entered: Notify,
        token: Mutex<Option<CancellationToken>>,
    }

    #[async_trait]
    impl Translator for StalledFinal {
        async fn translate(
            &self,
            _: TranslationRequest,
            cancel: CancellationToken,
            _: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            *self.token.lock().unwrap() = Some(cancel);
            self.entered.notify_one();
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn session_end_from_os_thread_bounds_stalled_final_and_releases_sink() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&received);
        let sink: EventSink = Arc::new(move |event| output.lock().unwrap().push(event));
        let retained = Arc::downgrade(&sink);
        let translator = Arc::new(StalledFinal { entered: Notify::new(), token: Mutex::new(None) });
        let worker = TranslationWorker::spawn(translator.clone(), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 1, "stalled", "en", Stability::Final));
        translator.entered.notified().await;
        tokio::time::pause();
        std::thread::spawn(move || worker.on_session_end()).join().unwrap();
        // Let the lifecycle task observe Stop before advancing its drain clock.
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(12)).await;
        tokio::task::yield_now().await;
        assert!(retained.upgrade().is_none(), "a stalled final must release the recorder before its 15 s deadline");
        assert!(translator.token.lock().unwrap().as_ref().unwrap().is_cancelled());
        assert!(
            received.lock().unwrap().iter().any(|event| matches!(
                event, SessionEvent::Notice { notice } if notice.level == parlance_protocol::NoticeLevel::Warning
            )),
            "unfinished final translations must be reported to the user"
        );
    }

    struct GatedFinal {
        entered: tokio::sync::mpsc::UnboundedSender<String>,
        permit: tokio::sync::Semaphore,
    }

    #[async_trait]
    impl Translator for GatedFinal {
        async fn translate(
            &self,
            request: TranslationRequest,
            _: CancellationToken,
            _: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            self.entered.send(request.text.clone()).unwrap();
            self.permit.acquire().await.unwrap().forget();
            Ok(request.text)
        }
    }

    #[tokio::test]
    async fn session_end_from_os_thread_drains_healthy_finals_and_releases_sink() {
        let (sink, log) = recorder();
        let retained = Arc::downgrade(&sink);
        let (entered, mut received) = tokio::sync::mpsc::unbounded_channel();
        let translator = Arc::new(GatedFinal { entered, permit: tokio::sync::Semaphore::new(0) });
        let worker = TranslationWorker::spawn(translator.clone(), LanguageCode::new("es").unwrap(), sink);
        for id in 1..=3 {
            worker.on_segment(&segment(id, 1, &format!("final {id}"), "en", Stability::Final));
        }
        std::thread::spawn(move || worker.on_session_end()).join().unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            for id in 1..=3 {
                assert_eq!(received.recv().await.unwrap(), format!("final {id}"));
                translator.permit.add_permits(1);
            }
            while retained.upgrade().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let finals: Vec<_> = log.lock().unwrap().iter().filter(|t| t.is_final).map(|t| t.text.clone()).collect();
        assert_eq!(finals, ["final 1", "final 2", "final 3"]);
    }

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

    #[test]
    fn language_correction_is_not_deduplicated_when_words_are_unchanged() {
        let slot = LatestPartial::default();
        slot.offer(Job::from_segment(&segment(1, 0, "hola", "en", Stability::Partial)));
        slot.offer(Job::from_segment(&segment(1, 1, "hola", "es", Stability::Partial)));
        let corrected = slot.take().unwrap();
        assert_eq!(corrected.revision, 1);
        assert_eq!(corrected.language.unwrap().as_str(), "es");
    }

    struct FailsFirst(std::sync::atomic::AtomicBool);

    #[async_trait]
    impl Translator for FailsFirst {
        async fn translate(
            &self,
            request: TranslationRequest,
            _: CancellationToken,
            on_token: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            if !self.0.swap(true, Ordering::SeqCst) {
                on_token("al aeropuerto al aeropuerto ");
                return Err(TranslationError::Model(parlance_llm::LlmError::Request("failed generation".into())));
            }
            Ok(format!("clean context={}", request.context.len()))
        }
    }

    #[tokio::test]
    async fn clears_rejected_streamed_text_and_does_not_remember_it() {
        let (sink, log) = recorder();
        let worker = TranslationWorker::spawn(
            Arc::new(FailsFirst(AtomicBool::new(false))),
            LanguageCode::new("es").unwrap(),
            sink,
        );
        worker.on_segment(&segment(1, 1, "first", "en", Stability::Final));
        worker.on_segment(&segment(2, 1, "next", "en", Stability::Final));
        worker.on_session_end();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if log.lock().unwrap().iter().any(|t| t.segment_id == SegmentId(2) && t.is_final) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let log = log.lock().unwrap();
        let first = log.iter().rev().find(|t| t.segment_id == SegmentId(1)).unwrap();
        assert!(first.text.is_empty(), "failed provisional output must not remain visible");
        assert!(first.is_final);
        assert_eq!(log.last().unwrap().text, "clean context=0");
    }

    struct SlowPreview;

    #[async_trait]
    impl Translator for SlowPreview {
        async fn translate(
            &self,
            request: TranslationRequest,
            _: CancellationToken,
            on_token: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            let words = if request.text == "short correction" {
                vec!["short"]
            } else if request.text == "first" {
                vec!["one ", "two ", "three"]
            } else {
                vec!["one ", "two ", "three ", "four"]
            };
            for word in &words {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                on_token(word);
            }
            Ok(words.concat())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn newer_partial_does_not_erase_the_visible_translation_as_it_streams() {
        let (sink, log) = recorder();
        let worker = TranslationWorker::spawn(Arc::new(SlowPreview), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 0, "first", "en", Stability::Partial));
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        assert_eq!(log.lock().unwrap().last().unwrap().text, "one two three");
        worker.on_segment(&segment(1, 1, "next", "en", Stability::Partial));
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert_eq!(log.lock().unwrap().last().unwrap().text, "one two three");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(log.lock().unwrap().last().unwrap().text, "one two three four");
        worker.on_session_end();
    }

    #[tokio::test(start_paused = true)]
    async fn continuous_same_segment_updates_allow_progress_before_any_pause() {
        let (sink, log) = recorder();
        let worker = TranslationWorker::spawn(Arc::new(SlowPreview), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 0, "first", "en", Stability::Partial));
        for revision in 1..=3 {
            tokio::time::sleep(std::time::Duration::from_millis(110)).await;
            worker.on_segment(&segment(1, revision, "next", "en", Stability::Partial));
        }
        assert!(log.lock().unwrap().iter().any(|t| t.source_revision == 0 && t.text == "one two three"));
        worker.on_session_end();
    }

    #[tokio::test(start_paused = true)]
    async fn completed_corrections_can_replace_a_longer_preview() {
        let (sink, log) = recorder();
        let worker = TranslationWorker::spawn(Arc::new(SlowPreview), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 0, "first", "en", Stability::Partial));
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        worker.on_segment(&segment(1, 1, "short correction", "en", Stability::Final));
        worker.on_session_end();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let log = log.lock().unwrap();
        assert_eq!(log.last().unwrap().text, "short");
        assert!(log.last().unwrap().is_final);
    }

    struct BlocksOld {
        entered: Notify,
        token: Mutex<Option<CancellationToken>>,
    }

    #[async_trait]
    impl Translator for BlocksOld {
        async fn translate(
            &self,
            request: TranslationRequest,
            cancel: CancellationToken,
            _: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            if request.text == "old" {
                *self.token.lock().unwrap() = Some(cancel.clone());
                self.entered.notify_one();
                cancel.cancelled().await;
                return Err(parlance_llm::LlmError::Cancelled.into());
            }
            Ok(request.text)
        }
    }

    #[tokio::test]
    async fn a_final_preempts_its_obsolete_partial_without_a_failure_notice() {
        let (sink, log) = recorder();
        let notices = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = notices.clone();
        let events: EventSink = Arc::new(move |e| {
            if matches!(e, SessionEvent::Notice { .. }) {
                count.fetch_add(1, Ordering::SeqCst);
            }
            sink(e);
        });
        let translator = Arc::new(BlocksOld { entered: Notify::new(), token: Mutex::new(None) });
        let worker = TranslationWorker::spawn(translator.clone(), LanguageCode::new("es").unwrap(), events);
        worker.on_segment(&segment(1, 0, "old", "en", Stability::Partial));
        translator.entered.notified().await;
        worker.on_segment(&segment(1, 1, "complete", "en", Stability::Final));
        tokio::time::timeout(std::time::Duration::from_millis(200), async {
            loop {
                if log.lock().unwrap().iter().any(|t| t.is_final && t.text == "complete") {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("final must not wait for the obsolete generation");
        assert!(translator.token.lock().unwrap().as_ref().unwrap().is_cancelled());
        assert_eq!(notices.load(Ordering::SeqCst), 0);
        worker.on_session_end();
    }

    #[tokio::test]
    async fn a_new_segment_preempts_an_obsolete_partial() {
        let (sink, log) = recorder();
        let translator = Arc::new(BlocksOld { entered: Notify::new(), token: Mutex::new(None) });
        let worker = TranslationWorker::spawn(translator.clone(), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 0, "old", "en", Stability::Partial));
        translator.entered.notified().await;
        worker.on_segment(&segment(2, 0, "current", "en", Stability::Partial));
        tokio::time::timeout(std::time::Duration::from_millis(200), async {
            while !log.lock().unwrap().iter().any(|t| t.segment_id == SegmentId(2)) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(translator.token.lock().unwrap().as_ref().unwrap().is_cancelled());
        worker.on_session_end();
    }

    #[tokio::test]
    async fn an_empty_final_cancels_a_retracted_partial() {
        let (sink, _) = recorder();
        let translator = Arc::new(BlocksOld { entered: Notify::new(), token: Mutex::new(None) });
        let worker = TranslationWorker::spawn(translator.clone(), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 0, "old", "en", Stability::Partial));
        translator.entered.notified().await;
        worker.on_segment(&segment(1, 1, "", "en", Stability::Final));
        tokio::time::timeout(std::time::Duration::from_millis(200), async {
            while !translator.token.lock().unwrap().as_ref().unwrap().is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        worker.on_session_end();
    }

    struct FailsFinal;

    #[async_trait]
    impl Translator for FailsFinal {
        async fn translate(
            &self,
            request: TranslationRequest,
            _: CancellationToken,
            on_token: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            if request.text == "partial" {
                return Ok("useful preview".into());
            }
            on_token("rejected output");
            Err(parlance_llm::LlmError::Request("generation failed".into()).into())
        }
    }

    #[tokio::test]
    async fn failed_final_preserves_the_preview_without_claiming_it_is_final() {
        let (sink, log) = recorder();
        let worker = TranslationWorker::spawn(Arc::new(FailsFinal), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 0, "partial", "en", Stability::Partial));
        tokio::time::timeout(std::time::Duration::from_millis(200), async {
            while log.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        worker.on_segment(&segment(1, 1, "final", "en", Stability::Final));
        worker.on_session_end();
        tokio::time::timeout(std::time::Duration::from_millis(200), async {
            while !log.lock().unwrap().iter().any(|t| t.source_revision == 1) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let log = log.lock().unwrap();
        assert_eq!(log.last().unwrap().text, "useful preview");
        assert!(!log.last().unwrap().is_final);
        assert!(!log.iter().any(|t| t.text.contains("rejected")));
    }

    #[tokio::test]
    async fn session_end_drains_ordered_finals_after_preempting_a_partial() {
        let (sink, log) = recorder();
        let translator = Arc::new(BlocksOld { entered: Notify::new(), token: Mutex::new(None) });
        let worker = TranslationWorker::spawn(translator.clone(), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 0, "old", "en", Stability::Partial));
        translator.entered.notified().await;
        for id in 1..=3 {
            worker.on_segment(&segment(id, 1, &format!("final {id}"), "en", Stability::Final));
        }
        worker.on_session_end();
        tokio::time::timeout(std::time::Duration::from_millis(200), async {
            while log.lock().unwrap().iter().filter(|t| t.is_final).count() < 3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let finals: Vec<_> = log.lock().unwrap().iter().filter(|t| t.is_final).map(|t| t.text.clone()).collect();
        assert_eq!(finals, ["final 1", "final 2", "final 3"]);
        assert!(translator.token.lock().unwrap().as_ref().unwrap().is_cancelled());
    }

    struct Ordered(Mutex<Vec<String>>);

    #[async_trait]
    impl Translator for Ordered {
        async fn translate(
            &self,
            request: TranslationRequest,
            _: CancellationToken,
            _: &OnToken<'_>,
        ) -> Result<String, TranslationError> {
            self.0.lock().unwrap().push(request.text.clone());
            Ok(request.text)
        }
    }

    #[tokio::test]
    async fn pending_finals_do_not_starve_the_current_live_partial() {
        let (sink, _) = recorder();
        let translator = Arc::new(Ordered(Mutex::new(Vec::new())));
        let worker = TranslationWorker::spawn(translator.clone(), LanguageCode::new("es").unwrap(), sink);
        worker.on_segment(&segment(1, 1, "final one", "en", Stability::Final));
        worker.on_segment(&segment(2, 1, "final two", "en", Stability::Final));
        worker.on_segment(&segment(3, 0, "live", "en", Stability::Partial));
        tokio::time::timeout(std::time::Duration::from_millis(200), async {
            while translator.0.lock().unwrap().len() < 3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(*translator.0.lock().unwrap(), ["final one", "live", "final two"]);
        worker.on_session_end();
    }
}
