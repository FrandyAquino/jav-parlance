use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use parlance_asr::{SpeechRecognizer, StreamingTranscriber};
use parlance_audio::vad::GateEvent;
use parlance_domain::{LanguageCode, TranscriptSegment};
use parlance_protocol::{Notice, SessionEvent};

use crate::ports::{EventSink, SegmentListener};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct AsrWorker<R> {
    transcriber: StreamingTranscriber<R>,
    input: Receiver<GateEvent>,
    events: EventSink,
    listeners: Vec<Arc<dyn SegmentListener>>,
    last_language: Option<LanguageCode>,
}

impl<R: SpeechRecognizer> AsrWorker<R> {
    pub fn new(
        transcriber: StreamingTranscriber<R>,
        input: Receiver<GateEvent>,
        events: EventSink,
        listeners: Vec<Arc<dyn SegmentListener>>,
    ) -> Self {
        Self { transcriber, input, events, listeners, last_language: None }
    }

    /// Runs until the DSP stage hangs up, then finalizes what is left.
    pub fn run(mut self) {
        loop {
            match self.input.recv_timeout(POLL_INTERVAL) {
                Ok(event) => self.transcriber.push(event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            // Drain the backlog before decoding: one decode over all pending
            // audio beats several stale ones when inference is slow.
            while let Ok(event) = self.input.try_recv() {
                self.transcriber.push(event);
            }
            if self.transcriber.has_work() {
                let result = self.transcriber.poll();
                self.publish(result);
            }
        }
        let result = self.transcriber.flush();
        self.publish(result);
        while self.transcriber.has_work() {
            let result = self.transcriber.poll();
            self.publish(result);
        }
        for listener in &self.listeners {
            listener.on_session_end();
        }
    }

    fn publish(&mut self, result: Result<Vec<TranscriptSegment>, parlance_asr::AsrError>) {
        match result {
            Ok(segments) => segments.into_iter().for_each(|segment| self.emit(segment)),
            Err(error) => {
                tracing::error!(%error, "transcription failed");
                (self.events)(SessionEvent::Notice { notice: Notice::error(error.to_string()) });
            }
        }
    }

    fn emit(&mut self, segment: TranscriptSegment) {
        if segment.is_final()
            && let Some(language) = segment.language.clone()
            && self.last_language.as_ref() != Some(&language)
        {
            self.last_language = Some(language.clone());
            (self.events)(SessionEvent::LanguageDetected { language });
        }
        (self.events)(SessionEvent::Transcript { segment: segment.clone() });
        for listener in &self.listeners {
            listener.on_segment(&segment);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Mutex, mpsc};

    use parlance_asr::{AsrError, DecodeOptions, Hypothesis, StreamingConfig};
    use parlance_domain::{SegmentId, Stability};

    struct ScriptedRecognizer(VecDeque<Result<&'static str, &'static str>>);

    impl SpeechRecognizer for ScriptedRecognizer {
        fn transcribe(&mut self, _audio: &[f32], _options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
            self.0
                .pop_front()
                .expect("one scripted result per decode")
                .map(|text| Hypothesis { text: text.into(), language: None })
                .map_err(|error| AsrError::Inference(error.into()))
        }
    }

    struct RecordingListener(Arc<Mutex<Vec<String>>>);

    impl SegmentListener for RecordingListener {
        fn on_segment(&self, segment: &TranscriptSegment) {
            self.0.lock().unwrap().push(format!("listener:{}", segment.id.0));
        }

        fn on_session_end(&self) {
            self.0.lock().unwrap().push("end".into());
        }
    }

    fn event_sink(log: Arc<Mutex<Vec<String>>>) -> EventSink {
        Arc::new(move |event| match event {
            SessionEvent::Transcript { segment } => log.lock().unwrap().push(format!("transcript:{}", segment.id.0)),
            SessionEvent::Notice { .. } => log.lock().unwrap().push("error".into()),
            _ => {}
        })
    }

    #[test]
    fn disconnect_drains_all_successes_and_errors_before_listener_shutdown() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let recognizer = ScriptedRecognizer(VecDeque::from([Ok("one"), Err("two"), Err("three"), Ok("four")]));
        let mut transcriber = StreamingTranscriber::new(recognizer, StreamingConfig::default());
        for index in 0..4 {
            transcriber.push(GateEvent::SpeechStarted { start_sample: index * 16_000 });
            transcriber.push(GateEvent::Speech(vec![0.1; 16_000]));
        }
        let (tx, rx) = mpsc::channel();
        drop(tx);
        AsrWorker::new(transcriber, rx, event_sink(log.clone()), vec![Arc::new(RecordingListener(log.clone()))]).run();
        let log = log.lock().unwrap();
        assert_eq!(log.iter().filter(|entry| entry.as_str() == "error").count(), 2);
        let transcripts: Vec<_> =
            log.iter().filter(|entry| entry.starts_with("transcript:")).map(String::as_str).collect();
        assert_eq!(transcripts, ["transcript:1", "transcript:4"]);
        assert_eq!(log.iter().filter(|entry| entry.starts_with("listener:")).count(), 2);
        assert_eq!(log.last().map(String::as_str), Some("end"));
    }

    #[test]
    fn transcript_is_published_before_derivative_listener_is_notified() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let (_tx, rx) = mpsc::channel();
        let transcriber = StreamingTranscriber::new(ScriptedRecognizer(VecDeque::new()), StreamingConfig::default());
        let mut worker =
            AsrWorker::new(transcriber, rx, event_sink(log.clone()), vec![Arc::new(RecordingListener(log.clone()))]);
        worker.emit(TranscriptSegment {
            id: SegmentId(7),
            revision: 0,
            stable_text: "ready".into(),
            unstable_text: String::new(),
            language: None,
            start_ms: 0,
            end_ms: 1000,
            stability: Stability::Final,
        });
        assert_eq!(*log.lock().unwrap(), ["transcript:7", "listener:7"]);
    }
}
