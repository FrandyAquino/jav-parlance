//! Core Audio taps (14.2+), ScreenCaptureKit (13+) and native microphone input.
//! The Objective-C bridge owns Apple handles; this module owns the audio ring.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use parlance_audio::{StreamFormat, ring::AudioSink};
use parlance_domain::{SourceDescriptor, SourceId};

use crate::{CaptureBackend, CaptureCapabilities, CaptureError, CaptureEvent, CaptureEventHandler, CaptureStream};

#[derive(Debug)]
pub struct MacosBackend;

impl CaptureBackend for MacosBackend {
    fn name(&self) -> &'static str {
        "core-audio"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities { system_output: true, application: true, microphone: true, browser_tab: false }
    }

    fn handles(&self, source: &SourceId) -> bool {
        Target::parse(source).is_ok()
    }

    fn list_sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError> {
        // SAFETY: the bridge returns a malloc-owned NUL-terminated string or null.
        let json = unsafe { take_string(parlance_macos_sources()) }
            .ok_or_else(|| CaptureError::Backend("could not enumerate macOS audio sources".into()))?;
        serde_json::from_str(&json).map_err(|e| CaptureError::Backend(e.to_string()))
    }

    fn start(
        &self,
        source: &SourceId,
        sink: AudioSink,
        on_event: CaptureEventHandler,
    ) -> Result<Box<dyn CaptureStream>, CaptureError> {
        Ok(Box::new(MacosStream::spawn(Target::parse(source)?, sink, on_event)?))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Target {
    kind: i32,
    identifier: CString,
}

impl Target {
    fn parse(source: &SourceId) -> Result<Self, CaptureError> {
        let raw = source.0.as_str();
        let (kind, id) = if raw == "system:default" {
            (0, "default")
        } else if let Some(pid) = raw.strip_prefix("app:") {
            if !pid.parse::<i32>().is_ok_and(|pid| pid > 0) {
                return Err(CaptureError::SourceNotFound(raw.into()));
            }
            (1, pid)
        } else if let Some(uid) = raw.strip_prefix("mic:").filter(|s| !s.is_empty()) {
            (2, uid)
        } else {
            return Err(CaptureError::SourceNotFound(raw.into()));
        };
        let identifier = CString::new(id).map_err(|_| CaptureError::SourceNotFound(raw.into()))?;
        Ok(Self { kind, identifier })
    }
}

#[repr(C)]
struct NativeBuffer {
    data: *const f32,
    samples: u32,
    channels: u32,
}

struct CallbackState {
    sink: AudioSink,
    invalid: AtomicBool,
}

/// Each Apple stream serializes callbacks. Native stop fences them before the
/// worker drops CallbackState. No other thread touches its AudioSink.
unsafe extern "C" fn push(context: *mut c_void, buffers: *const NativeBuffer, count: u32) {
    // SAFETY: native.m invokes this only with the live boxed state and count valid buffers.
    let state = context.cast::<CallbackState>();
    // SAFETY: native.m supplies a stack array of `count` entries, bounded to 32.
    let buffers = unsafe { std::slice::from_raw_parts(buffers, count as usize) };
    // SAFETY: only the serialized callbacks access sink. The worker reads only invalid.
    if !copy_buffers(unsafe { &mut (*state).sink }, buffers) {
        // SAFETY: state remains allocated; invalid is shared only through atomic operations.
        unsafe { &(*state).invalid }.store(true, Ordering::Release);
    }
}

fn copy_buffers(sink: &mut AudioSink, buffers: &[NativeBuffer]) -> bool {
    let channels: usize = buffers.iter().map(|b| b.channels as usize).sum();
    if channels == 0 || channels > 32 || buffers.iter().any(|b| b.channels == 0 || b.data.is_null()) {
        return false;
    }
    let frames = buffers[0].samples / buffers[0].channels;
    if buffers.iter().any(|b| b.samples % b.channels != 0 || b.samples / b.channels != frames) {
        return false;
    }
    if buffers.len() == 1 {
        // SAFETY: native buffers hold exactly samples initialized f32 values until callback returns.
        sink.push(unsafe { std::slice::from_raw_parts(buffers[0].data, buffers[0].samples as usize) });
        return true;
    }
    let mut chunk = [0.0; 1024];
    let chunk_frames = chunk.len() / channels;
    for start in (0..frames as usize).step_by(chunk_frames) {
        let frames_here = chunk_frames.min(frames as usize - start);
        for frame in 0..frames_here {
            let mut channel = 0;
            for buffer in buffers {
                for c in 0..buffer.channels as usize {
                    // SAFETY: all buffer lengths were validated against frames above.
                    chunk[frame * channels + channel] =
                        unsafe { *buffer.data.add((start + frame) * buffer.channels as usize + c) };
                    channel += 1;
                }
            }
        }
        sink.push(&chunk[..frames_here * channels]);
    }
    true
}

#[derive(Debug)]
struct MacosStream {
    format: StreamFormat,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MacosStream {
    fn spawn(target: Target, sink: AudioSink, on_event: CaptureEventHandler) -> Result<Self, CaptureError> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);
        let (ready, started) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("parlance-core-audio".into())
            .spawn(move || {
                let mut state = Box::new(CallbackState { sink, invalid: AtomicBool::new(false) });
                let (mut rate, mut channels, mut code, mut error) = (0, 0, 0, std::ptr::null_mut());
                // SAFETY: all out-pointers are valid; state stays boxed until native stop fences callbacks.
                let handle = unsafe {
                    parlance_macos_start(
                        target.kind,
                        target.identifier.as_ptr(),
                        (&mut *state as *mut CallbackState).cast(),
                        push,
                        &mut rate,
                        &mut channels,
                        &mut code,
                        &mut error,
                    )
                };
                if handle.is_null() {
                    // SAFETY: error is null or a malloc-owned string from the bridge.
                    let message = unsafe { take_string(error) }.unwrap_or_else(|| "macOS audio capture failed".into());
                    let error = match code {
                        1 => CaptureError::PermissionDenied(message),
                        3 => CaptureError::SourceNotFound(message),
                        _ => CaptureError::Backend(message),
                    };
                    let _ = ready.send(Err(error));
                    return;
                }
                let _ = ready.send(Ok(StreamFormat { sample_rate: rate, channels }));
                while !stop_worker.load(Ordering::Acquire) {
                    // SAFETY: the native handle is live and status only reads atomic/native device state.
                    let status = unsafe { parlance_macos_status(handle) };
                    if status != 0 || state.invalid.load(Ordering::Acquire) {
                        let reason = match status {
                            3 => "The default audio device changed. Start a new session to capture it.",
                            2 => "The audio source disconnected or capture permission was revoked.",
                            _ => "The audio format changed. Start a new session to use the new format.",
                        };
                        on_event(CaptureEvent::Failed { reason: reason.into() });
                        break;
                    }
                    std::thread::park_timeout(Duration::from_millis(100));
                }
                // SAFETY: called once on this live handle, before state and its sink are dropped.
                unsafe { parlance_macos_stop(handle) };
            })
            .map_err(|e| CaptureError::Backend(e.to_string()))?;
        match started.recv() {
            Ok(Ok(format)) => Ok(Self { format, stop, thread: Some(thread) }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(error) => {
                let _ = thread.join();
                Err(CaptureError::Backend(error.to_string()))
            }
        }
    }
}

impl CaptureStream for MacosStream {
    fn format(&self) -> StreamFormat {
        self.format
    }
}

impl Drop for MacosStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

unsafe fn take_string(value: *mut c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a malloc-owned NUL-terminated string from native.m.
    let text = unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned();
    // SAFETY: paired with the malloc/strdup allocator used by native.m.
    unsafe { parlance_macos_free(value) };
    Some(text)
}

unsafe extern "C" {
    fn parlance_macos_sources() -> *mut c_char;
    fn parlance_macos_start(
        kind: i32,
        identifier: *const c_char,
        context: *mut c_void,
        push: unsafe extern "C" fn(*mut c_void, *const NativeBuffer, u32),
        rate: *mut u32,
        channels: *mut u16,
        code: *mut i32,
        error: *mut *mut c_char,
    ) -> *mut c_void;
    fn parlance_macos_status(handle: *mut c_void) -> i32;
    fn parlance_macos_stop(handle: *mut c_void);
    fn parlance_macos_free(value: *mut c_char);
}

#[cfg(test)]
mod tests {
    use super::*;
    use parlance_audio::ring::channel;

    #[test]
    fn native_enumeration_returns_valid_source_descriptors_without_prompting() {
        let sources = MacosBackend.list_sources().unwrap();
        assert!(sources.iter().any(|s| s.id.0 == "system:default" && s.is_default));
        assert!(sources.iter().all(|s| MacosBackend.handles(&s.id)));
    }

    #[test]
    fn source_ids_are_strict_and_microphone_uids_are_preserved() {
        for id in ["system:default", "mic:default", "mic:BuiltInMicrophoneDevice", "app:123"] {
            assert!(Target::parse(&SourceId(id.into())).is_ok(), "{id}");
        }
        for id in ["system:other", "mic:", "mic:bad\0uid", "app:0", "app:-1", "app:2147483648", "tab:123"] {
            assert!(Target::parse(&SourceId(id.into())).is_err(), "{id}");
        }
    }

    #[test]
    fn copies_planar_and_interleaved_audio_in_frame_order() {
        let (mut sink, mut source) = channel(4096);
        let left = vec![0.25; 1200];
        let right = vec![-0.5; 1200];
        let buffers = [
            NativeBuffer { data: left.as_ptr(), samples: 1200, channels: 1 },
            NativeBuffer { data: right.as_ptr(), samples: 1200, channels: 1 },
        ];
        assert!(copy_buffers(&mut sink, &buffers));
        let mut out = Vec::new();
        source.drain_into(&mut out);
        assert_eq!(out.len(), 2400);
        assert!(out.as_chunks::<2>().0.iter().all(|f| *f == [0.25, -0.5]));
        let packed = [1.0, 2.0, 3.0, 4.0];
        assert!(copy_buffers(&mut sink, &[NativeBuffer { data: packed.as_ptr(), samples: 4, channels: 2 }]));
        out.clear();
        source.drain_into(&mut out);
        assert_eq!(out, packed);
    }

    #[test]
    fn rejects_mismatched_buffers_without_pushing_and_overflow_drops() {
        let (mut sink, mut source) = channel(2);
        let data = [1.0, 2.0, 3.0, 4.0];
        assert!(!copy_buffers(&mut sink, &[]));
        assert!(!copy_buffers(&mut sink, &[NativeBuffer { data: data.as_ptr(), samples: 3, channels: 2 }]));
        assert!(copy_buffers(&mut sink, &[NativeBuffer { data: data.as_ptr(), samples: 4, channels: 2 }]));
        assert_eq!(source.dropped_samples(), 2);
        let mut out = Vec::new();
        source.drain_into(&mut out);
        assert_eq!(out, [1.0, 2.0]);
    }
}
