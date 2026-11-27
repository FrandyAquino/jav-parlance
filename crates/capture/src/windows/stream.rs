use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parlance_audio::StreamFormat;
use parlance_audio::ring::AudioSink;
use wasapi::{AudioCaptureClient, AudioClient, Direction, Handle, SampleType, StreamMode, WasapiError, WaveFormat};

use super::backend_error;
use super::sources::Target;
use crate::{CaptureError, CaptureEvent, CaptureEventHandler, CaptureStream};

/// WASAPI converts whatever the device mix format is into this one, so the
/// rest of the pipeline sees a single, predictable format on Windows.
const FORMAT: StreamFormat = StreamFormat { sample_rate: 48_000, channels: 2 };
const EVENT_TIMEOUT_MS: u32 = 100;
const RECONNECT_DELAY: Duration = Duration::from_millis(500);
const MAX_RECONNECT_ATTEMPTS: u32 = 40;

#[derive(Debug)]
pub struct WasapiStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl WasapiStream {
    pub fn spawn(target: Target, sink: AudioSink, on_event: CaptureEventHandler) -> Result<Self, CaptureError> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = CaptureWorker { target, sink, on_event, stop: Arc::clone(&stop) };
        let thread = std::thread::Builder::new()
            .name("parlance-wasapi-capture".into())
            .spawn(move || worker.run(ready_tx))
            .map_err(|e| backend_error("spawn capture thread", e))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { stop, thread: Some(thread) }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(CaptureError::Backend("capture thread exited during startup".into())),
        }
    }
}

impl CaptureStream for WasapiStream {
    fn format(&self) -> StreamFormat {
        FORMAT
    }
}

impl Drop for WasapiStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct CaptureWorker {
    target: Target,
    sink: AudioSink,
    on_event: CaptureEventHandler,
    stop: Arc<AtomicBool>,
}

/// An initialized, started WASAPI client.
struct OpenStream {
    client: AudioClient,
    capture: AudioCaptureClient,
    event: Handle,
}

enum PumpEnd {
    Stopped,
    ConsumerGone,
    Lost(String),
}

impl CaptureWorker {
    fn run(mut self, ready: mpsc::SyncSender<Result<(), CaptureError>>) {
        if wasapi::initialize_mta().is_err() {
            let _ = ready.send(Err(CaptureError::Backend("COM initialization failed".into())));
            return;
        }
        let mut ready = Some(ready);
        let mut attempts = 0;

        while !self.stop.load(Ordering::Acquire) {
            match open(&self.target) {
                Ok(stream) => {
                    attempts = 0;
                    match ready.take() {
                        Some(tx) => drop(tx.send(Ok(()))),
                        None => (self.on_event)(CaptureEvent::Reconnected),
                    }
                    match self.pump(&stream) {
                        PumpEnd::Stopped | PumpEnd::ConsumerGone => break,
                        PumpEnd::Lost(reason) => (self.on_event)(CaptureEvent::DeviceLost { reason }),
                    }
                    let _ = stream.client.stop_stream();
                }
                Err(error) => {
                    if let Some(tx) = ready.take() {
                        let _ = tx.send(Err(error));
                        return;
                    }
                    attempts += 1;
                    if attempts >= MAX_RECONNECT_ATTEMPTS {
                        (self.on_event)(CaptureEvent::Failed { reason: error.to_string() });
                        break;
                    }
                }
            }
            if !self.target.is_reconnectable() {
                (self.on_event)(CaptureEvent::Failed { reason: "captured application stopped".into() });
                break;
            }
            std::thread::sleep(RECONNECT_DELAY);
        }
        wasapi::deinitialize();
    }

    fn pump(&mut self, stream: &OpenStream) -> PumpEnd {
        let block_align = FORMAT.channels as usize * std::mem::size_of::<f32>();
        let mut bytes = Vec::new();
        let mut samples = Vec::new();
        let mut last_data = Instant::now();

        while !self.stop.load(Ordering::Acquire) {
            if !self.sink.is_consumer_alive() {
                return PumpEnd::ConsumerGone;
            }
            match stream.event.wait_for_event(EVENT_TIMEOUT_MS) {
                Ok(()) | Err(WasapiError::EventTimeout) => {}
                Err(error) => return PumpEnd::Lost(error.to_string()),
            }
            match drain_packets(&stream.capture, block_align, &mut bytes, &mut samples) {
                Ok(0) => self.fill_silence(&mut last_data, &mut samples),
                Ok(_) => {
                    self.sink.push(&samples);
                    last_data = Instant::now();
                }
                Err(error) => return PumpEnd::Lost(error.to_string()),
            }
        }
        PumpEnd::Stopped
    }

    /// Loopback endpoints deliver no packets while nothing plays. Injecting
    /// silence keeps the sample clock aligned with wall-clock time, so
    /// transcript timestamps stay meaningful across pauses.
    fn fill_silence(&mut self, last_data: &mut Instant, samples: &mut Vec<f32>) {
        let elapsed = last_data.elapsed();
        if elapsed < Duration::from_millis(u64::from(EVENT_TIMEOUT_MS)) {
            return;
        }
        let frames = (elapsed.as_secs_f64() * f64::from(FORMAT.sample_rate)) as usize;
        samples.clear();
        samples.resize(frames * FORMAT.channels as usize, 0.0);
        self.sink.push(samples);
        *last_data = Instant::now();
    }
}

fn open(target: &Target) -> Result<OpenStream, CaptureError> {
    let format =
        WaveFormat::new(32, 32, SampleType::Float, FORMAT.sample_rate as usize, FORMAT.channels as usize, None);
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 200_000 };

    let mut client = match target {
        Target::Process(pid) => AudioClient::new_application_loopback_client(*pid, true)
            .map_err(|e| backend_error("process loopback activation", e))?,
        endpoint => endpoint.open_device()?.get_iaudioclient().map_err(|e| backend_error("audio client", e))?,
    };
    client.initialize_client(&format, Direction::Capture, mode, None).map_err(|e| map_init_error(target, e))?;
    let event = client.set_get_eventhandle().map_err(|e| backend_error("event handle", e))?;
    let capture = client.get_audiocaptureclient().map_err(|e| backend_error("capture client", e))?;
    client.start_stream().map_err(|e| backend_error("start stream", e))?;
    Ok(OpenStream { client, capture, event })
}

fn map_init_error(target: &Target, error: WasapiError) -> CaptureError {
    const E_ACCESSDENIED: i32 = 0x8007_0005_u32 as i32;
    match &error {
        WasapiError::Windows(inner) if inner.code().0 == E_ACCESSDENIED => CaptureError::PermissionDenied(format!(
            "{target:?}: microphone access is disabled in Windows privacy settings"
        )),
        _ => backend_error("initialize client", error),
    }
}

/// Reads every queued packet, converting to f32. Returns the frame count.
fn drain_packets(
    capture: &AudioCaptureClient,
    block_align: usize,
    bytes: &mut Vec<u8>,
    samples: &mut Vec<f32>,
) -> Result<usize, WasapiError> {
    samples.clear();
    let mut total = 0;
    while let Some(frames) = capture.get_next_packet_size()?.filter(|f| *f > 0) {
        bytes.resize(frames as usize * block_align, 0);
        let (read, _info) = capture.read_from_device(bytes)?;
        let valid = &bytes[..read as usize * block_align];
        samples.extend(valid.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)));
        total += read as usize;
    }
    Ok(total)
}
