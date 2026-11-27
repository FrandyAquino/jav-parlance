use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use parlance_audio::StreamFormat;
use parlance_audio::ring::AudioSink;

use super::Target;
use crate::{CaptureError, CaptureEvent, CaptureEventHandler, CaptureStream};

const FORMAT: StreamFormat = StreamFormat { sample_rate: 48_000, channels: 2 };
const RECONNECT_DELAY: Duration = Duration::from_millis(500);
const MAX_RECONNECTS: u32 = 40;
/// 10 ms of interleaved f32 stereo.
const READ_BYTES: usize = 48_000 / 100 * 2 * 4;

/// Runs `parec` and copies its raw f32 output into the ring buffer. The
/// child process does the real-time work; this thread only converts bytes.
#[derive(Debug)]
pub struct ParecStream {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    thread: Option<JoinHandle<()>>,
}

fn spawn_parec(target: &Target) -> std::io::Result<Child> {
    Command::new("parec")
        .args(["--raw", "--format=float32le", "--rate=48000", "--channels=2", "--latency-msec=20"])
        .args(["--client-name=Parlance", "--stream-name=Meeting capture"])
        .args(target.parec_args())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

impl ParecStream {
    pub fn spawn(target: Target, mut sink: AudioSink, on_event: CaptureEventHandler) -> Result<Self, CaptureError> {
        let first = spawn_parec(&target)
            .map_err(|e| CaptureError::Unsupported(format!("parec not found ({e}); install pulseaudio-utils")))?;
        let stop = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(Some(first)));
        let (stop_flag, child_slot) = (Arc::clone(&stop), Arc::clone(&child));

        let thread = std::thread::Builder::new()
            .name("parlance-parec".into())
            .spawn(move || {
                let mut attempts = 0;
                loop {
                    let stdout = child_slot.lock().ok().and_then(|mut c| c.as_mut().and_then(|c| c.stdout.take()));
                    if let Some(stdout) = stdout {
                        pump(stdout, &mut sink, &stop_flag);
                    }
                    if stop_flag.load(Ordering::Acquire) || !sink.is_consumer_alive() {
                        return;
                    }
                    if !target.is_reconnectable() || attempts >= MAX_RECONNECTS {
                        on_event(CaptureEvent::Failed { reason: "the audio stream ended".into() });
                        return;
                    }
                    on_event(CaptureEvent::DeviceLost { reason: "parec exited".into() });
                    attempts += 1;
                    std::thread::sleep(RECONNECT_DELAY);
                    match spawn_parec(&target) {
                        Ok(next) => {
                            if let Ok(mut slot) = child_slot.lock() {
                                *slot = Some(next);
                            }
                            on_event(CaptureEvent::Reconnected);
                        }
                        Err(error) => tracing::warn!(%error, "parec restart failed"),
                    }
                }
            })
            .map_err(|e| CaptureError::Backend(e.to_string()))?;
        Ok(Self { stop, child, thread: Some(thread) })
    }
}

fn pump(mut stdout: impl Read, sink: &mut AudioSink, stop: &AtomicBool) {
    let mut bytes = vec![0u8; READ_BYTES];
    let mut pending = Vec::with_capacity(READ_BYTES + 4);
    let mut samples = Vec::with_capacity(READ_BYTES / 4);
    while !stop.load(Ordering::Acquire) {
        let read = match stdout.read(&mut bytes) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        pending.extend_from_slice(&bytes[..read]);
        let whole = pending.len() / 4 * 4;
        samples.clear();
        samples.extend(pending[..whole].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)));
        pending.drain(..whole);
        sink.push(&samples);
    }
}

impl CaptureStream for ParecStream {
    fn format(&self) -> StreamFormat {
        FORMAT
    }
}

impl Drop for ParecStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut slot) = self.child.lock()
            && let Some(mut child) = slot.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
