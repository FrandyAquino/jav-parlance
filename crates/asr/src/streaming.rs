use std::collections::VecDeque;

use parlance_audio::TARGET_SAMPLE_RATE;
use parlance_audio::vad::GateEvent;
use parlance_domain::{LanguageCode, SegmentId, Stability, TranscriptSegment};

use crate::agreement::LocalAgreement;
use crate::recognizer::{AsrError, DecodeOptions, SpeechRecognizer};

#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// New audio required before re-decoding the open utterance. 500 ms keeps
    /// words appearing (and being translated) while the speaker talks; slow
    /// machines self-throttle because decoding drains the backlog first.
    pub step_ms: u32,
    /// Utterances shorter than this are not decoded as partials.
    pub min_partial_ms: u32,
    /// Forced source language; `None` enables per-utterance detection.
    pub language: Option<LanguageCode>,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self { step_ms: 500, min_partial_ms: 500, language: None }
    }
}

#[derive(Debug)]
struct Utterance {
    id: SegmentId,
    start_sample: u64,
    audio: Vec<f32>,
    pending_samples: usize,
    agreement: LocalAgreement,
    revision: u32,
    language: Option<LanguageCode>,
    emitted: bool,
}

impl Utterance {
    fn update_language(&mut self, language: Option<LanguageCode>) {
        if self.language != language {
            self.agreement = LocalAgreement::new();
            self.language = language;
        }
    }

    fn segment(&mut self, stable: String, unstable: String, stability: Stability) -> TranscriptSegment {
        let segment = TranscriptSegment {
            id: self.id,
            revision: self.revision,
            stable_text: stable,
            unstable_text: unstable,
            language: self.language.clone(),
            start_ms: samples_to_ms(self.start_sample),
            end_ms: samples_to_ms(self.start_sample + self.audio.len() as u64),
            stability,
        };
        self.revision += 1;
        self.emitted = true;
        segment
    }
}

fn samples_to_ms(samples: u64) -> u64 {
    samples * 1000 / u64::from(TARGET_SAMPLE_RATE)
}

fn ms_to_samples(ms: u32) -> usize {
    TARGET_SAMPLE_RATE as usize * ms as usize / 1000
}

/// Converts VAD-gated audio into partial and final transcript segments.
///
/// Buffering ([`Self::push`]) is cheap; decoding happens only in
/// [`Self::poll`]. A worker drains every queued event before polling, so a
/// slow recognizer naturally decodes larger steps instead of falling behind.
#[derive(Debug)]
pub struct StreamingTranscriber<R> {
    recognizer: R,
    config: StreamingConfig,
    next_id: u64,
    open: Option<Utterance>,
    closed: VecDeque<Utterance>,
    ready: Vec<TranscriptSegment>,
}

impl<R: SpeechRecognizer> StreamingTranscriber<R> {
    pub fn new(recognizer: R, config: StreamingConfig) -> Self {
        Self { recognizer, config, next_id: 1, open: None, closed: VecDeque::new(), ready: Vec::new() }
    }

    pub fn set_language(&mut self, language: Option<LanguageCode>) {
        self.config.language = language;
    }

    pub fn push(&mut self, event: GateEvent) {
        match event {
            GateEvent::SpeechStarted { start_sample } => {
                self.close_open();
                let id = SegmentId(self.next_id);
                self.next_id += 1;
                self.open = Some(Utterance {
                    id,
                    start_sample,
                    audio: Vec::new(),
                    pending_samples: 0,
                    agreement: LocalAgreement::new(),
                    revision: 0,
                    language: self.config.language.clone(),
                    emitted: false,
                });
            }
            GateEvent::Speech(samples) => {
                if let Some(utterance) = self.open.as_mut() {
                    utterance.pending_samples += samples.len();
                    utterance.audio.extend_from_slice(&samples);
                }
            }
            GateEvent::SpeechEnded { .. } => self.close_open(),
        }
    }

