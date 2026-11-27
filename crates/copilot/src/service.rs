use crate::prompt::{PROMPT_BYTES, build_prompt};
use crate::{SuggestionPolicy, is_question, should_suggest};
use parlance_domain::{
    LanguageCode, ProfileAttribution, ProfileSnapshot, SegmentId, Suggestion, SuggestionId, SuggestionOrigin,
    SuggestionStatus, TranscriptSegment,
};
use parlance_llm::prompt::clean_completion;
use parlance_llm::{ChatRequest, LanguageModel};
use parlance_protocol::{AskCopilotRequest, Notice, SessionEvent};
use parlance_session::{EventSink, SegmentListener};
use parlance_translation::{TranslationRequest, Translator};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// User input limits are measured in UTF-8 bytes, matching the prompt budget.
const QUESTION_BYTES: usize = 2_000;
const IDEA_BYTES: usize = 1_000;
/// Prevent unbounded request identifiers without constraining client UUID formatting.
const REQUEST_ID_BYTES: usize = 128;
/// Bound ASR handoff even when the model or IPC consumer stops making progress.
const SEGMENT_QUEUE_CAPACITY: usize = 64;

#[derive(Debug, Clone)]
pub struct CopilotPreferences {
    pub enabled: bool,
    pub source_language: Option<LanguageCode>,
    pub user_language: LanguageCode,
}
#[derive(Debug, thiserror::Error)]
pub enum CopilotError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error(
        "The question, idea and project context exceed the available capacity. Reduce the supplied text and retry."
    )]
    ContextTooLarge,
    #[error("The copilot is shutting down. Try again after the model is ready.")]
    Closed,
}
pub type ActiveProfile = Arc<dyn Fn() -> Option<ProfileSnapshot> + Send + Sync>;

