//! Linux adapter over the PulseAudio protocol, which both PulseAudio and
//! PipeWire (through `pipewire-pulse`) serve. It drives the standard `pactl`
//! and `parec` tools instead of linking libpulse, so builds need no system
//! development headers and capture runs out of process.
//!
//! Sources: sink monitors (system output), sources (microphones) and
//! sink-inputs (per-application streams via `parec --monitor-stream`).

mod pactl;
mod stream;

use parlance_audio::ring::AudioSink;
use parlance_domain::{SourceDescriptor, SourceId};

use crate::{CaptureBackend, CaptureCapabilities, CaptureError, CaptureEventHandler, CaptureStream};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `@DEFAULT_MONITOR@` or `<sink>.monitor`.
    Monitor(Option<String>),
    /// `@DEFAULT_SOURCE@` or a named source.
    Microphone(Option<String>),
    /// A sink-input index (one application's playback stream).
    Application(u32),
}

impl Target {
    pub fn parse(id: &SourceId) -> Result<Self, CaptureError> {
        let named = |rest: &str| (rest != "default").then(|| rest.to_owned());
        let raw = id.0.as_str();
        if let Some(rest) = raw.strip_prefix("system:") {
            Ok(Target::Monitor(named(rest)))
        } else if let Some(rest) = raw.strip_prefix("mic:") {
            Ok(Target::Microphone(named(rest)))
        } else if let Some(index) = raw.strip_prefix("app:").and_then(|i| i.parse().ok()) {
            Ok(Target::Application(index))
        } else {
            Err(CaptureError::SourceNotFound(raw.to_owned()))
        }
    }

    /// Arguments selecting this target for `parec`.
    pub fn parec_args(&self) -> Vec<String> {
        match self {
            Target::Monitor(None) => vec!["--device=@DEFAULT_MONITOR@".into()],
            Target::Monitor(Some(sink)) => vec![format!("--device={sink}.monitor")],
            Target::Microphone(None) => vec!["--device=@DEFAULT_SOURCE@".into()],
            Target::Microphone(Some(source)) => vec![format!("--device={source}")],
            Target::Application(index) => vec![format!("--monitor-stream={index}")],
        }
    }

    pub fn is_reconnectable(&self) -> bool {
        !matches!(self, Target::Application(_))
    }
}

#[derive(Debug, Default)]
pub struct PulseBackend;

impl CaptureBackend for PulseBackend {
    fn name(&self) -> &'static str {
        "pulse"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        let available = pactl::server_available();
        CaptureCapabilities {
            system_output: available,
            application: available,
            microphone: available,
            browser_tab: false,
        }
    }

    fn handles(&self, source: &SourceId) -> bool {
        Target::parse(source).is_ok()
    }

    fn list_sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError> {
        pactl::list_sources()
    }

    fn start(
        &self,
        source: &SourceId,
        sink: AudioSink,
        on_event: CaptureEventHandler,
    ) -> Result<Box<dyn CaptureStream>, CaptureError> {
        Ok(Box::new(stream::ParecStream::spawn(Target::parse(source)?, sink, on_event)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &str) -> Vec<String> {
        Target::parse(&SourceId(raw.into())).unwrap().parec_args()
    }

    #[test]
    fn maps_sources_to_parec_devices() {
        assert_eq!(args("system:default"), ["--device=@DEFAULT_MONITOR@"]);
        assert_eq!(args("system:alsa_output.pci"), ["--device=alsa_output.pci.monitor"]);
        assert_eq!(args("mic:default"), ["--device=@DEFAULT_SOURCE@"]);
        assert_eq!(args("app:42"), ["--monitor-stream=42"]);
    }
}
