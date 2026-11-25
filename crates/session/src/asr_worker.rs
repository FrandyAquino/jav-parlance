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
        for listener in &self.listeners {
            listener.on_segment(&segment);
        }
        (self.events)(SessionEvent::Transcript { segment });
    }
}
