use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

use parlance_audio::convert::downmix_to_mono;
use parlance_audio::level::LevelMeter;
use parlance_audio::mix::StreamMixer;
use parlance_audio::resample::StreamResampler;
use parlance_audio::ring::AudioSource;
use parlance_audio::vad::{GateEvent, VadGate};
use parlance_audio::{StreamFormat, TARGET_SAMPLE_RATE};
use parlance_protocol::{Notice, SessionEvent};

use crate::ports::EventSink;

const IDLE_SLEEP: Duration = Duration::from_millis(10);
const LEVEL_WINDOW: usize = TARGET_SAMPLE_RATE as usize / 20;
const MAX_MIX_LAG: usize = TARGET_SAMPLE_RATE as usize / 4;
const DROP_REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// One captured source as seen by the DSP stage.
pub struct DspInput {
    pub source: AudioSource,
    pub format: StreamFormat,
}

struct Lane {
    source: AudioSource,
    channels: u16,
    resampler: StreamResampler,
    raw: Vec<f32>,
    mono: Vec<f32>,
    resampled: Vec<f32>,
}

pub struct DspStage {
    lanes: Vec<Lane>,
    mixer: StreamMixer,
    meter: LevelMeter,
    gate: VadGate,
    events: EventSink,
    asr: SyncSender<GateEvent>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    reported_drops: u64,
    last_drop_report: Instant,
}

impl DspStage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        inputs: Vec<DspInput>,
        gate: VadGate,
        events: EventSink,
        asr: SyncSender<GateEvent>,
        stop: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Result<Self, parlance_audio::AudioError> {
        let lanes = inputs
            .into_iter()
            .map(|input| {
                Ok(Lane {
                    source: input.source,
                    channels: input.format.channels,
                    resampler: StreamResampler::new(input.format.sample_rate, TARGET_SAMPLE_RATE)?,
                    raw: Vec::new(),
                    mono: Vec::new(),
                    resampled: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, parlance_audio::AudioError>>()?;
        Ok(Self {
            mixer: StreamMixer::new(lanes.len(), MAX_MIX_LAG),
            lanes,
            meter: LevelMeter::new(LEVEL_WINDOW),
            gate,
            events,
            asr,
            stop,
            paused,
            reported_drops: 0,
            last_drop_report: Instant::now(),
        })
    }

    /// Runs until stopped or until every capture stream has ended, then
    /// closes any open utterance so the ASR stage can finalize it.
    pub fn run(mut self) {
        let mut mixed = Vec::new();
        while !self.stop.load(Ordering::Acquire) {
            let received = self.pull_inputs();
            if received == 0 {
                if self.lanes.iter().all(|lane| !lane.source.is_producer_alive()) {
                    break;
                }
                std::thread::sleep(IDLE_SLEEP);
                continue;
            }
            mixed.clear();
            self.mixer.drain_into(&mut mixed);
            if !self.process(&mixed) {
                return;
            }
            self.report_drops();
        }
        for event in self.gate.flush() {
            let _ = self.asr.send(event);
        }
    }

    fn pull_inputs(&mut self) -> usize {
        let mut received = 0;
        for (index, lane) in self.lanes.iter_mut().enumerate() {
            lane.raw.clear();
            if lane.source.drain_into(&mut lane.raw) == 0 {
                continue;
            }
            lane.mono.clear();
            downmix_to_mono(&lane.raw, lane.channels, &mut lane.mono);
            lane.resampled.clear();
            if let Err(error) = lane.resampler.process(&lane.mono, &mut lane.resampled) {
                tracing::error!(%error, "resampling failed");
                continue;
            }
            received += lane.raw.len();
            self.mixer.push(index, &lane.resampled);
        }
        received
    }

    /// Returns `false` when ASR is gone or VAD fails.
    fn process(&mut self, samples: &[f32]) -> bool {
        let events = &self.events;
        self.meter.feed(samples, |level| events(SessionEvent::Level { level }));

        if self.paused.load(Ordering::Acquire) {
            for event in self.gate.flush() {
                if self.asr.send(event).is_err() {
                    return false;
                }
            }
            return true;
        }
        match self.gate.process(samples) {
            Ok(gate_events) => gate_events.into_iter().all(|event| self.asr.send(event).is_ok()),
            Err(error) => {
                (self.events)(SessionEvent::Notice { notice: Notice::error(error.to_string()) });
                for event in self.gate.flush() {
                    if self.asr.send(event).is_err() {
                        break;
                    }
                }
                false
            }
        }
    }

    fn report_drops(&mut self) {
        if self.last_drop_report.elapsed() < DROP_REPORT_INTERVAL {
            return;
        }
        self.last_drop_report = Instant::now();
        let dropped: u64 = self.lanes.iter().map(|lane| lane.source.dropped_samples()).sum();
        if dropped > self.reported_drops {
            let message = format!("audio pipeline overloaded: {} samples dropped", dropped - self.reported_drops);
            tracing::warn!("{message}");
            (self.events)(SessionEvent::Notice { notice: Notice::warning(message) });
            self.reported_drops = dropped;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, mpsc};

    use parlance_audio::vad::{GateConfig, VadError, VoiceActivityDetector};

    struct TransientFailure;

    impl VoiceActivityDetector for TransientFailure {
        fn frame_len(&self) -> usize {
            160
        }

        fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, VadError> {
            if frame[0] == 2.0 { Err(VadError("injected failure".into())) } else { Ok(0.9) }
        }
    }

    #[test]
    fn vad_error_stops_dsp_and_forwards_valid_audio_with_its_closing_boundary() {
        let notices = Arc::new(Mutex::new(Vec::new()));
        let recorded = notices.clone();
        let events: EventSink = Arc::new(move |event| {
            if let SessionEvent::Notice { notice } = event {
                recorded.lock().unwrap().push(notice);
            }
        });
        let (tx, rx) = mpsc::sync_channel(8);
        let mut stage = DspStage::new(
            Vec::new(),
            VadGate::new(Box::new(TransientFailure), GateConfig::default()),
            events,
            tx,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(stage.process(&[1.0; 160]));
        let mut input = vec![3.0; 160];
        input.extend([2.0; 160]);
        input.extend([4.0; 160]);
        assert!(!stage.process(&input), "a VAD error must stop the DSP run loop");
        drop(stage);
        assert_eq!(
            rx.into_iter().collect::<Vec<_>>(),
            vec![
                GateEvent::SpeechStarted { start_sample: 0 },
                GateEvent::Speech(vec![1.0; 160]),
                GateEvent::Speech(vec![3.0; 160]),
                GateEvent::SpeechEnded { end_sample: 320 },
            ]
        );
        assert_eq!(notices.lock().unwrap().len(), 1);
    }
}
