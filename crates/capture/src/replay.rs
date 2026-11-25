//! Replays in-memory audio as if it were a live device. Used by integration
//! tests, benchmarks and the `--replay` developer mode, so the full pipeline
//! can be exercised deterministically without audio hardware.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parlance_audio::StreamFormat;
use parlance_audio::ring::AudioSink;
use parlance_domain::{SourceDescriptor, SourceId, SourceKind};

use crate::{CaptureBackend, CaptureCapabilities, CaptureError, CaptureEventHandler, CaptureStream};

const CHUNK: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct ReplayClip {
    pub name: String,
    pub format: StreamFormat,
    /// Interleaved f32 samples.
    pub samples: Arc<Vec<f32>>,
}

#[derive(Debug, Default)]
pub struct ReplayBackend {
    clips: HashMap<SourceId, ReplayClip>,
    /// Multiplier over real time; `None` replays as fast as the consumer allows.
    speed: Option<f64>,
}

impl ReplayBackend {
    pub fn new(speed: Option<f64>) -> Self {
        Self { clips: HashMap::new(), speed }
    }

    pub fn with_clip(mut self, id: impl Into<String>, clip: ReplayClip) -> Self {
        self.clips.insert(SourceId(id.into()), clip);
        self
    }
}

impl CaptureBackend for ReplayBackend {
    fn name(&self) -> &'static str {
        "replay"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities { system_output: true, ..CaptureCapabilities::default() }
    }

    fn handles(&self, source: &SourceId) -> bool {
        self.clips.contains_key(source)
    }

    fn list_sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError> {
        Ok(self
            .clips
            .iter()
            .map(|(id, clip)| SourceDescriptor {
                id: id.clone(),
                kind: SourceKind::SystemOutput,
                name: clip.name.clone(),
                is_default: false,
            })
            .collect())
    }

    fn start(
        &self,
        source: &SourceId,
        sink: AudioSink,
        _on_event: CaptureEventHandler,
    ) -> Result<Box<dyn CaptureStream>, CaptureError> {
        let clip = self.clips.get(source).cloned().ok_or_else(|| CaptureError::SourceNotFound(source.0.clone()))?;
        Ok(Box::new(ReplayStream::spawn(clip, sink, self.speed)))
    }
}

#[derive(Debug)]
struct ReplayStream {
    format: StreamFormat,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ReplayStream {
    fn spawn(clip: ReplayClip, mut sink: AudioSink, speed: Option<f64>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let format = clip.format;
        let chunk_len = (format.sample_rate as usize * format.channels as usize) / 100;
        let thread = std::thread::spawn(move || {
            let started = Instant::now();
            for (index, chunk) in clip.samples.chunks(chunk_len).enumerate() {
                if stop_flag.load(Ordering::Acquire) {
                    return;
                }
                if let Some(speed) = speed {
                    let due = CHUNK.mul_f64(index as f64 / speed);
                    if let Some(wait) = due.checked_sub(started.elapsed()) {
                        std::thread::sleep(wait);
                    }
                }
                let mut offset = 0;
                while offset < chunk.len() && !stop_flag.load(Ordering::Acquire) {
                    offset += sink.push(&chunk[offset..]);
                    if offset < chunk.len() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            }
        });
        Self { format, stop, thread: Some(thread) }
    }
}

impl CaptureStream for ReplayStream {
    fn format(&self) -> StreamFormat {
        self.format
    }
}

impl Drop for ReplayStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