struct Job {
    suggestion: Mutex<Suggestion>,
    prompt: ChatRequest,
    cancel: CancellationToken,
    epoch: Option<u64>,
}
struct State {
    active: Option<Arc<Job>>,
    pending: Option<Arc<Job>>,
    next_id: u64,
    epoch: u64,
    session_open: bool,
    closed: bool,
    preferences: CopilotPreferences,
    conversation: VecDeque<String>,
    conversation_bytes: usize,
    conversation_trimmed: bool,
    last_suggestion: Option<Instant>,
}
struct SegmentInput {
    segment: TranscriptSegment,
    profile: Option<ProfileSnapshot>,
    epoch: u64,
}
struct Inner {
    incoming: Mutex<VecDeque<SegmentInput>>,
    incoming_dropped: AtomicUsize,
    current_epoch: AtomicU64,
    ended_epoch: AtomicU64,
    closed: AtomicBool,
    state: Mutex<State>,
    wake: Notify,
    stopped: CancellationToken,
    model: Arc<dyn LanguageModel>,
    translator: Arc<dyn Translator>,
    events: EventSink,
    policy: SuggestionPolicy,
}
/// Application-owned generation queue; session adapters only supply conversation.
pub struct CopilotService {
    inner: Arc<Inner>,
    task: Mutex<Option<JoinHandle<()>>>,
}
impl std::fmt::Debug for CopilotService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopilotService").finish_non_exhaustive()
    }
}
impl CopilotService {
    /// Construct inside the Tokio runtime. The worker serializes model operations.
    pub fn new(
        model: Arc<dyn LanguageModel>,
        translator: Arc<dyn Translator>,
        events: EventSink,
        policy: SuggestionPolicy,
        preferences: CopilotPreferences,
    ) -> Arc<Self> {
        let inner = Arc::new(Inner {
            incoming: Mutex::new(VecDeque::with_capacity(SEGMENT_QUEUE_CAPACITY)),
            incoming_dropped: AtomicUsize::new(0),
            current_epoch: AtomicU64::new(0),
            ended_epoch: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            state: Mutex::new(State {
                active: None,
                pending: None,
                next_id: 1,
                epoch: 0,
                session_open: false,
                closed: false,
                preferences,
                conversation: VecDeque::new(),
                conversation_bytes: 0,
                conversation_trimmed: false,
                last_suggestion: None,
            }),
            wake: Notify::new(),
            stopped: CancellationToken::new(),
            model,
            translator,
            events,
            policy,
        });
        let task = tokio::spawn(run(inner.clone()));
        Arc::new(Self { inner, task: Mutex::new(Some(task)) })
    }
    pub fn submit_manual(
        &self,
        request: AskCopilotRequest,
        profile: Option<ProfileSnapshot>,
    ) -> Result<(), CopilotError> {
        validate(&request)?;
        if request.profile_id.as_deref() != profile.as_ref().map(|p| p.id.as_str()) {
            return Err(CopilotError::InvalidRequest("The selected project profile is no longer available.".into()));
        }
        self.inner.drain_segments();
        let mut state = self.inner.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.closed {
            return Err(CopilotError::Closed);
        }
        if [&state.active, &state.pending].iter().any(|job| {
            job.as_ref().is_some_and(|j| {
                j.suggestion.lock().unwrap_or_else(|p| p.into_inner()).request_id == request.request_id
            })
        }) {
            return Err(CopilotError::InvalidRequest("This request is already active.".into()));
        }
        let job = make_job(&mut state, request, profile, None, None)?;
        if let Some(previous) = state.active.take() {
            self.inner.cancel(&previous);
        }
        self.inner.emit(&job);
        state.active = Some(job);
        self.inner.wake.notify_one();
        Ok(())
    }
    pub fn cancel_request(&self, request_id: &str) {
        let mut state = self.inner.state.lock().unwrap_or_else(|p| p.into_inner());
        if state
            .pending
            .as_ref()
            .is_some_and(|j| j.suggestion.lock().unwrap_or_else(|p| p.into_inner()).request_id == request_id)
            && let Some(job) = state.pending.take()
        {
            self.inner.cancel(&job);
        }
        if state
            .active
            .as_ref()
            .is_some_and(|j| j.suggestion.lock().unwrap_or_else(|p| p.into_inner()).request_id == request_id)
        {
            if let Some(job) = state.active.take() {
                self.inner.cancel(&job);
            }
            state.active = state.pending.take();
            self.inner.wake.notify_one();
        }
    }
    pub fn update_preferences(&self, preferences: CopilotPreferences) {
        self.inner.drain_segments();
        let mut state = self.inner.state.lock().unwrap_or_else(|p| p.into_inner());
        state.preferences = preferences;
        if !state.preferences.enabled {
            self.inner.cancel_automatic(&mut state);
        }
    }
    pub fn session_listener(self: &Arc<Self>, active_profile: ActiveProfile) -> Arc<dyn SegmentListener> {
        let mut state = self.inner.state.lock().unwrap_or_else(|p| p.into_inner());
        self.inner.cancel_automatic(&mut state);
        state.epoch += 1;
        self.inner.current_epoch.store(state.epoch, Ordering::Release);
        self.inner.incoming.lock().unwrap_or_else(|p| p.into_inner()).clear();
        self.inner.incoming_dropped.store(0, Ordering::Release);
        state.session_open = true;
        state.conversation.clear();
        state.conversation_bytes = 0;
        state.conversation_trimmed = false;
        state.last_suggestion = None;
        Arc::new(SessionAdapter { service: self.clone(), epoch: state.epoch, active_profile })
    }
    pub fn stop_session(&self) {
        self.inner.end_session(self.inner.current_epoch.load(Ordering::Acquire));
    }
    pub fn cancel_all(&self) {
        let mut state = self.inner.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(job) = state.active.take() {
            self.inner.cancel(&job);
        }
        if let Some(job) = state.pending.take() {
            self.inner.cancel(&job);
        }
        self.inner.wake.notify_one();
    }
    pub async fn shutdown(&self) {
        self.close();
        let task = self.task.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
    fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
        let mut state = self.inner.state.lock().unwrap_or_else(|p| p.into_inner());
        state.closed = true;
        if let Some(job) = state.active.take() {
            self.inner.cancel(&job);
        }
        if let Some(job) = state.pending.take() {
            self.inner.cancel(&job);
        }
        self.inner.stopped.cancel();
        self.inner.wake.notify_one();
    }
}
impl Drop for CopilotService {
    fn drop(&mut self) {
        self.close();
    }
}

