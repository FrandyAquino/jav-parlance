//! End-to-end pipeline tests with a replayed source and a scripted recognizer.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers may panic

use std::sync::{Arc, Mutex};
use std::time::Duration;

use parlance_asr::{AsrError, DecodeOptions, Hypothesis, SpeechRecognizer};
use parlance_audio::StreamFormat;
use parlance_audio::vad::{EnergyVad, VoiceActivityDetector};
use parlance_capture::replay::{ReplayBackend, ReplayClip};
use parlance_domain::{SessionState, SourceId, TranscriptSegment};
use parlance_protocol::SessionEvent;
use parlance_session::{EventSink, SegmentListener, SessionConfig, SessionController, SpeechEngine};

struct EchoRecognizer;

impl SpeechRecognizer for EchoRecognizer {
    fn transcribe(&mut self, audio: &[f32], _options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
        let seconds = audio.len() / 16_000;
        Ok(Hypothesis { text: vec!["word"; seconds.max(1)].join(" "), language: None })
    }
}

struct FakeEngine;

impl SpeechEngine for FakeEngine {
    fn recognizer(&self) -> Result<Box<dyn SpeechRecognizer>, AsrError> {
        Ok(Box::new(EchoRecognizer))
    }
    fn voice_detector(&self) -> Result<Box<dyn VoiceActivityDetector>, AsrError> {
        Ok(Box::new(EnergyVad::default()))
    }
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<SessionEvent>>,
    finals: Mutex<Vec<TranscriptSegment>>,
}

impl SegmentListener for Recorder {
    fn on_segment(&self, segment: &TranscriptSegment) {
        if segment.is_final() {
            self.finals.lock().unwrap().push(segment.clone());
        }
    }
}

/// 48 kHz stereo: 0.5 s silence, `speech_s` of tone, 1 s silence.
fn clip(speech_s: f32) -> ReplayClip {
    let rate = 48_000.0;
    let mut samples = vec![0.0; (rate * 0.5) as usize * 2];
    for i in 0..(rate * speech_s) as usize {
        let value = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / rate).sin();
        samples.extend([value, value]);
    }
    samples.extend(vec![0.0; rate as usize * 2]);
    ReplayClip {
        name: "tone".into(),
        format: StreamFormat { sample_rate: 48_000, channels: 2 },
        samples: Arc::new(samples),
    }
}

fn controller(backend: ReplayBackend, recorder: &Arc<Recorder>) -> SessionController {
    let sink_recorder = Arc::clone(recorder);
    let sink: EventSink = Arc::new(move |event| sink_recorder.events.lock().unwrap().push(event));
    SessionController::new(Arc::new(backend), Arc::new(FakeEngine), sink)
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    for _ in 0..400 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not met in time");
}

#[tokio::test(flavor = "multi_thread")]
async fn replayed_speech_produces_final_segment_and_session_auto_stops() {
    let recorder = Arc::new(Recorder::default());
    let controller = controller(ReplayBackend::new(None).with_clip("tone", clip(2.0)), &recorder);
    let config = SessionConfig::new(vec![SourceId("tone".into())], None);
    let listener: Arc<dyn SegmentListener> = Arc::clone(&recorder) as Arc<dyn SegmentListener>;

    controller.start(config, vec![listener]).await.unwrap();
    wait_until(|| controller.state() == SessionState::Idle).await;

    let finals = recorder.finals.lock().unwrap().clone();
    assert_eq!(finals.len(), 1, "one utterance expected: {finals:?}");
    assert!(finals[0].stable_text.starts_with("word"));
    assert!((250..=500).contains(&finals[0].start_ms), "start {}", finals[0].start_ms);

    let events = recorder.events.lock().unwrap();
    let states: Vec<_> = events
        .iter()
        .filter_map(|e| if let SessionEvent::StateChanged { state } = e { Some(state.clone()) } else { None })
        .collect();
    assert_eq!(states, [SessionState::Starting, SessionState::Running, SessionState::Stopping, SessionState::Idle]);
    assert!(events.iter().any(|e| matches!(e, SessionEvent::Level { .. })));
}

#[tokio::test(flavor = "multi_thread")]
async fn manual_stop_finalizes_open_utterance() {
    let recorder = Arc::new(Recorder::default());
    let backend = ReplayBackend::new(Some(1.0)).with_clip("tone", clip(30.0));
    let controller = controller(backend, &recorder);
    let listener: Arc<dyn SegmentListener> = Arc::clone(&recorder) as Arc<dyn SegmentListener>;

    controller.start(SessionConfig::new(vec![SourceId("tone".into())], None), vec![listener]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    controller.stop().await.unwrap();

    assert_eq!(controller.state(), SessionState::Idle);
    assert_eq!(recorder.finals.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_source_fails_session_with_actionable_error() {
    let recorder = Arc::new(Recorder::default());
    let controller = controller(ReplayBackend::new(None), &recorder);
    let error = controller.start(SessionConfig::new(vec![SourceId("missing".into())], None), vec![]).await.unwrap_err();
    assert!(error.to_string().contains("missing"));
    assert!(matches!(controller.state(), SessionState::Failed { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_double_start_and_stop_when_idle() {
    let recorder = Arc::new(Recorder::default());
    let controller = controller(ReplayBackend::new(Some(1.0)).with_clip("tone", clip(30.0)), &recorder);
    assert!(controller.stop().await.is_err());
    controller.start(SessionConfig::new(vec![SourceId("tone".into())], None), vec![]).await.unwrap();
    assert!(controller.start(SessionConfig::new(vec![SourceId("tone".into())], None), vec![]).await.is_err());
    controller.pause().unwrap();
    controller.resume().unwrap();
    controller.stop().await.unwrap();
}
