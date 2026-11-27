use std::sync::{Arc, Mutex, MutexGuard};

use parlance_asr::{AsrError, StreamingConfig};
use parlance_audio::AudioError;
use parlance_audio::vad::GateConfig;
use parlance_capture::{CaptureBackend, CaptureError};
use parlance_domain::{LanguageCode, SessionCommand, SessionState, SourceId, TransitionError};
use parlance_protocol::{Notice, SessionEvent};
use tokio::sync::oneshot;

use crate::pipeline::{PipelineSpec, RunningPipeline};
use crate::ports::{EventSink, SegmentListener, SpeechEngine};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    InvalidTransition(#[from] TransitionError),
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error(transparent)]
    Speech(#[from] AsrError),
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Primary source first; additional sources (e.g. microphone) are mixed in.
    pub sources: Vec<SourceId>,
    pub source_language: Option<LanguageCode>,
    pub gate: GateConfig,
    pub streaming: StreamingConfig,
}

impl SessionConfig {
    pub fn new(sources: Vec<SourceId>, source_language: Option<LanguageCode>) -> Self {
        let streaming = StreamingConfig { language: source_language.clone(), ..StreamingConfig::default() };
        Self { sources, source_language, gate: GateConfig::default(), streaming }
    }
}

struct Inner {
    state: SessionState,
    pipeline: Option<RunningPipeline>,
    /// Incremented per session so a stale "pipeline finished" signal from a
    /// previous session never stops the current one.
    generation: u64,
}

/// Owns the session state machine and the running pipeline.
#[derive(Clone)]
pub struct SessionController {
    inner: Arc<Mutex<Inner>>,
    capture: Arc<dyn CaptureBackend>,
    engine: Arc<dyn SpeechEngine>,
    events: EventSink,
}

impl std::fmt::Debug for SessionController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionController").field("state", &self.state()).finish()
    }
}

impl SessionController {
    pub fn new(capture: Arc<dyn CaptureBackend>, engine: Arc<dyn SpeechEngine>, events: EventSink) -> Self {
        let inner = Inner { state: SessionState::Idle, pipeline: None, generation: 0 };
        Self { inner: Arc::new(Mutex::new(inner)), capture, engine, events }
    }

    pub fn state(&self) -> SessionState {
        self.lock().state.clone()
    }

    pub async fn start(
        &self,
        config: SessionConfig,
        listeners: Vec<Arc<dyn SegmentListener>>,
    ) -> Result<(), SessionError> {
        if config.sources.is_empty() {
            return Err(SessionError::Internal("no audio source selected".into()));
        }
        let generation = {
            let mut inner = self.lock();
            self.transition(&mut inner, SessionCommand::Start)?;
            inner.generation += 1;
            inner.generation
        };

        let (finished_tx, finished_rx) = oneshot::channel();
        let (capture, engine, events) = (Arc::clone(&self.capture), Arc::clone(&self.engine), Arc::clone(&self.events));
        let spec = PipelineSpec { sources: config.sources, streaming: config.streaming, gate: config.gate };
        let started = tokio::task::spawn_blocking(move || {
            RunningPipeline::start(spec, capture.as_ref(), engine.as_ref(), events, listeners, finished_tx)
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))
        .and_then(|result| result);

        let mut inner = self.lock();
        match started {
            Ok(pipeline) => {
                inner.pipeline = Some(pipeline);
                self.transition(&mut inner, SessionCommand::MarkRunning)?;
                drop(inner);
                self.watch_pipeline(generation, finished_rx);
                Ok(())
            }
            Err(error) => {
                self.transition(&mut inner, SessionCommand::Fail(error.to_string()))?;
                Err(error)
            }
        }
    }

    pub async fn stop(&self) -> Result<(), SessionError> {
        let pipeline = {
            let mut inner = self.lock();
            // While starting, the pipeline is still being built off-thread;
            // stopping now would leave it running behind an Idle state.
            if inner.state == SessionState::Starting {
                return Err(SessionError::Internal("the session is still starting; try again in a moment".into()));
            }
            self.transition(&mut inner, SessionCommand::Stop)?;
            inner.pipeline.take()
        };
        if let Some(pipeline) = pipeline {
            tokio::task::spawn_blocking(move || pipeline.shutdown())
                .await
                .map_err(|e| SessionError::Internal(e.to_string()))?;
        }
        let mut inner = self.lock();
        self.transition(&mut inner, SessionCommand::MarkStopped)?;
        Ok(())
    }

    pub fn pause(&self) -> Result<(), SessionError> {
        self.set_paused(SessionCommand::Pause, true)
    }

    pub fn resume(&self) -> Result<(), SessionError> {
        self.set_paused(SessionCommand::Resume, false)
    }

    fn set_paused(&self, command: SessionCommand, paused: bool) -> Result<(), SessionError> {
        let mut inner = self.lock();
        self.transition(&mut inner, command)?;
        if let Some(pipeline) = &inner.pipeline {
            pipeline.set_paused(paused);
        }
        Ok(())
    }

    /// Stops the session on its own when every capture stream ends (e.g. the
    /// captured application quit or the device vanished for good).
    fn watch_pipeline(&self, generation: u64, finished: oneshot::Receiver<()>) {
        let controller = self.clone();
        tokio::spawn(async move {
            if finished.await.is_err() {
                return;
            }
            let still_current = {
                let inner = controller.lock();
                inner.generation == generation && inner.state.is_active()
            };
            if still_current {
                (controller.events)(SessionEvent::Notice {
                    notice: Notice::warning("Audio pipeline ended; session stopped"),
                });
                if let Err(error) = controller.stop().await {
                    tracing::warn!(%error, "automatic stop failed");
                }
            }
        });
    }

    fn transition(&self, inner: &mut Inner, command: SessionCommand) -> Result<(), TransitionError> {
        inner.state = inner.state.apply(command)?;
        (self.events)(SessionEvent::StateChanged { state: inner.state.clone() });
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A poisoned lock means a panic mid-transition; the state value itself
        // is always consistent, so recover rather than cascade the panic.
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