fn validate(request: &AskCopilotRequest) -> Result<(), CopilotError> {
    if request.request_id.trim().is_empty() || request.request_id.len() > REQUEST_ID_BYTES {
        return Err(CopilotError::InvalidRequest("A valid request identifier is required.".into()));
    }
    if request.question.trim().is_empty() || request.question.len() > QUESTION_BYTES {
        return Err(CopilotError::InvalidRequest("Enter a question of at most 2000 UTF-8 bytes.".into()));
    }
    if request.idea.len() > IDEA_BYTES {
        return Err(CopilotError::InvalidRequest("The optional idea may contain at most 1000 UTF-8 bytes.".into()));
    }
    Ok(())
}
fn make_job(
    state: &mut State,
    request: AskCopilotRequest,
    profile: Option<ProfileSnapshot>,
    segment: Option<SegmentId>,
    epoch: Option<u64>,
) -> Result<Arc<Job>, CopilotError> {
    let conversation = state.conversation.iter().cloned().collect::<Vec<_>>();
    let (prompt, trimmed) = build_prompt(&request, profile.as_ref(), &conversation, state.conversation_trimmed)?;
    let id = SuggestionId(state.next_id);
    state.next_id += 1;
    Ok(Arc::new(Job {
        suggestion: Mutex::new(Suggestion {
            id,
            question_segment_id: segment,
            request_id: request.request_id,
            origin: if epoch.is_some() { SuggestionOrigin::Automatic } else { SuggestionOrigin::Manual },
            question: request.question,
            idea: request.idea,
            profile: profile.as_ref().map(ProfileAttribution::from),
            status: SuggestionStatus::Queued,
            error: None,
            conversation_truncated: trimmed,
            speaker_language: Some(request.reply_language),
            user_language: request.user_language,
            reply_in_speaker_language: String::new(),
            reply_in_user_language: String::new(),
            is_complete: false,
        }),
        prompt,
        cancel: CancellationToken::new(),
        epoch,
    }))
}
impl Inner {
    fn emit(&self, job: &Job) {
        (self.events)(SessionEvent::Suggestion {
            suggestion: job.suggestion.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        });
    }
    fn cancel(&self, job: &Job) {
        job.cancel.cancel();
        let mut suggestion = job.suggestion.lock().unwrap_or_else(|p| p.into_inner());
        if !matches!(suggestion.status, SuggestionStatus::Queued | SuggestionStatus::Generating) {
            return;
        }
        suggestion.status = SuggestionStatus::Cancelled;
        suggestion.is_complete = false;
        suggestion.error = None;
        (self.events)(SessionEvent::Suggestion { suggestion: suggestion.clone() });
    }
    fn cancel_automatic(&self, state: &mut State) {
        if let Some(job) = state.pending.take() {
            self.cancel(&job);
        }
        if state.active.as_ref().is_some_and(|j| j.epoch.is_some())
            && let Some(job) = state.active.take()
        {
            self.cancel(&job);
        }
        self.wake.notify_one();
    }
    fn update(&self, job: &Arc<Job>, update: impl FnOnce(&mut Suggestion)) {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.closed
            || job.cancel.is_cancelled()
            || job.epoch.is_some_and(|epoch| {
                self.ended_epoch.load(Ordering::Acquire) >= epoch || self.current_epoch.load(Ordering::Acquire) != epoch
            })
            || !state.active.as_ref().is_some_and(|active| Arc::ptr_eq(active, job))
        {
            return;
        }
        let mut suggestion = job.suggestion.lock().unwrap_or_else(|p| p.into_inner());
        if !matches!(suggestion.status, SuggestionStatus::Queued | SuggestionStatus::Generating) {
            return;
        }
        update(&mut suggestion);
        (self.events)(SessionEvent::Suggestion { suggestion: suggestion.clone() });
    }
}
struct SessionAdapter {
    service: Arc<CopilotService>,
    epoch: u64,
    active_profile: ActiveProfile,
}
impl std::fmt::Debug for SessionAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopilotSession").field("epoch", &self.epoch).finish_non_exhaustive()
    }
}
impl SegmentListener for SessionAdapter {
    fn on_segment(&self, segment: &TranscriptSegment) {
        let inner = &self.service.inner;
        if !segment.is_final()
            || inner.closed.load(Ordering::Acquire)
            || inner.current_epoch.load(Ordering::Acquire) != self.epoch
            || inner.ended_epoch.load(Ordering::Acquire) >= self.epoch
        {
            return;
        }
        let input = SegmentInput { segment: segment.clone(), profile: (self.active_profile)(), epoch: self.epoch };
        if let Ok(mut incoming) = inner.incoming.try_lock() {
            if incoming.len() == SEGMENT_QUEUE_CAPACITY {
                incoming.pop_front();
                inner.incoming_dropped.fetch_add(1, Ordering::Relaxed);
            }
            incoming.push_back(input);
        } else {
            inner.incoming_dropped.fetch_add(1, Ordering::Relaxed);
        }
        inner.wake.notify_one();
    }
    fn on_session_end(&self) {
        self.service.inner.end_session(self.epoch);
    }
}
impl Inner {
    fn end_session(&self, epoch: u64) {
        self.ended_epoch.fetch_max(epoch, Ordering::AcqRel);
        self.wake.notify_one();
    }
    fn drain_segments(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.closed {
            return;
        }
        // Release the handoff lock before any prompt construction or event delivery.
        let inputs = {
            let mut incoming = self.incoming.lock().unwrap_or_else(|p| p.into_inner());
            incoming.drain(..).collect::<Vec<_>>()
        };
        let dropped = self.incoming_dropped.swap(0, Ordering::AcqRel);
        if dropped > 0 {
            state.conversation_trimmed = true;
            (self.events)(SessionEvent::Notice {
                notice: Notice::warning(format!(
                    "Copilot skipped {dropped} conversation updates while busy. Recent context may be incomplete."
                )),
            });
        }
        for input in inputs {
            if state.session_open && state.epoch == input.epoch {
                self.accept_segment(&mut state, input);
            }
        }
        if state.session_open && self.ended_epoch.load(Ordering::Acquire) >= state.epoch {
            state.session_open = false;
            self.cancel_automatic(&mut state);
        }
    }
    fn accept_segment(&self, state: &mut State, input: SegmentInput) {
        let segment = &input.segment;
        let text = segment.text();
        if !text.trim().is_empty() {
            if text.len() > PROMPT_BYTES {
                state.conversation.clear();
                state.conversation_bytes = 0;
                state.conversation_trimmed = true;
            } else {
                state.conversation_bytes += text.len();
                state.conversation.push_back(text.clone());
                while state.conversation_bytes > PROMPT_BYTES {
                    if let Some(old) = state.conversation.pop_front() {
                        state.conversation_bytes -= old.len();
                        state.conversation_trimmed = true;
                    }
                }
            }
        }
        let language = segment.language.clone().or_else(|| state.preferences.source_language.clone());
        if !state.preferences.enabled
            || !is_question(&text, language.as_ref().map(LanguageCode::as_str))
            || !should_suggest(&text, &self.policy, state.last_suggestion.map(|t| t.elapsed()))
        {
            return;
        }
        state.last_suggestion = Some(Instant::now());
        let profile = input.profile;
        let request = AskCopilotRequest {
            request_id: format!("automatic-{}", state.next_id),
            question: text,
            idea: String::new(),
            reply_language: language.unwrap_or_else(|| state.preferences.user_language.clone()),
            user_language: state.preferences.user_language.clone(),
            profile_id: profile.as_ref().map(|p| p.id.clone()),
        };
        let job = match make_job(state, request.clone(), profile.clone(), Some(segment.id), Some(input.epoch)) {
            Ok(job) => job,
            Err(error) => {
                let id = SuggestionId(state.next_id);
                state.next_id += 1;
                (self.events)(SessionEvent::Suggestion {
                    suggestion: Suggestion {
                        id,
                        question_segment_id: Some(segment.id),
                        request_id: request.request_id,
                        origin: SuggestionOrigin::Automatic,
                        idea: String::new(),
                        profile: profile.as_ref().map(ProfileAttribution::from),
                        status: SuggestionStatus::Failed,
                        error: Some(error.to_string()),
                        conversation_truncated: false,
                        question: request.question,
                        speaker_language: Some(request.reply_language),
                        user_language: request.user_language,
                        reply_in_speaker_language: String::new(),
                        reply_in_user_language: String::new(),
                        is_complete: false,
                    },
                });
                return;
            }
        };
        if state.active.as_ref().is_some_and(|j| j.epoch.is_none()) {
            if let Some(previous) = state.pending.replace(job.clone()) {
                self.cancel(&previous);
            }
        } else {
            if let Some(previous) = state.active.replace(job.clone()) {
                self.cancel(&previous);
            }
        }
        self.emit(&job);
        self.wake.notify_one();
    }
}

