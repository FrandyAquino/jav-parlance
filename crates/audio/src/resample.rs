use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::AudioError;

const CHUNK_FRAMES: usize = 1024;

/// Streaming mono resampler that accepts arbitrarily sized input blocks.
pub struct StreamResampler {
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
    scratch: Vec<f32>,
}

impl std::fmt::Debug for StreamResampler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamResampler")
            .field("passthrough", &self.inner.is_none())
            .field("pending", &self.pending.len())
            .finish()
    }
}

impl StreamResampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Result<Self, AudioError> {
        let inner = if input_rate == output_rate {
            None
        } else {
            let fft = Fft::<f32>::new(input_rate as usize, output_rate as usize, CHUNK_FRAMES, 1, FixedSync::Input)
                .map_err(|e| AudioError::Resampler(e.to_string()))?;
            Some(fft)
        };
        Ok(Self { inner, pending: Vec::with_capacity(CHUNK_FRAMES * 2), scratch: Vec::new() })
    }

    /// Resamples `input`, appending every completed output sample to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<(), AudioError> {
        let Some(resampler) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            return Ok(());
        };
        self.pending.extend_from_slice(input);

        let mut consumed = 0;
        while self.pending.len() - consumed >= resampler.input_frames_next() {
            let needed = resampler.input_frames_next();
            self.scratch.resize(resampler.output_frames_max(), 0.0);
            let source = &self.pending[consumed..consumed + needed];
            let input_adapter =
                InterleavedSlice::new(source, 1, needed).map_err(|e| AudioError::Resampler(e.to_string()))?;
            let capacity = self.scratch.len();
            let mut output_adapter = InterleavedSlice::new_mut(&mut self.scratch, 1, capacity)
                .map_err(|e| AudioError::Resampler(e.to_string()))?;
            let (read, written) = resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, None)
                .map_err(|e| AudioError::Resampler(e.to_string()))?;
            out.extend_from_slice(&self.scratch[..written]);
            consumed += read;
        }
        self.pending.drain(..consumed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, seconds: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin()).collect()
    }

    #[test]
    fn passthrough_when_rates_match() {
        let mut resampler = StreamResampler::new(16_000, 16_000).unwrap();
        let mut out = Vec::new();
        resampler.process(&[0.1, 0.2], &mut out).unwrap();
        assert_eq!(out, [0.1, 0.2]);
    }

    #[test]
    fn downsamples_48k_to_16k_with_expected_length() {
        let input = sine(48_000, 440.0, 1.0);
        let mut resampler = StreamResampler::new(48_000, 16_000).unwrap();
        let mut out = Vec::new();
        for block in input.chunks(480) {
            resampler.process(block, &mut out).unwrap();
        }
        let expected = 16_000;
        assert!(out.len() <= expected && out.len() > expected - 2 * CHUNK_FRAMES, "{}", out.len());
    }

    #[test]
    fn preserves_signal_energy() {
        let input = sine(44_100, 300.0, 1.0);
        let mut resampler = StreamResampler::new(44_100, 16_000).unwrap();
        let mut out = Vec::new();
        resampler.process(&input, &mut out).unwrap();
        let tail = &out[out.len() / 4..];
        let rms = (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt();
        assert!((rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.05, "rms {rms}");
    }
}
