//! WASAPI adapter: endpoint loopback, process loopback and microphones.

mod sources;
mod stream;

use parlance_audio::ring::AudioSink;
use parlance_domain::{SourceDescriptor, SourceId};

use crate::{CaptureBackend, CaptureCapabilities, CaptureError, CaptureEventHandler, CaptureStream};
use sources::Target;

#[derive(Debug, Default)]
pub struct WasapiBackend;

impl WasapiBackend {
    pub fn new() -> Self {
        Self
    }
}

impl CaptureBackend for WasapiBackend {
    fn name(&self) -> &'static str {
        "wasapi"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities {
            system_output: true,
            application: sources::supports_process_loopback(),
            microphone: true,
            browser_tab: false,
        }
    }

    fn handles(&self, source: &SourceId) -> bool {
        Target::parse(source).is_ok()
    }

    fn list_sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError> {
        sources::list()
    }

    fn start(
        &self,
        source: &SourceId,
        sink: AudioSink,
        on_event: CaptureEventHandler,
    ) -> Result<Box<dyn CaptureStream>, CaptureError> {
        let target = Target::parse(source)?;
        Ok(Box::new(stream::WasapiStream::spawn(target, sink, on_event)?))
    }
}

fn backend_error(context: &str, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Backend(format!("{context}: {error}"))
}
