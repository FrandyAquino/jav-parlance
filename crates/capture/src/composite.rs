use std::sync::Arc;

use parlance_audio::ring::AudioSink;
use parlance_domain::{SourceDescriptor, SourceId};

use crate::{CaptureBackend, CaptureCapabilities, CaptureError, CaptureEventHandler, CaptureStream};

/// Presents several adapters (native OS capture, browser bridge, ...) as one,
/// routing each source id to the adapter that owns it.
pub struct CompositeBackend {
    backends: Vec<Arc<dyn CaptureBackend>>,
}

impl std::fmt::Debug for CompositeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<_> = self.backends.iter().map(|b| b.name()).collect();
        f.debug_struct("CompositeBackend").field("backends", &names).finish()
    }
}

impl CompositeBackend {
    pub fn new(backends: Vec<Arc<dyn CaptureBackend>>) -> Self {
        Self { backends }
    }
}

impl CaptureBackend for CompositeBackend {
    fn name(&self) -> &'static str {
        self.backends.first().map_or("none", |b| b.name())
    }

    fn capabilities(&self) -> CaptureCapabilities {
        self.backends.iter().map(|b| b.capabilities()).fold(CaptureCapabilities::default(), |acc, c| {
            CaptureCapabilities {
                system_output: acc.system_output || c.system_output,
                application: acc.application || c.application,
                microphone: acc.microphone || c.microphone,
                browser_tab: acc.browser_tab || c.browser_tab,
            }
        })
    }

    fn handles(&self, source: &SourceId) -> bool {
        self.backends.iter().any(|b| b.handles(source))
    }

    /// A failing adapter must not hide the others' sources.
    fn list_sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError> {
        let mut sources = Vec::new();
        for backend in &self.backends {
            match backend.list_sources() {
                Ok(found) => sources.extend(found),
                Err(error) => tracing::warn!(backend = backend.name(), %error, "source listing failed"),
            }
        }
        Ok(sources)
    }

    fn start(
        &self,
        source: &SourceId,
        sink: AudioSink,
        on_event: CaptureEventHandler,
    ) -> Result<Box<dyn CaptureStream>, CaptureError> {
        let backend = self
            .backends
            .iter()
            .find(|b| b.handles(source))
            .ok_or_else(|| CaptureError::SourceNotFound(source.0.clone()))?;
        backend.start(source, sink, on_event)
    }
}
