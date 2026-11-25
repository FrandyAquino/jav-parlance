//! Voice activity detection: detector port plus a hysteresis gate that turns
//! per-frame speech probabilities into utterance boundaries.

use std::collections::VecDeque;

use crate::TARGET_SAMPLE_RATE;

#[derive(Debug, thiserror::Error)]
#[error("voice activity detection failed: {0}")]
pub struct VadError(pub String);

/// Port implemented by concrete detectors (Silero, energy, ...).
pub trait VoiceActivityDetector: Send {
    /// Number of 16 kHz mono samples per probability.
    fn frame_len(&self) -> usize;
    fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, VadError>;
    fn reset(&mut self) {}
}

/// Dependency-free fallback detector based on frame loudness.
#[derive(Debug)]
pub struct EnergyVad {
    frame_len: usize,
    threshold_db: f32,
}

impl EnergyVad {
    pub fn new(threshold_db: f32) -> Self {
        Self { frame_len: 512, threshold_db }
    }
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new(-45.0)
    }
}

impl VoiceActivityDetector for EnergyVad {
    fn frame_len(&self) -> usize {
        self.frame_len
    }

    fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, VadError> {
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len().max(1) as f32).sqrt();
        let db = 20.0 * rms.max(1e-6).log10();
        Ok(1.0 / (1.0 + (-(db - self.threshold_db) / 3.0).exp()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GateConfig {
    pub start_threshold: f32,
    pub end_threshold: f32,
    /// Silence required before an utterance is closed.
    pub min_silence_ms: u32,
    /// Audio kept from before the detected onset so first phonemes are not clipped.
    pub pre_roll_ms: u32,
    /// Hard cap that forces a boundary in monologues without pauses.
    pub max_utterance_ms: u32,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            start_threshold: 0.5,
            end_threshold: 0.35,
            min_silence_ms: 600,
            pre_roll_ms: 200,
            max_utterance_ms: 20_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateEvent {
    SpeechStarted { start_sample: u64 },
    Speech(Vec<f32>),
    SpeechEnded { end_sample: u64 },
}

/// Hysteresis state machine over detector output.
pub struct VadGate {
    detector: Box<dyn VoiceActivityDetector>,
    config: GateConfig,
    pending: Vec<f32>,
    pre_roll: VecDeque<f32>,
    in_speech: bool,
    silence_samples: usize,
    utterance_samples: usize,
    position: u64,
}

impl std::fmt::Debug for VadGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VadGate")
            .field("config", &self.config)
            .field("in_speech", &self.in_speech)
            .field("position", &self.position)
            .finish()
    }
}

fn ms_to_samples(ms: u32) -> usize {
    (TARGET_SAMPLE_RATE as usize * ms as usize) / 1000
}

impl VadGate {
    pub fn new(detector: Box<dyn VoiceActivityDetector>, config: GateConfig) -> Self {
        Self {
            detector,
            config,
            pending: Vec::new(),
            pre_roll: VecDeque::new(),
            in_speech: false,
            silence_samples: 0,
            utterance_samples: 0,
            position: 0,
        }
    }

    pub fn is_in_speech(&self) -> bool {
        self.in_speech
    }

    /// Consumes 16 kHz mono samples and returns the resulting boundary events.
    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<GateEvent>, VadError> {
        self.pending.extend_from_slice(samples);
        let frame_len = self.detector.frame_len();
        let mut events = Vec::new();
        let mut offset = 0;
        while self.pending.len() - offset >= frame_len {
            let frame: Vec<f32> = self.pending[offset..offset + frame_len].to_vec();
            offset += frame_len;
            let probability = self.detector.speech_probability(&frame)?;
            self.step(&frame, probability, &mut events);
        }
        self.pending.drain(..offset);
        Ok(events)
    }

    /// Closes any open utterance, e.g. when the session stops.
    pub fn flush(&mut self) -> Vec<GateEvent> {
        let mut events = Vec::new();
        if self.in_speech {
            self.close(&mut events);
        }
        events
    }

    fn step(&mut self, frame: &[f32], probability: f32, events: &mut Vec<GateEvent>) {
        let frame_start = self.position;
        self.position += frame.len() as u64;

        if !self.in_speech {
            if probability >= self.config.start_threshold {
                self.open(frame_start, frame, events);
            } else {
                self.remember_pre_roll(frame);
            }
            return;
        }

        push_speech(events, frame);
        self.utterance_samples += frame.len();
        self.silence_samples =
            if probability < self.config.end_threshold { self.silence_samples + frame.len() } else { 0 };

        let silent_long_enough = self.silence_samples >= ms_to_samples(self.config.min_silence_ms);
        let too_long = self.utterance_samples >= ms_to_samples(self.config.max_utterance_ms);
        if silent_long_enough || too_long {
            self.close(events);
        }
    }

    fn open(&mut self, frame_start: u64, frame: &[f32], events: &mut Vec<GateEvent>) {
        let pre_roll: Vec<f32> = self.pre_roll.drain(..).collect();
        let start_sample = frame_start - pre_roll.len() as u64;
        events.push(GateEvent::SpeechStarted { start_sample });
        push_speech(events, &pre_roll);
        push_speech(events, frame);
        self.in_speech = true;
        self.silence_samples = 0;
        self.utterance_samples = pre_roll.len() + frame.len();
    }

    fn close(&mut self, events: &mut Vec<GateEvent>) {
        events.push(GateEvent::SpeechEnded { end_sample: self.position });
        self.in_speech = false;
        self.silence_samples = 0;
        self.utterance_samples = 0;
        self.detector.reset();
    }

    fn remember_pre_roll(&mut self, frame: &[f32]) {
        self.pre_roll.extend(frame);
        let capacity = ms_to_samples(self.config.pre_roll_ms);
        let excess = self.pre_roll.len().saturating_sub(capacity);
        self.pre_roll.drain(..excess);
    }
}

/// Appends to the trailing `Speech` event when possible to keep events coarse.
fn push_speech(events: &mut Vec<GateEvent>, samples: &[f32]) {
    if samples.is_empty() {
        return;
    }
    if let Some(GateEvent::Speech(buffer)) = events.last_mut() {
        buffer.extend_from_slice(samples);
    } else {
        events.push(GateEvent::Speech(samples.to_vec()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detector that treats any non-zero frame as speech.
    struct ScriptedVad;

    impl VoiceActivityDetector for ScriptedVad {
        fn frame_len(&self) -> usize {
            160
        }
        fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, VadError> {
            Ok(if frame.iter().any(|s| *s != 0.0) { 0.9 } else { 0.0 })
        }
    }

    fn gate(config: GateConfig) -> VadGate {
        VadGate::new(Box::new(ScriptedVad), config)
    }

    fn speech_len(events: &[GateEvent]) -> usize {
        events.iter().map(|e| if let GateEvent::Speech(s) = e { s.len() } else { 0 }).sum()
    }

    #[test]
    fn detects_utterance_with_pre_roll() {
        let mut gate = gate(GateConfig { pre_roll_ms: 20, min_silence_ms: 50, ..Default::default() });
        let mut audio = vec![0.0; 1600];
        audio.extend(vec![0.5; 1600]);
        audio.extend(vec![0.0; 1600]);
        let events = gate.process(&audio).unwrap();

        assert_eq!(events.first(), Some(&GateEvent::SpeechStarted { start_sample: 1600 - 320 }));
        assert!(matches!(events.last(), Some(GateEvent::SpeechEnded { .. })));
        assert!(speech_len(&events) >= 1600 + 320);
        assert!(!gate.is_in_speech());
    }

    #[test]
    fn short_pauses_do_not_split_utterances() {
        let mut gate = gate(GateConfig { min_silence_ms: 500, ..Default::default() });
        let mut audio = vec![0.5; 1600];
        audio.extend(vec![0.0; 3200]);
        audio.extend(vec![0.5; 1600]);
        let events = gate.process(&audio).unwrap();
        let starts = events.iter().filter(|e| matches!(e, GateEvent::SpeechStarted { .. })).count();
        assert_eq!(starts, 1);
        assert!(gate.is_in_speech());
    }

    #[test]
    fn caps_utterance_length() {
        let mut gate = gate(GateConfig { max_utterance_ms: 1000, ..Default::default() });
        let events = gate.process(&vec![0.5; 16_000 * 3]).unwrap();
        let ends = events.iter().filter(|e| matches!(e, GateEvent::SpeechEnded { .. })).count();
        assert_eq!(ends, 3);
    }

    #[test]
    fn buffers_partial_frames_between_calls() {
        let mut gate = gate(GateConfig::default());
        assert!(gate.process(&[0.5; 100]).unwrap().is_empty());
        let events = gate.process(&[0.5; 100]).unwrap();
        assert!(matches!(events.first(), Some(GateEvent::SpeechStarted { .. })));
    }

    #[test]
    fn flush_closes_open_utterance() {
        let mut gate = gate(GateConfig::default());
        gate.process(&[0.5; 320]).unwrap();
        assert_eq!(gate.flush(), vec![GateEvent::SpeechEnded { end_sample: 320 }]);
        assert!(gate.flush().is_empty());
    }

    #[test]
    fn energy_vad_separates_silence_from_speech() {
        let mut vad = EnergyVad::default();
        assert!(vad.speech_probability(&[0.0; 512]).unwrap() < 0.1);
        assert!(vad.speech_probability(&[0.2; 512]).unwrap() > 0.9);
    }
}