    /// Whether [`Self::poll`] would deliver updates or run the recognizer.
    pub fn has_work(&self) -> bool {
        !self.ready.is_empty() || !self.closed.is_empty() || self.open.as_ref().is_some_and(|u| self.partial_due(u))
    }

    /// Finalizes closed utterances first (they are user-visible commitments),
    /// then refreshes the open utterance's partial hypothesis when due.
    /// Successful updates survive errors and are delivered by a later poll.
    pub fn poll(&mut self) -> Result<Vec<TranscriptSegment>, AsrError> {
        if !self.ready.is_empty() {
            return Ok(std::mem::take(&mut self.ready));
        }
        while let Some(mut utterance) = self.closed.pop_front() {
            if let Some(segment) = self.finalize(&mut utterance)? {
                self.ready.push(segment);
            }
        }
        if let Some(mut utterance) = self.open.take() {
            let due = self.partial_due(&utterance);
            let result = if due { self.refresh_partial(&mut utterance) } else { Ok(None) };
            self.open = Some(utterance);
            self.ready.extend(result?);
        }
        Ok(std::mem::take(&mut self.ready))
    }

    /// Closes the open utterance and polls pending finals. After an error,
    /// drain while [`Self::has_work`] to deliver retained successful updates.
    pub fn flush(&mut self) -> Result<Vec<TranscriptSegment>, AsrError> {
        self.close_open();
        self.poll()
    }

    fn close_open(&mut self) {
        if let Some(utterance) = self.open.take() {
            self.closed.push_back(utterance);
        }
    }

    fn partial_due(&self, utterance: &Utterance) -> bool {
        utterance.audio.len() >= ms_to_samples(self.config.min_partial_ms)
            && utterance.pending_samples >= ms_to_samples(self.config.step_ms)
    }

    fn options(&self, fast: bool) -> DecodeOptions {
        DecodeOptions { language: self.config.language.clone(), fast }
    }

    fn refresh_partial(&mut self, utterance: &mut Utterance) -> Result<Option<TranscriptSegment>, AsrError> {
        utterance.pending_samples = 0;
        let options = self.options(true);
        let hypothesis = self.recognizer.transcribe(&utterance.audio, &options)?;
        utterance.update_language(options.language.or(hypothesis.language));
        if hypothesis.text.trim().is_empty() && !utterance.emitted {
            return Ok(None);
        }
        let (stable, unstable) = utterance.agreement.update(&hypothesis.text);
        Ok(Some(utterance.segment(stable, unstable, Stability::Partial)))
    }