async fn run(inner: Arc<Inner>) {
    loop {
        let notified = inner.wake.notified();
        inner.drain_segments();
        let job = {
            let state = inner.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.closed {
                return;
            }
            state.active.clone()
        };
        let Some(job) = job else {
            tokio::select! {_ = inner.stopped.cancelled()=>return,_=notified=>continue}
        };
        inner.update(&job, |s| s.status = SuggestionStatus::Generating);
        {
            let generation = generate(&inner, &job);
            tokio::pin!(generation);
            loop {
                tokio::select! {biased;
                    _ = job.cancel.cancelled() => break,
                    _ = inner.stopped.cancelled() => return,
                    _ = inner.wake.notified() => inner.drain_segments(),
                    _ = &mut generation => break,
                }
            }
        }
        inner.drain_segments();
        let mut state = inner.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.active.as_ref().is_some_and(|active| Arc::ptr_eq(active, &job)) {
            state.active = state.pending.take();
        }
    }
}
async fn generate(inner: &Inner, job: &Arc<Job>) {
    let prompt = job.prompt.clone();
    let on_token = |token: &str| inner.update(job, |s| s.reply_in_speaker_language.push_str(token));
    let result = inner.model.generate(prompt, job.cancel.clone(), &on_token).await;
    let reply = match result {
        Ok(reply) => clean_completion(&reply),
        Err(error) => {
            inner.update(job, |s| {
                s.status = SuggestionStatus::Failed;
                s.error = Some(error.to_string());
            });
            return;
        }
    };
    if reply.is_empty() {
        inner.update(job, |s| {
            s.status = SuggestionStatus::Failed;
            s.error = Some("The model returned an empty reply. Try again.".into());
        });
        return;
    }
    inner.update(job, |s| s.reply_in_speaker_language = reply.clone());
    let snapshot = job.suggestion.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let meaning = if snapshot.speaker_language.as_ref() == Some(&snapshot.user_language) {
        Ok(String::new())
    } else {
        let request = TranslationRequest {
            text: reply,
            source: snapshot.speaker_language,
            target: snapshot.user_language,
            context: Vec::new(),
        };
        let on_token = |token: &str| inner.update(job, |s| s.reply_in_user_language.push_str(token));
        inner.translator.translate(request, job.cancel.clone(), &on_token).await.and_then(|text| {
            if text.trim().is_empty() {
                Err(parlance_llm::LlmError::Request("The model returned an empty translation.".into()).into())
            } else {
                Ok(text)
            }
        })
    };
    match meaning {
        Ok(text) => inner.update(job, |s| {
            s.reply_in_user_language = text;
            s.status = SuggestionStatus::Complete;
            s.is_complete = true;
        }),
        Err(error) => inner.update(job, |s| {
            s.status = SuggestionStatus::Failed;
            s.error = Some(error.to_string());
        }),
    }
}
