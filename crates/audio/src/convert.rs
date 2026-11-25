/// Averages interleaved channels into a mono signal, appending to `out`.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16, out: &mut Vec<f32>) {
    let channels = usize::from(channels.max(1));
    if channels == 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    let scale = 1.0 / channels as f32;
    out.extend(interleaved.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() * scale));
}

/// Converts signed 16-bit PCM to normalized f32.
pub fn i16_to_f32(samples: &[i16], out: &mut Vec<f32>) {
    out.extend(samples.iter().map(|&s| f32::from(s) / 32_768.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_stereo_frames() {
        let mut out = Vec::new();
        downmix_to_mono(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2, &mut out);
        assert_eq!(out, [0.5, 0.5, 0.0]);
    }

    #[test]
    fn mono_passes_through() {
        let mut out = Vec::new();
        downmix_to_mono(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, [0.1, 0.2]);
    }

    #[test]
    fn ignores_trailing_partial_frame() {
        let mut out = Vec::new();
        downmix_to_mono(&[1.0, 1.0, 0.3], 2, &mut out);
        assert_eq!(out, [1.0]);
    }

    #[test]
    fn converts_i16_range() {
        let mut out = Vec::new();
        i16_to_f32(&[i16::MIN, 0, 16_384], &mut out);
        assert_eq!(out, [-1.0, 0.0, 0.5]);
    }
}