    fn finalize(&mut self, utterance: &mut Utterance) -> Result<Option<TranscriptSegment>, AsrError> {
        if utterance.audio.is_empty() {
            return Ok(None);
        }
        let options = self.options(false);
        let hypothesis = self.recognizer.transcribe(&utterance.audio, &options)?;
        utterance.update_language(options.language.or(hypothesis.language));
        let text = utterance.agreement.finalize(&hypothesis.text);
        if text.trim().is_empty() && !utterance.emitted {
            return Ok(None);
        }
        Ok(Some(utterance.segment(text, String::new(), Stability::Final)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recognizer::Hypothesis;

    /// Reveals one more word of a script each time it is called, like a
    /// recognizer seeing more audio.
    struct ScriptedRecognizer {
        words: Vec<&'static str>,
        calls: usize,
        fast_calls: usize,
    }

    impl ScriptedRecognizer {
        fn new(text: &'static str) -> Self {
            Self { words: text.split(' ').collect(), calls: 0, fast_calls: 0 }
        }
    }

    impl SpeechRecognizer for ScriptedRecognizer {
        fn transcribe(&mut self, _audio: &[f32], options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
            self.calls += 1;
            if options.fast {
                self.fast_calls += 1;
            }
            let visible = if options.fast { self.calls.min(self.words.len()) } else { self.words.len() };
            Ok(Hypothesis { text: self.words[..visible].join(" "), language: Some(LanguageCode::new("en").unwrap()) })
        }
    }

    fn transcriber(text: &'static str) -> StreamingTranscriber<ScriptedRecognizer> {
        StreamingTranscriber::new(ScriptedRecognizer::new(text), StreamingConfig::default())
    }

    fn one_second() -> GateEvent {
        GateEvent::Speech(vec![0.1; TARGET_SAMPLE_RATE as usize])
    }

    struct FallibleRecognizer {
        results: VecDeque<Result<&'static str, &'static str>>,
    }

    impl SpeechRecognizer for FallibleRecognizer {
        fn transcribe(&mut self, _audio: &[f32], _options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
            self.results
                .pop_front()
                .expect("one scripted result per decode")
                .map(|text| Hypothesis { text: text.into(), language: None })
                .map_err(|error| AsrError::Inference(error.into()))
        }
    }

    fn queue_utterance<R: SpeechRecognizer>(t: &mut StreamingTranscriber<R>, start_sample: u64, closed: bool) {
        t.push(GateEvent::SpeechStarted { start_sample });
        t.push(one_second());
        if closed {
            t.push(GateEvent::SpeechEnded { end_sample: start_sample + 16_000 });
        }
    }

    #[test]
    fn successful_final_survives_a_later_final_error_exactly_once() {
        let recognizer = FallibleRecognizer { results: VecDeque::from([Ok("first"), Err("second")]) };
        let mut t = StreamingTranscriber::new(recognizer, StreamingConfig::default());
        queue_utterance(&mut t, 0, true);
        queue_utterance(&mut t, 16_000, true);
        assert!(t.poll().is_err());
        assert!(t.has_work(), "a decoded final still needs delivery");
        let updates = t.poll().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].id, SegmentId(1));
        assert_eq!(updates[0].text(), "first");
        assert!(updates[0].is_final());
        assert!(!t.has_work());
        assert!(t.flush().unwrap().is_empty());
    }

    #[test]
    fn successful_final_survives_a_later_partial_error() {
        let recognizer = FallibleRecognizer { results: VecDeque::from([Ok("first"), Err("partial"), Ok("last")]) };
        let mut t = StreamingTranscriber::new(recognizer, StreamingConfig::default());
        queue_utterance(&mut t, 0, true);
        queue_utterance(&mut t, 16_000, false);
        assert!(t.poll().is_err());
        let updates = t.poll().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].text(), "first");
        let final_updates = t.flush().unwrap();
        assert_eq!(final_updates.len(), 1);
        assert_eq!(final_updates[0].id, SegmentId(2));
        assert_eq!(final_updates[0].revision, 0);
        assert_eq!(final_updates[0].text(), "last");
    }

    #[test]
    fn flush_preserves_successes_across_multiple_failures() {
        let recognizer =
            FallibleRecognizer { results: VecDeque::from([Ok("first"), Err("second"), Err("third"), Ok("fourth")]) };
        let mut t = StreamingTranscriber::new(recognizer, StreamingConfig::default());
        for index in 0..4 {
            queue_utterance(&mut t, index * 16_000, index != 3);
        }
        let mut updates = Vec::new();
        let mut errors = 0;
        for _ in 0..4 {
            match t.flush() {
                Ok(segments) => updates.extend(segments),
                Err(_) => errors += 1,
            }
        }
        assert_eq!(errors, 2);
        assert_eq!(updates.iter().map(|segment| segment.id).collect::<Vec<_>>(), [SegmentId(1), SegmentId(4)]);
        assert!(!t.has_work());
    }

    struct AmbiguousStart;

    impl SpeechRecognizer for AmbiguousStart {
        fn transcribe(&mut self, audio: &[f32], options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
            let language = options
                .language
                .clone()
                .unwrap_or_else(|| LanguageCode::new(if audio.len() <= 16_000 { "en" } else { "es" }).unwrap());
            let text = if language.as_str() == "es" { "ayer terminamos el trabajo" } else { "the airport" };
            Ok(Hypothesis { text: text.into(), language: Some(language) })
        }
    }

    #[test]
    fn auto_language_recovers_from_an_ambiguous_start_without_preserving_wrong_words() {
        let mut t = StreamingTranscriber::new(AmbiguousStart, StreamingConfig::default());
        t.push(GateEvent::SpeechStarted { start_sample: 0 });
        for _ in 0..2 {
            t.push(GateEvent::Speech(vec![0.1; 8_000]));
            t.poll().unwrap();
        }
        t.push(one_second());
        let correction = t.poll().unwrap();
        assert_eq!(correction[0].language.as_ref().unwrap().as_str(), "es");
        assert_eq!(correction[0].text(), "ayer terminamos el trabajo");
        let final_update = t.flush().unwrap();
        assert_eq!(final_update[0].text(), "ayer terminamos el trabajo");
        assert_eq!(final_update[0].language.as_ref().unwrap().as_str(), "es");
    }

    #[test]
    fn explicit_language_stays_forced_despite_ambiguous_audio() {
        let config = StreamingConfig { language: Some(LanguageCode::new("es").unwrap()), ..Default::default() };
        let mut t = StreamingTranscriber::new(AmbiguousStart, config);
        t.push(GateEvent::SpeechStarted { start_sample: 0 });
        t.push(GateEvent::Speech(vec![0.1; 8_000]));
        let partial = t.poll().unwrap();
        assert_eq!(partial[0].text(), "ayer terminamos el trabajo");
        assert_eq!(t.flush().unwrap()[0].language.as_ref().unwrap().as_str(), "es");
    }

    #[test]
    fn emits_partials_then_final_with_stable_id_and_increasing_revision() {
        let mut t = transcriber("one two three");
        t.push(GateEvent::SpeechStarted { start_sample: 16_000 });
        t.push(one_second());
        let first = t.poll().unwrap();
        t.push(one_second());
        let second = t.poll().unwrap();
        t.push(GateEvent::SpeechEnded { end_sample: 48_000 });
        let last = t.poll().unwrap();

        let all: Vec<_> = first.iter().chain(&second).chain(&last).collect();
        assert!(all.iter().all(|s| s.id == SegmentId(1)));
        assert_eq!(all.iter().map(|s| s.revision).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(second[0].stable_text, "one");
        assert_eq!(second[0].unstable_text, "two");
        assert_eq!(last[0].stability, Stability::Final);
        assert_eq!(last[0].stable_text, "one two three");
        assert_eq!((last[0].start_ms, last[0].end_ms), (1_000, 3_000));
        assert_eq!(last[0].language.as_ref().map(LanguageCode::as_str), Some("en"));
    }

    #[test]
    fn waits_for_a_full_step_before_decoding_again() {
        let mut t = transcriber("a b");
        t.push(GateEvent::SpeechStarted { start_sample: 0 });
        t.push(GateEvent::Speech(vec![0.1; 4_000]));
        assert!(!t.has_work());
        assert!(t.poll().unwrap().is_empty());
        t.push(GateEvent::Speech(vec![0.1; 4_000]));
        assert!(t.has_work());
        t.poll().unwrap();
        t.push(GateEvent::Speech(vec![0.1; 4_000]));
        assert!(!t.has_work(), "less than one step of new audio");
    }

    #[test]
    fn new_utterance_closes_the_previous_one() {
        let mut t = transcriber("hi");
        t.push(GateEvent::SpeechStarted { start_sample: 0 });
        t.push(one_second());
        t.push(GateEvent::SpeechStarted { start_sample: 32_000 });
        t.push(one_second());
        let updates = t.poll().unwrap();
        assert_eq!(updates[0].id, SegmentId(1));
        assert!(updates[0].is_final());
        assert_eq!(updates[1].id, SegmentId(2));
        assert!(!updates[1].is_final());
    }

    #[test]
    fn flush_finalizes_open_utterance() {
        let mut t = transcriber("bye");
        t.push(GateEvent::SpeechStarted { start_sample: 0 });
        t.push(GateEvent::Speech(vec![0.1; 4_000]));
        let updates = t.flush().unwrap();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].is_final());
        assert_eq!(t.recognizer.fast_calls, 0);
    }
}
