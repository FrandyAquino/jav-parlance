//! Native audio capture behind a platform-independent port.
//!
//! Each adapter owns a dedicated real-time thread that only copies samples
//! into an [`AudioSink`]; it never performs inference, I/O or locking.

pub mod bridge;
mod composite;
mod error;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
pub mod replay;
#[cfg(windows)]
mod windows;

use std::sync::Arc;

use parlance_audio::StreamFormat;
use parlance_audio::ring::AudioSink;
use parlance_domain::{SourceDescriptor, SourceId};

pub use composite::CompositeBackend;
pub use error::CaptureError;

/// What the current platform adapter can capture. Reported to the UI so
/// unsupported options are hidden instead of failing at start time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureCapabilities {
    pub system_output: bool,
    pub application: bool,
    pub microphone: bool,
    pub browser_tab: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureEvent {
    /// The device disappeared; the adapter is trying to reopen it.
    DeviceLost {
        reason: String,
    },
    Reconnected,
    /// Unrecoverable failure; the stream has ended.
    Failed {
        reason: String,
    },
}

pub type CaptureEventHandler = Arc<dyn Fn(CaptureEvent) + Send + Sync>;

/// Running capture; dropping it stops the native stream and joins its thread.
pub trait CaptureStream: Send + std::fmt::Debug {
    fn format(&self) -> StreamFormat;
}

pub trait CaptureBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> CaptureCapabilities;
    /// Whether `source` belongs to this adapter (used for routing).
    fn handles(&self, source: &SourceId) -> bool;
    fn list_sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError>;
    /// Opens `source` and starts pushing interleaved f32 samples into `sink`.
    /// Returns once the device is running so the format is known.
    fn start(
        &self,
        source: &SourceId,
        sink: AudioSink,
        on_event: CaptureEventHandler,
    ) -> Result<Box<dyn CaptureStream>, CaptureError>;
}

/// Backend for the platform this binary was compiled for.
pub fn platform_backend() -> Result<Arc<dyn CaptureBackend>, CaptureError> {
    #[cfg(windows)]
    {
        Ok(Arc::new(windows::WasapiBackend::new()))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Arc::new(linux::PulseBackend))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Arc::new(macos::MacosBackend))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Err(CaptureError::Unsupported(format!("no native capture adapter for {} yet", std::env::consts::OS)))
    }
}
