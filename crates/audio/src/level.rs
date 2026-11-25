use parlance_domain::AudioLevel;

fn to_db(amplitude: f32) -> f32 {
    if amplitude <= 1e-5 { AudioLevel::SILENCE_DB } else { 20.0 * amplitude.log10() }
}

/// Accumulates samples and emits one [`AudioLevel`] per `window` samples.
#[derive(Debug)]
pub struct LevelMeter {
    window: usize,
    count: usize,
    sum_squares: f64,
    peak: f32,
}

impl LevelMeter {
    pub fn new(window: usize) -> Self {
        Self { window: window.max(1), count: 0, sum_squares: 0.0, peak: 0.0 }
    }

    /// Feeds mono samples; calls `emit` each time a window completes.
    pub fn feed(&mut self, samples: &[f32], mut emit: impl FnMut(AudioLevel)) {
        for &sample in samples {
            self.sum_squares += f64::from(sample) * f64::from(sample);
            self.peak = self.peak.max(sample.abs());
            self.count += 1;
            if self.count == self.window {
                emit(self.take());
            }
        }
    }

    fn take(&mut self) -> AudioLevel {
        let rms = (self.sum_squares / self.count as f64).sqrt() as f32;
        let level = AudioLevel { rms_db: to_db(rms), peak_db: to_db(self.peak) };
        self.count = 0;
        self.sum_squares = 0.0;
        self.peak = 0.0;
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measure(samples: &[f32], window: usize) -> Vec<AudioLevel> {
        let mut meter = LevelMeter::new(window);
        let mut levels = Vec::new();
        meter.feed(samples, |level| levels.push(level));
        levels
    }

    #[test]
    fn silence_reports_floor() {
        let levels = measure(&[0.0; 100], 100);
        assert_eq!(levels, [AudioLevel::silent()]);
    }

    #[test]
    fn full_scale_square_wave_is_zero_db() {
        let wave: Vec<f32> = (0..100).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let level = measure(&wave, 100)[0];
        assert!(level.rms_db.abs() < 1e-3);
        assert!(level.peak_db.abs() < 1e-3);
    }

    #[test]
    fn emits_once_per_window_across_calls() {
        let mut meter = LevelMeter::new(10);
        let mut count = 0;
        meter.feed(&[0.5; 7], |_| count += 1);
        meter.feed(&[0.5; 7], |_| count += 1);
        assert_eq!(count, 1);
    }
}
