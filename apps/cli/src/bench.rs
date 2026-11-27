use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Args;
use parlance_asr::metrics::word_error_rate;
use parlance_audio::StreamFormat;
use parlance_capture::replay::{ReplayBackend, ReplayClip};
use parlance_domain::{SessionState, SourceId, TranscriptSegment};
use parlance_protocol::SessionEvent;
use parlance_session::{EventSink, SessionConfig, SessionController};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::engine::FileEngine;

#[derive(Args)]
pub struct BenchArgs {
    /// 16-bit PCM WAV file (any rate, mono or stereo).
    wav: PathBuf,
    #[arg(long, default_value = "whisper-base-q8")]
    model: String,
    /// Reference transcript for WER.
    #[arg(long)]
    reference: Option<String>,
    /// Replay speed relative to real time; 0 replays as fast as possible.
    #[arg(long, default_value_t = 1.0)]
    speed: f64,
}

struct Timed {
    at: Duration,
    segment: TranscriptSegment,
}

fn load_wav(path: &PathBuf) -> anyhow::Result<ReplayClip> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 {
        bail!("only 16-bit PCM WAV is supported");
    }
    let samples: Vec<f32> =
        reader.samples::<i16>().map(|s| s.map(|v| f32::from(v) / 32_768.0)).collect::<Result<_, _>>()?;
    Ok(ReplayClip {
        name: path.display().to_string(),
        format: StreamFormat { sample_rate: spec.sample_rate, channels: spec.channels },
        samples: Arc::new(samples),
    })
}

fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}

pub async fn run(args: BenchArgs) -> anyhow::Result<()> {
    let clip = load_wav(&args.wav)?;
    let audio_seconds =
        clip.samples.len() as f64 / f64::from(clip.format.sample_rate) / f64::from(clip.format.channels);
    let speed = (args.speed > 0.0).then_some(args.speed);
    let backend = ReplayBackend::new(speed).with_clip("bench", clip);
    let engine = Arc::new(FileEngine::from_catalog(&args.model)?);

    let log: Arc<Mutex<Vec<Timed>>> = Arc::default();
    let started = Instant::now();
    let sink_log = Arc::clone(&log);
    let sink: EventSink = Arc::new(move |event| {
        if let SessionEvent::Transcript { segment } = event {
            sink_log.lock().expect("log").push(Timed { at: started.elapsed(), segment });
        }
    });

    let mut system = System::new().context("initialize process metrics")?;
    let pid = Pid::from_u32(std::process::id());
    let (mut peak_rss, mut cpu_samples) = (0u64, Vec::new());

    let controller = SessionController::new(Arc::new(backend), engine, sink);
    controller.start(SessionConfig::new(vec![SourceId("bench".into())], None), Vec::new()).await?;
    let pipeline_ready = started.elapsed();
    while controller.state() != SessionState::Idle {
        tokio::time::sleep(Duration::from_millis(250)).await;
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::everything());
        if let Some(process) = system.process(pid) {
            peak_rss = peak_rss.max(process.memory());
            cpu_samples.push(f64::from(process.cpu_usage()));
        }
    }
    let wall = started.elapsed() - pipeline_ready;
    report(&args, &log.lock().expect("log"), audio_seconds, wall, pipeline_ready, peak_rss, &cpu_samples, speed);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn report(
    args: &BenchArgs,
    log: &[Timed],
    audio_seconds: f64,
    wall: Duration,
    load: Duration,
    peak_rss: u64,
    cpu: &[f64],
    speed: Option<f64>,
) {
    let mut first_partial: HashMap<u64, Duration> = HashMap::new();
    let mut finals = Vec::new();
    for entry in log {
        first_partial.entry(entry.segment.id.0).or_insert(entry.at);
        if entry.segment.is_final() {
            finals.push(entry);
        }
    }
    let transcript: Vec<String> = finals.iter().map(|f| f.segment.text()).collect();
    let transcript = transcript.join(" ");

    println!("model          {}", args.model);
    println!("audio          {audio_seconds:.1} s, replay speed {}", speed.map_or("max".into(), |s| format!("{s}x")));
    println!("model load     {:.0} ms", load.as_secs_f64() * 1000.0);
    println!("processing RTF {:.2}", wall.as_secs_f64() / audio_seconds);
    if let Some(speed) = speed {
        // Audio position p is played at load + p / speed; latency is measured from there.
        let played = |ms: u64| load + Duration::from_secs_f64(ms as f64 / 1000.0 / speed);
        let final_lat: Vec<f64> =
            finals.iter().map(|f| (f.at.as_secs_f64() - played(f.segment.end_ms).as_secs_f64()) * 1000.0).collect();
        let partial_lat: Vec<f64> = finals
            .iter()
            .filter_map(|f| first_partial.get(&f.segment.id.0).map(|at| (*at, f.segment.start_ms)))
            .map(|(at, start)| (at.as_secs_f64() - played(start).as_secs_f64()) * 1000.0)
            .collect();
        println!(
            "first partial  p50 {:.0} ms  p95 {:.0} ms (from utterance start)",
            percentile(partial_lat.clone(), 0.5),
            percentile(partial_lat, 0.95)
        );
        println!(
            "final          p50 {:.0} ms  p95 {:.0} ms (from utterance end)",
            percentile(final_lat.clone(), 0.5),
            percentile(final_lat, 0.95)
        );
    }
    println!("segments       {} final, {} updates", finals.len(), log.len());
    println!("peak RSS       {:.0} MiB", peak_rss as f64 / 1_048_576.0);
    println!("CPU            avg {:.0}% (100% = one core)", cpu.iter().sum::<f64>() / cpu.len().max(1) as f64);
    if let Some(reference) = &args.reference {
        println!("WER            {:.3}", word_error_rate(reference, &transcript));
    }
    println!("transcript     {transcript}");
}
