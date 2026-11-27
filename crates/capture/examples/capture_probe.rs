use std::sync::Arc;
use std::time::{Duration, Instant};

use parlance_audio::ring::channel;
use parlance_capture::platform_backend;
use parlance_domain::SourceId;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = platform_backend()?;
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 1 {
        for source in backend.list_sources()? {
            println!("{}\t{}", source.id.0, source.name);
        }
        return Ok(());
    }
    let source = SourceId(args[1].clone());
    let seconds = args.get(2).map(|v| v.parse::<u64>()).transpose()?.unwrap_or(10);
    let (sink, mut audio) = channel(192_000);
    let stream = backend.start(&source, sink, Arc::new(|event| eprintln!("{event:?}")))?;
    println!("capturing {}: {:?}", source.0, stream.format());
    let started = Instant::now();
    let mut samples = Vec::with_capacity(192_000);
    let (mut count, mut nonzero, mut peak) = (0, 0, 0.0_f32);
    while started.elapsed() < Duration::from_secs(seconds) && audio.is_producer_alive() {
        samples.clear();
        audio.drain_into(&mut samples);
        count += samples.len();
        nonzero += samples.iter().filter(|v| v.abs() > 0.00001).count();
        peak = samples.iter().fold(peak, |peak, value| peak.max(value.abs()));
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stream);
    println!("samples={count} nonzero={nonzero} peak={peak:.6} dropped={}", audio.dropped_samples());
    if count == 0 {
        return Err("no audio callbacks received; check device and capture permission".into());
    }
    Ok(())
}
