//! Audio plumbing between native capture callbacks and inference.
//!
//! Everything reachable from a capture callback ([`ring::AudioSink`]) is
//! allocation-free and lock-free. Heavier work (resampling, metering, VAD)
//! runs on the consumer side.

pub mod convert;
pub mod level;
pub mod mix;
pub mod resample;
pub mod ring;
pub mod vad;

/// Sample rate expected by Whisper and Silero VAD.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("unsupported stream format: {0:?}")]
    UnsupportedFormat(StreamFormat),
    #[error("resampler failure: {0}")]
    Resampler(String),
}
