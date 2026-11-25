/// Aligns and sums several 16 kHz mono streams (e.g. loopback + microphone).
///
/// Device clocks are independent, so inputs arrive in uneven bursts. Output
/// advances only as far as every input has delivered, except that an input
/// lagging more than `max_lag` samples is padded with silence so a stalled
/// device never freezes the whole pipeline.
#[derive(Debug)]
pub struct StreamMixer {
    inputs: Vec<Vec<f32>>,
    max_lag: usize,
}

impl StreamMixer {
    pub fn new(inputs: usize, max_lag: usize) -> Self {
        Self { inputs: vec![Vec::new(); inputs.max(1)], max_lag }
    }

    pub fn push(&mut self, input: usize, samples: &[f32]) {
        self.inputs[input].extend_from_slice(samples);
    }

    /// Appends every sample that can be mixed now to `out`.
    pub fn drain_into(&mut self, out: &mut Vec<f32>) {
        if self.inputs.len() == 1 {
            out.append(&mut self.inputs[0]);
            return;
        }
        let longest = self.inputs.iter().map(Vec::len).max().unwrap_or(0);
        for buffer in &mut self.inputs {
            if longest - buffer.len() > self.max_lag {
                buffer.resize(longest - self.max_lag, 0.0);
            }
        }
        let ready = self.inputs.iter().map(Vec::len).min().unwrap_or(0);
        let start = out.len();
        out.resize(start + ready, 0.0);
        for buffer in &mut self.inputs {
            for (mixed, sample) in out[start..].iter_mut().zip(buffer.drain(..ready)) {
                *mixed += sample;
            }
        }
        for mixed in &mut out[start..] {
            *mixed = mixed.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(mixer: &mut StreamMixer) -> Vec<f32> {
        let mut out = Vec::new();
        mixer.drain_into(&mut out);
        out
    }

    #[test]
    fn single_input_passes_through() {
        let mut mixer = StreamMixer::new(1, 10);
        mixer.push(0, &[0.1, 0.2]);
        assert_eq!(drain(&mut mixer), [0.1, 0.2]);
    }

    #[test]
    fn sums_aligned_samples_and_keeps_remainder() {
        let mut mixer = StreamMixer::new(2, 100);
        mixer.push(0, &[0.1, 0.2, 0.3]);
        mixer.push(1, &[0.1, 0.1]);
        assert_eq!(drain(&mut mixer), [0.2, 0.3]);
        mixer.push(1, &[0.5]);
        assert_eq!(drain(&mut mixer), [0.8]);
    }

    #[test]
    fn pads_stalled_input_beyond_max_lag() {
        let mut mixer = StreamMixer::new(2, 2);
        mixer.push(0, &[0.5; 5]);
        assert_eq!(drain(&mut mixer), [0.5; 3]);
    }

    #[test]
    fn clamps_to_full_scale() {
        let mut mixer = StreamMixer::new(2, 10);
        mixer.push(0, &[0.9]);
        mixer.push(1, &[0.9]);
        assert_eq!(drain(&mut mixer), [1.0]);
    }
}
