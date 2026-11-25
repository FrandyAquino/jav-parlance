use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use parlance_asr::{StreamingConfig, StreamingTranscriber};
use parlance_audio::ring;
use parlance_audio::vad::{GateConfig, VadGate};
use parlance_capture::{CaptureBackend, CaptureEvent, CaptureEventHandler, CaptureStream};
use parlance_domain::SourceId;
use parlance_protocol::{Notice, SessionEvent};
use tokio::sync::oneshot;

use crate::asr_worker::AsrWorker;
use crate::controller::SessionError;
use crate::dsp::{DspInput, DspStage};
use crate::ports::{EventSink, SegmentListener, SpeechEngine};

/// Ten seconds of 48 kHz stereo: enough to ride out a long GC-free stall in
/// the DSP stage without dropping audio.
const RING_CAPACITY: usize = 48_000 * 2 * 10;
/// Gate events are ~32 ms to a few hundred ms each; 512 covers well over the
/// longest decode before the DSP stage experiences backpressure.
const ASR_QUEUE: usize = 512;

pub struct PipelineSpec {
    pub sources: Vec<SourceId>,
    pub streaming: StreamingConfig,
    pub gate: GateConfig,
}

pub struct RunningPipeline {
    captures: Vec<Box<dyn CaptureStream>>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    dsp: Option<JoinHandle<()>>,
    asr: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for RunningPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningPipeline").field("captures", &self.captures.len()).finish()
    }
}

fn capture_handler(events: EventSink) -> CaptureEventHandler {
    Arc::new(move |event| {
        let notice = match event {
            CaptureEvent::DeviceLost { reason } => {
                Notice::warning(format!("Audio device lost, reconnecting: {reason}"))
            }
            CaptureEvent::Reconnected => Notice::info("Audio device reconnected"),
            CaptureEvent::Failed { reason } => Notice::error(format!("Audio capture ended: {reason}")),
        };
        events(SessionEvent::Notice { notice });
    })
}

fn spawn(name: &str, body: impl FnOnce() + Send + 'static) -> Result<JoinHandle<()>, SessionError> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(body)
        .map_err(|e| SessionError::Internal(format!("failed to spawn {name}: {e}")))
}

impl RunningPipeline {
    /// Opens every source and starts the DSP and ASR threads. Blocking: it
    /// loads models, so call it off the async runtime.
    pub fn start(
        spec: PipelineSpec,
        capture: &dyn CaptureBackend,
        engine: &dyn SpeechEngine,
        events: EventSink,
        listeners: Vec<Arc<dyn SegmentListener>>,
        finished: oneshot::Sender<()>,
    ) -> Result<Self, SessionError> {
        let recognizer = engine.recognizer()?;
        let gate = VadGate::new(engine.voice_detector()?, spec.gate);

        let mut captures = Vec::new();
        let mut inputs = Vec::new();
        for source in &spec.sources {
            let (sink, source_half) = ring::channel(RING_CAPACITY);
            let stream = capture.start(source, sink, capture_handler(Arc::clone(&events)))?;
            inputs.push(DspInput { source: source_half, format: stream.format() });
            captures.push(stream);
        }

        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let (asr_tx, asr_rx) = mpsc::sync_channel(ASR_QUEUE);

        let dsp_stage =
            DspStage::new(inputs, gate, Arc::clone(&events), asr_tx, Arc::clone(&stop), Arc::clone(&paused))?;
        let transcriber = StreamingTranscriber::new(recognizer, spec.streaming);
        let worker = AsrWorker::new(transcriber, asr_rx, events, listeners);

        let dsp = spawn("parlance-dsp", move || dsp_stage.run())?;
        let asr = spawn("parlance-asr", move || {
            worker.run();
            let _ = finished.send(());
        })?;
        Ok(Self { captures, stop, paused, dsp: Some(dsp), asr: Some(asr) })
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
    }

    /// Stops capture, lets the DSP stage close the open utterance and waits
    /// for the ASR stage to finalize it. Blocking.
    pub fn shutdown(mut self) {
        self.captures.clear();
        self.stop.store(true, Ordering::Release);
        for handle in [self.dsp.take(), self.asr.take()].into_iter().flatten() {
            if handle.join().is_err() {
                tracing::error!("pipeline thread panicked");
            }
        }
    }
}
