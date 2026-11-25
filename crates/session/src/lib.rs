//! Session lifecycle and the real-time pipeline:
//!
//! ```text
//! native capture ──ring──▶ DSP thread ──bounded──▶ ASR thread ──▶ events
//!  (per source)            normalize, mix,          streaming        │
//!                          meter, VAD gate          whisper          └─▶ segment listeners
//!                                                                        (translation, copilot)
//! ```

mod asr_worker;
mod controller;
mod dsp;
mod pipeline;
mod ports;

pub use controller::{SessionConfig, SessionController, SessionError};
pub use ports::{EventSink, SegmentListener, SpeechEngine};
