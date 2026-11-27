#![allow(clippy::unwrap_used, clippy::expect_used)]
use async_trait::async_trait;
use parlance_copilot::{CopilotError, CopilotPreferences, CopilotService, SuggestionPolicy};
use parlance_domain::{
    LanguageCode, ProfileSnapshot, SegmentId, Stability, Suggestion, SuggestionStatus, TranscriptSegment,
};
use parlance_llm::{ChatRequest, LanguageModel, LlmError, OnToken};
use parlance_protocol::{AskCopilotRequest, SessionEvent};
use parlance_session::EventSink;
use parlance_translation::{TranslationError, TranslationRequest, Translator};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct Call {
    request: ChatRequest,
    release: oneshot::Sender<Result<String, LlmError>>,
}
struct Controlled(mpsc::UnboundedSender<Call>);
#[async_trait]
impl LanguageModel for Controlled {
    async fn generate(
        &self,
        request: ChatRequest,
        _: CancellationToken,
        on_token: &OnToken<'_>,
    ) -> Result<String, LlmError> {
        let (tx, rx) = oneshot::channel();
        self.0.send(Call { request, release: tx }).unwrap();
        let result = rx.await.unwrap();
        on_token("partial");
        result
    }
}
struct Translation(bool);
#[async_trait]
impl Translator for Translation {
    async fn translate(
        &self,
        _: TranslationRequest,
        _: CancellationToken,
        _: &OnToken<'_>,
    ) -> Result<String, TranslationError> {
        if self.0 { Ok("significado".into()) } else { Err(LlmError::Request("translation failed".into()).into()) }
    }
}
fn language(code: &str) -> LanguageCode {
    LanguageCode::new(code).unwrap()
}
fn request(id: &str) -> AskCopilotRequest {
    AskCopilotRequest {
        request_id: id.into(),
        question: "How do we cache reads?".into(),
        idea: String::new(),
        reply_language: language("en"),
        user_language: language("en"),
        profile_id: None,
    }
}
fn segment(id: u64, text: &str) -> TranscriptSegment {
    TranscriptSegment {
        id: SegmentId(id),
        revision: 1,
        stable_text: text.into(),
        unstable_text: String::new(),
        language: Some(language("en")),
        start_ms: 0,
        end_ms: 0,
        stability: Stability::Final,
    }
}
struct Harness {
    service: Arc<CopilotService>,
    calls: mpsc::UnboundedReceiver<Call>,
    events: mpsc::UnboundedReceiver<Suggestion>,
}
fn harness(translation_ok: bool) -> Harness {
    let (calls, rx) = mpsc::unbounded_channel();
    let (events, erx) = mpsc::unbounded_channel();
    let sink: EventSink = Arc::new(move |e| {
        if let SessionEvent::Suggestion { suggestion } = e {
            let _ = events.send(suggestion);
        }
    });
    let service = CopilotService::new(
        Arc::new(Controlled(calls)),
        Arc::new(Translation(translation_ok)),
        sink,
        SuggestionPolicy { cooldown: std::time::Duration::ZERO, min_chars: 1 },
        CopilotPreferences { enabled: true, source_language: Some(language("en")), user_language: language("en") },
    );
    Harness { service, calls: rx, events: erx }
}
async fn next_call(h: &mut Harness) -> Call {
    tokio::time::timeout(std::time::Duration::from_secs(2), h.calls.recv()).await.unwrap().unwrap()
}
async fn terminal(h: &mut Harness, id: &str) -> Suggestion {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let event = h.events.recv().await.unwrap();
            if event.request_id == id
                && matches!(
                    event.status,
                    SuggestionStatus::Complete | SuggestionStatus::Cancelled | SuggestionStatus::Failed
                )
            {
                return event;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn manual_works_without_audio_and_same_language_does_not_translate() {
    let mut h = harness(false);
    h.service.submit_manual(request("one"), None).unwrap();
    let call = next_call(&mut h).await;
    assert_eq!(call.request.max_tokens, 160);
    call.release.send(Ok("Use Redis.".into())).unwrap();
    let result = terminal(&mut h, "one").await;
    assert_eq!(result.status, SuggestionStatus::Complete);
    assert_eq!(result.question_segment_id, None);
    assert_eq!(result.reply_in_speaker_language, "Use Redis.");
    h.service.shutdown().await;
}

#[tokio::test]
async fn manual_has_priority_and_only_latest_automatic_survives() {
    let mut h = harness(true);
    let listener = h.service.session_listener(Arc::new(|| None));
    h.service.submit_manual(request("manual"), None).unwrap();
    let manual = next_call(&mut h).await;
    listener.on_segment(&segment(1, "First technical question?"));
    listener.on_segment(&segment(2, "Latest technical question?"));
    assert!(h.calls.try_recv().is_err());
    manual.release.send(Ok("Manual response".into())).unwrap();
    assert_eq!(terminal(&mut h, "manual").await.status, SuggestionStatus::Complete);
    let auto = next_call(&mut h).await;
    assert!(auto.request.messages.last().unwrap().content.contains("Latest technical question?"));
    auto.release.send(Ok("Automatic response".into())).unwrap();
    h.service.shutdown().await;
}

#[tokio::test]
async fn replacement_cancels_uncooperative_generation_and_fences_late_output() {
    let mut h = harness(true);
    h.service.submit_manual(request("old"), None).unwrap();
    let old = next_call(&mut h).await;
    h.service.submit_manual(request("new"), None).unwrap();
    assert_eq!(terminal(&mut h, "old").await.status, SuggestionStatus::Cancelled);
    let new = next_call(&mut h).await;
    assert!(old.release.send(Ok("stale".into())).is_err(), "old future must be dropped before starting replacement");
    new.release.send(Ok("current".into())).unwrap();
    assert_eq!(terminal(&mut h, "new").await.reply_in_speaker_language, "current");
    h.service.shutdown().await;
}

#[tokio::test]
async fn translation_failure_is_terminal_and_retains_partial_reply() {
    let mut h = harness(false);
    let mut input = request("one");
    input.user_language = language("es");
    h.service.submit_manual(input, None).unwrap();
    next_call(&mut h).await.release.send(Ok("Use Redis.".into())).unwrap();
    let result = terminal(&mut h, "one").await;
    assert_eq!(result.status, SuggestionStatus::Failed);
    assert!(!result.is_complete);
    assert_eq!(result.reply_in_speaker_language, "Use Redis.");
    assert!(result.error.is_some());
    h.service.shutdown().await;
}

#[tokio::test]
async fn snapshot_is_frozen_and_capacity_rejection_never_calls_model() {
    let mut h = harness(true);
    let current = Arc::new(Mutex::new(ProfileSnapshot {
        id: "a".into(),
        name: "A".into(),
        revision: 1,
        content: "Redis".into(),
    }));
    let profiles = current.clone();
    let listener = h.service.session_listener(Arc::new(move || Some(profiles.lock().unwrap().clone())));
    listener.on_segment(&segment(1, "How are reads cached?"));
    current.lock().unwrap().content = "Postgres".into();
    current.lock().unwrap().revision = 2;
    let call = next_call(&mut h).await;
    assert!(call.request.messages[1].content.contains("Redis"));
    assert!(!call.request.messages[1].content.contains("Postgres"));
    call.release.send(Ok("Use Redis".into())).unwrap();
    let answer = terminal(&mut h, "automatic-1").await;
    let attribution = answer.profile.unwrap();
    assert_eq!(attribution.name, "A");
    assert_eq!(attribution.revision, 1);
    let mut oversized = current.lock().unwrap().clone();
    oversized.content = "界".repeat(2000);
    let mut input = request("large");
    input.profile_id = Some("a".into());
    assert!(matches!(h.service.submit_manual(input, Some(oversized)), Err(CopilotError::ContextTooLarge)));
    h.service.shutdown().await;
    assert!(h.calls.try_recv().is_err());
}

#[tokio::test]
async fn session_stop_preserves_manual_and_new_session_rejects_old_listener() {
    let mut h = harness(true);
    let old = h.service.session_listener(Arc::new(|| None));
    old.on_segment(&segment(1, "Old conversation statement."));
    h.service.submit_manual(request("manual"), None).unwrap();
    let call = next_call(&mut h).await;
    assert!(call.request.messages[1].content.contains("Old conversation"));
    old.on_session_end();
    call.release.send(Ok("manual survives".into())).unwrap();
    assert_eq!(terminal(&mut h, "manual").await.status, SuggestionStatus::Complete);
    let _new = h.service.session_listener(Arc::new(|| None));
    old.on_segment(&segment(2, "Obsolete technical question?"));
    h.service.submit_manual(request("fresh"), None).unwrap();
    let call = next_call(&mut h).await;
    assert!(!call.request.messages[1].content.contains("Old conversation"));
    assert!(!call.request.messages[1].content.contains("Obsolete"));
    call.release.send(Ok("fresh".into())).unwrap();
    terminal(&mut h, "fresh").await;
    h.service.shutdown().await;
}

#[tokio::test]
async fn disabling_automatic_preserves_manual_language_snapshot_and_discards_pending() {
    let mut h = harness(true);
    let listener = h.service.session_listener(Arc::new(|| None));
    h.service.submit_manual(request("manual"), None).unwrap();
    let call = next_call(&mut h).await;
    listener.on_segment(&segment(1, "Can we deploy?"));
    h.service.update_preferences(CopilotPreferences {
        enabled: false,
        source_language: Some(language("fr")),
        user_language: language("es"),
    });
    assert_eq!(terminal(&mut h, "automatic-2").await.status, SuggestionStatus::Cancelled);
    call.release.send(Ok("Original language".into())).unwrap();
    let result = terminal(&mut h, "manual").await;
    assert_eq!(result.user_language, language("en"));
    assert_eq!(result.status, SuggestionStatus::Complete);
    listener.on_segment(&segment(2, "Do we deploy now?"));
    h.service.shutdown().await;
    assert!(h.calls.try_recv().is_err());
}

#[tokio::test]
async fn failed_generation_and_duplicate_active_ids_have_visible_correct_states() {
    let mut h = harness(true);
    h.service.submit_manual(request("manual"), None).unwrap();
    let call = next_call(&mut h).await;
    assert!(matches!(h.service.submit_manual(request("manual"), None), Err(CopilotError::InvalidRequest(_))));
    call.release.send(Err(LlmError::Request("model rejected context".into()))).unwrap();
    let result = terminal(&mut h, "manual").await;
    assert_eq!(result.status, SuggestionStatus::Failed);
    assert!(!result.is_complete);
    assert!(result.error.unwrap().contains("model rejected"));
    h.service.shutdown().await;
    assert!(matches!(h.service.submit_manual(request("after-shutdown"), None), Err(CopilotError::Closed)));
}

#[tokio::test]
async fn stopping_session_cancels_only_its_automatic_job_and_keeps_recent_context() {
    let mut h = harness(true);
    let listener = h.service.session_listener(Arc::new(|| None));
    listener.on_segment(&segment(1, "Can you explain Redis?"));
    let automatic = next_call(&mut h).await;
    listener.on_session_end();
    assert_eq!(terminal(&mut h, "automatic-1").await.status, SuggestionStatus::Cancelled);
    h.service.submit_manual(request("followup"), None).unwrap();
    let manual = next_call(&mut h).await;
    assert!(automatic.release.send(Ok("stale".into())).is_err());
    assert!(manual.request.messages[1].content.contains("Can you explain Redis?"));
    manual.release.send(Ok("Follow up".into())).unwrap();
    terminal(&mut h, "followup").await;
    h.service.shutdown().await;
}

struct ControlledTranslation(mpsc::UnboundedSender<oneshot::Sender<()>>);
#[async_trait]
impl Translator for ControlledTranslation {
    async fn translate(
        &self,
        _: TranslationRequest,
        _: CancellationToken,
        on_token: &OnToken<'_>,
    ) -> Result<String, TranslationError> {
        let (tx, rx) = oneshot::channel();
        self.0.send(tx).unwrap();
        let _ = rx.await;
        on_token("obsolete meaning");
        Ok("obsolete meaning".into())
    }
}
#[tokio::test]
async fn cancel_during_translation_fences_meaning_and_emits_exactly_one_terminal() {
    let (calls, mut call_rx) = mpsc::unbounded_channel();
    let (translations, mut translation_rx) = mpsc::unbounded_channel();
    let events: Arc<Mutex<Vec<Suggestion>>> = Arc::default();
    let recorded = events.clone();
    let sink: EventSink = Arc::new(move |event| {
        if let SessionEvent::Suggestion { suggestion } = event {
            recorded.lock().unwrap().push(suggestion);
        }
    });
    let service = CopilotService::new(
        Arc::new(Controlled(calls)),
        Arc::new(ControlledTranslation(translations)),
        sink,
        SuggestionPolicy::default(),
        CopilotPreferences { enabled: true, source_language: None, user_language: language("es") },
    );
    let mut input = request("manual");
    input.user_language = language("es");
    service.submit_manual(input, None).unwrap();
    let call = tokio::time::timeout(std::time::Duration::from_secs(2), call_rx.recv()).await.unwrap().unwrap();
    call.release.send(Ok("Useful reply".into())).unwrap();
    let translation =
        tokio::time::timeout(std::time::Duration::from_secs(2), translation_rx.recv()).await.unwrap().unwrap();
    service.cancel_request("manual");
    service.cancel_request("manual");
    service.shutdown().await;
    assert!(translation.send(()).is_err());
    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e.status,
                SuggestionStatus::Complete | SuggestionStatus::Failed | SuggestionStatus::Cancelled
            ))
            .count(),
        1
    );
    let last = events.last().unwrap();
    assert_eq!(last.status, SuggestionStatus::Cancelled);
    assert_eq!(last.reply_in_speaker_language, "Useful reply");
    assert!(last.reply_in_user_language.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn asr_callbacks_do_not_wait_for_blocked_suggestion_delivery() {
    let (entered, entered_rx) = oneshot::channel();
    let entered = Mutex::new(Some(entered));
    let (release, release_rx) = std::sync::mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let sink: EventSink = Arc::new(move |event| {
        if let SessionEvent::Suggestion { suggestion } = event
            && suggestion.status == SuggestionStatus::Generating
            && let Some(entered) = entered.lock().unwrap().take()
        {
            let _ = entered.send(());
            release_rx.lock().unwrap().recv().unwrap();
        }
    });
    let (calls, _call_rx) = mpsc::unbounded_channel();
    let service = CopilotService::new(
        Arc::new(Controlled(calls)),
        Arc::new(Translation(true)),
        sink,
        SuggestionPolicy::default(),
        CopilotPreferences { enabled: true, source_language: Some(language("en")), user_language: language("en") },
    );
    let listener = service.session_listener(Arc::new(|| None));
    service.submit_manual(request("manual"), None).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), entered_rx).await.unwrap().unwrap();
    let (returned, returned_rx) = std::sync::mpsc::channel();
    let asr = std::thread::spawn(move || {
        listener.on_segment(&segment(1, "Can you explain Redis?"));
        listener.on_session_end();
        returned.send(()).unwrap();
    });
    let responsive = returned_rx.recv_timeout(std::time::Duration::from_millis(100)).is_ok();
    release.send(()).unwrap();
    asr.join().unwrap();
    service.shutdown().await;
    assert!(responsive, "ASR segment/end callbacks must return while suggestion delivery is blocked");
}

