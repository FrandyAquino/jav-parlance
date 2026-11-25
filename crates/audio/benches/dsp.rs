use criterion::{Criterion, criterion_group, criterion_main};
use parlance_audio::convert::downmix_to_mono;
use parlance_audio::resample::StreamResampler;

fn bench_normalization(c: &mut Criterion) {
    let stereo_10ms: Vec<f32> = (0..960).map(|i| (i as f32 * 0.01).sin()).collect();

    c.bench_function("downmix 10ms stereo 48k", |b| {
        let mut out = Vec::with_capacity(480);
        b.iter(|| {
            out.clear();
            downmix_to_mono(&stereo_10ms, 2, &mut out);
        })
    });

    c.bench_function("resample 10ms 48k->16k", |b| {
        let mut resampler = StreamResampler::new(48_000, 16_000).expect("resampler");
        let mono = &stereo_10ms[..480];
        let mut out = Vec::with_capacity(4096);
        b.iter(|| {
            out.clear();
            resampler.process(mono, &mut out).expect("resample");
        })
    });
}

criterion_group!(benches, bench_normalization);
criterion_main!(benches);