#[tokio::test]
async fn bounded_handoff_keeps_recent_context_and_processes_end_after_queued_segments() {
    let mut h = harness(true);
    let listener = h.service.session_listener(Arc::new(|| None));
    // No await: the consumer cannot drain this burst until manual admission below.
    for id in 0..128 {
        listener.on_segment(&segment(id, &format!("Line {id:03}.")));
    }
    listener.on_segment(&segment(128, "What about the latest context?"));
    listener.on_session_end();
    listener.on_segment(&segment(129, "This ended-session text must be discarded."));
    h.service.submit_manual(request("after-end"), None).unwrap();
    let call = next_call(&mut h).await;
    let context = &call.request.messages[1].content;
    assert!(!context.contains("Line 000."));
    assert!(!context.contains("Line 064."));
    assert!(context.contains("Line 065."));
    assert!(context.contains("Line 127."));
    assert!(context.contains("What about the latest context?"));
    assert!(!context.contains("ended-session text"));
    call.release.send(Ok("Context retained".into())).unwrap();
    let result = terminal(&mut h, "after-end").await;
    assert!(result.conversation_truncated);
    assert_eq!(result.status, SuggestionStatus::Complete);
    h.service.shutdown().await;
    assert!(h.calls.try_recv().is_err(), "ended session must not start its queued automatic generation");
}
