//! Optional model-backed replay evidence, not native Tauri or live-device acceptance.
//! Run a release build with an existing >=120 s PCM16 WAV; no assets are downloaded.
//! Example: cargo run --release -p parlance-cli --example adaptive_copilot_probe -- /tmp/speech.wav

#[path = "../src/engine.rs"]
mod engine;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use parlance_audio::StreamFormat;
use parlance_capture::replay::{ReplayBackend, ReplayClip};
use parlance_copilot::{CopilotPreferences, CopilotService, SuggestionPolicy};
use parlance_domain::{LanguageCode, ProfileSnapshot, SessionState, SourceId, Suggestion, SuggestionStatus};
use parlance_llm::{LanguageModel, LlamaServer, ServerConfig, ServerModel, ServerStatus};
use parlance_models::{ModelStore, find};
use parlance_protocol::{AskCopilotRequest, SessionEvent};
use parlance_session::{EventSink, SessionConfig, SessionController};
use parlance_translation::{LlmTranslator, TranslationWorker, Translator};
use serde_json::{Value, json};

/// Contract requires at least two minutes of identical real-time replay per condition.
const MIN_AUDIO_SECONDS: f64 = 120.0;
/// Bound shutdown/drain even when the server stops answering or translation drops lines.
const DRAIN_SECONDS: u64 = 120;
/// Frequent collection prevents measurement bookkeeping from obstructing core worker threads.
const POLL_MS: u64 = 50;

#[derive(Debug, Parser)]
struct Args {
    /// Existing PCM16 speech WAV, or PARLANCE_PROBE_WAV. Missing prerequisites skip cleanly.
    wav: Option<PathBuf>,
    #[arg(long, default_value = "whisper-base-q8")]
    speech_model: String,
    #[arg(long, default_value = "qwen3.5-2b-q4")]
    llm_model: String,
    /// Three pairs support repeatability; use one for explicitly preliminary evidence.
    #[arg(long, default_value_t = 3)]
    repetitions: usize,
    #[arg(long)]
    cpu: bool,
}

#[derive(Default)]
struct RequestTiming {
    submitted_ms: f64,
    first_token_ms: Option<f64>,
    terminal_ms: Option<f64>,
    latest: Option<Suggestion>,
}

#[derive(Default)]
struct Observations {
    // segment id -> (audio end, publication time), both in milliseconds.
    transcripts: BTreeMap<u64, (u64, f64)>,
    translations: BTreeMap<u64, f64>,
    empty_translations: BTreeSet<u64>,
    requests: BTreeMap<String, RequestTiming>,
    notices: Vec<Value>,
    states: Vec<Value>,
    transcript_updates: usize,
}

impl Observations {
    fn record(&mut self, at_ms: f64, event: SessionEvent) {
        match event {
            SessionEvent::Transcript { segment } => {
                self.transcript_updates += 1;
                if segment.is_final() && !segment.text().trim().is_empty() {
                    self.transcripts.entry(segment.id.0).or_insert((segment.end_ms, at_ms));
                }
            }
            SessionEvent::Translation { translation } if translation.is_final => {
                if translation.text.trim().is_empty() {
                    self.empty_translations.insert(translation.segment_id.0);
                } else {
                    self.translations.entry(translation.segment_id.0).or_insert(at_ms);
                }
            }
            SessionEvent::Suggestion { suggestion } => {
                if let Some(request) = self.requests.get_mut(&suggestion.request_id) {
                    if !suggestion.reply_in_speaker_language.is_empty() {
                        request.first_token_ms.get_or_insert(at_ms);
                    }
                    if matches!(
                        suggestion.status,
                        SuggestionStatus::Complete | SuggestionStatus::Failed | SuggestionStatus::Cancelled
                    ) {
                        request.terminal_ms.get_or_insert(at_ms);
                    }
                    request.latest = Some(suggestion);
                }
            }
            SessionEvent::Notice { notice } => self.notices.push(json!({"at_ms": at_ms, "notice": notice})),
            SessionEvent::StateChanged { state } => self.states.push(json!({"at_ms": at_ms, "state": state})),
            _ => {}
        }
    }

    fn drained(&self) -> bool {
        !self.transcripts.is_empty()
            && self
                .transcripts
                .keys()
                .all(|id| self.translations.contains_key(id) || self.empty_translations.contains(id))
            && self.requests.values().all(|request| request.terminal_ms.is_some())
    }

    fn report(&self, ready_ms: f64) -> Value {
        let transcript_lags: Vec<_> =
            self.transcripts.values().map(|(end_ms, at)| at - ready_ms - *end_ms as f64).collect();
        let translation_lags: Vec<_> = self
            .transcripts
            .iter()
            .filter_map(|(id, (end_ms, _))| self.translations.get(id).map(|at| at - ready_ms - *end_ms as f64))
            .collect();
        let requests: Vec<_> = self
            .requests
            .iter()
            .map(|(id, timing)| {
                json!({
                    "request_id": id,
                    "submitted_after_ready_ms": timing.submitted_ms - ready_ms,
                    "ttft_ms": timing.first_token_ms.map(|at| at - timing.submitted_ms),
                    "total_ms": timing.terminal_ms.map(|at| at - timing.submitted_ms),
                    "result": timing.latest,
                })
            })
            .collect();
        json!({
            "transcript": metric(&transcript_lags),
            "translation": metric(&translation_lags),
            "transcript_updates": self.transcript_updates,
            "missing_final_translation_ids": self.transcripts.keys().filter(|id| !self.translations.contains_key(id)).collect::<Vec<_>>(),
            "empty_final_translation_ids": self.empty_translations,
            "final_segment_end_ms": self.transcripts.values().map(|(end, _)| end).collect::<Vec<_>>(),
            "requests": requests, "notices": self.notices, "states": self.states,
        })
    }
}

fn metric(values: &[f64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    // Nearest-rank p95; empty samples stay null, never zero or invented latency.
    let p95 = (!sorted.is_empty()).then(|| sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1]);
    json!({"sample_count": values.len(), "p95_ms": p95, "samples_ms": values})
}

fn load_wav(path: &Path) -> anyhow::Result<ReplayClip> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int || spec.channels == 0 {
        bail!("probe requires PCM16 WAV with at least one channel");
    }
    let samples =
        reader.samples::<i16>().map(|sample| sample.map(|v| f32::from(v) / 32768.0)).collect::<Result<Vec<_>, _>>()?;
    Ok(ReplayClip {
        name: path.display().to_string(),
        format: StreamFormat { sample_rate: spec.sample_rate, channels: spec.channels },
        samples: Arc::new(samples),
    })
}

fn manual_request(index: usize) -> anyhow::Result<(AskCopilotRequest, Option<ProfileSnapshot>)> {
    let profile = (index == 0).then(|| ProfileSnapshot {
        id: "probe-redis".into(),
        name: "Replay Redis context".into(),
        revision: 1,
        content: "Our team uses Redis to cache database reads. No cache TTL has been documented.".into(),
    });
    let (question, idea) = match index {
        0 => ("What do we use to cache database reads, and what exact TTL has our team configured?", ""),
        1 => ("How does cache-aside work, and what happens on a cache miss?", ""),
        _ => (
            "How should we correct this proposed cache design?",
            "Set no expiration and never invalidate cached records, even after database updates, so reads are always correct.",
        ),
    };
    Ok((
        AskCopilotRequest {
            request_id: format!("probe-manual-{}", index + 1),
            question: question.into(),
            idea: idea.into(),
            reply_language: LanguageCode::new("en")?,
            user_language: LanguageCode::new("es")?,
            profile_id: profile.as_ref().map(|profile| profile.id.clone()),
        },
        profile,
    ))
}

async fn condition(
    args: &Args,
    clip: ReplayClip,
    config: ServerConfig,
    audio_seconds: f64,
    manual: bool,
) -> anyhow::Result<Value> {
    let server = LlamaServer::spawn(config);
    let server_started = Instant::now();
    if let Err(error) = server.endpoint(Duration::from_secs(180)).await {
        server.shutdown_and_wait().await;
        return Err(error.into());
    }
    let server_ready_ms = server_started.elapsed().as_secs_f64() * 1000.0;
    let gpu = matches!(server.status(), ServerStatus::Ready { gpu: true, .. });
    let model: Arc<dyn LanguageModel> = Arc::new(ServerModel::new(server.clone()));
    let translator: Arc<dyn Translator> = Arc::new(LlmTranslator::new(model.clone()));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let start = Instant::now();
    let sink: EventSink = Arc::new(move |event| {
        let _ = tx.send((start.elapsed().as_secs_f64() * 1000.0, event));
    });
    let service = CopilotService::new(
        model,
        translator.clone(),
        sink.clone(),
        SuggestionPolicy::default(),
        CopilotPreferences {
            enabled: false,
            source_language: Some(LanguageCode::new("en")?),
            user_language: LanguageCode::new("es")?,
        },
    );
    let worker = TranslationWorker::spawn(translator, LanguageCode::new("es")?, sink.clone());
    let backend = ReplayBackend::new(Some(1.0)).with_clip("probe", clip);
    let controller = SessionController::new(
        Arc::new(backend),
        Arc::new(engine::FileEngine::from_catalog(&args.speech_model)?),
        sink,
    );
    let result = async {
        controller
            .start(
                SessionConfig::new(vec![SourceId("probe".into())], Some(LanguageCode::new("en")?)),
                vec![worker, service.session_listener(Arc::new(|| None))],
            )
            .await?;
        let ready_ms = start.elapsed().as_secs_f64() * 1000.0;
        let ready = Instant::now();
        let mut log = Observations::default();
        let mut next_question = 0;
        let schedule = [0.20, 0.45, 0.70];
        let mut idle_at = None;
        let mut timed_out = false;
        loop {
            while let Ok((at, event)) = rx.try_recv() {
                log.record(at, event);
            }
            if manual
                && next_question < schedule.len()
                && ready.elapsed().as_secs_f64() >= audio_seconds * schedule[next_question]
            {
                let (request, profile) = manual_request(next_question)?;
                log.requests.insert(
                    request.request_id.clone(),
                    RequestTiming { submitted_ms: start.elapsed().as_secs_f64() * 1000.0, ..Default::default() },
                );
                service.submit_manual(request, profile)?;
                next_question += 1;
            }
            if controller.state() == SessionState::Idle {
                let idle = *idle_at.get_or_insert_with(Instant::now);
                if log.drained() {
                    break;
                }
                if idle.elapsed() >= Duration::from_secs(DRAIN_SECONDS) {
                    timed_out = true;
                    break;
                }
            }
            if ready.elapsed().as_secs_f64() > audio_seconds + DRAIN_SECONDS as f64 * 2.0 {
                timed_out = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
        }
        let mut report = log.report(ready_ms);
        report["condition"] = json!(if manual { "three_manual_questions" } else { "baseline" });
        report["pipeline_ready_ms"] = json!(ready_ms);
        report["server_ready_ms"] = json!(server_ready_ms);
        report["gpu_offload"] = json!(gpu);
        report["wall_after_ready_ms"] = json!(ready.elapsed().as_secs_f64() * 1000.0);
        report["drain_timed_out"] = json!(timed_out);
        report["manual_questions_submitted"] = json!(next_question);
        Ok::<_, anyhow::Error>(report)
    }
    .await;
    if controller.state() != SessionState::Idle {
        let _ = controller.stop().await;
    }
    service.shutdown().await;
    server.shutdown_and_wait().await;
    result
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let wav = args.wav.clone().or_else(|| std::env::var_os("PARLANCE_PROBE_WAV").map(PathBuf::from));
    let Some(wav) = wav.filter(|path| path.is_file()) else {
        println!(
            "{}",
            json!({"status":"skipped", "reason":"Supply an existing >=120s speech WAV path or PARLANCE_PROBE_WAV"})
        );
        return Ok(());
    };
    let store = ModelStore::default_location()?;
    let installed = || -> anyhow::Result<_> {
        engine::FileEngine::from_catalog(&args.speech_model)?;
        let runtime = find("llama-server").context("runtime absent from catalog")?;
        let executable = store.find_executable(runtime, "llama-server").context("llama-server is not installed")?;
        let model = store.require(find(&args.llm_model).context("unknown LLM catalog ID")?)?;
        Ok(ServerConfig::new(executable, model))
    };
    let mut config = match installed() {
        Ok(config) => config,
        Err(error) => {
            println!("{}", json!({"status":"skipped", "reason":error.to_string()}));
            return Ok(());
        }
    };
    if args.cpu {
        config.gpu_layers = 0;
    }
    let clip = load_wav(&wav)?;
    let audio_seconds =
        clip.samples.len() as f64 / f64::from(clip.format.sample_rate) / f64::from(clip.format.channels);
    if audio_seconds < MIN_AUDIO_SECONDS || args.repetitions == 0 {
        bail!("requires >=120 seconds of audio and at least one repetition");
    }
    let mut hardware = sysinfo::System::new_all();
    hardware.refresh_memory();
    println!(
        "{}",
        json!({
            "type":"metadata", "scope":"model-backed real-time replay; not Tauri or live-device acceptance",
            "wav":wav, "audio_seconds":audio_seconds, "repetitions":args.repetitions,
            "speech_model":args.speech_model, "llm_model":args.llm_model,
            "os":sysinfo::System::long_os_version(), "cpu":hardware.cpus().first().map(|cpu| cpu.brand()),
            "logical_cpus":hardware.cpus().len(), "memory_bytes":hardware.total_memory(),
            "llm_threads":config.threads, "llm_parallel_slots":config.parallel,
            "latency_definition":"publication instant - start-return ready instant - final segment end_ms; 1x replay",
            "ttft_definition":"first nonempty speaker-language suggestion publication - submission; includes queueing",
            "limitations":"No live-device gaps assertion. Inspect notices and state transitions. Model answers require human semantic review. One pair is preliminary; no inferred medians."
        })
    );
    for repetition in 1..=args.repetitions {
        let baseline = condition(&args, clip.clone(), config.clone(), audio_seconds, false).await?;
        println!("{}", json!({"type":"condition", "repetition":repetition, "evidence":baseline}));
        let manual = condition(&args, clip.clone(), config.clone(), audio_seconds, true).await?;
        println!("{}", json!({"type":"condition", "repetition":repetition, "evidence":manual}));
        let delta =
            |key: &str| -> Option<f64> { Some(manual[key]["p95_ms"].as_f64()? - baseline[key]["p95_ms"].as_f64()?) };
        println!(
            "{}",
            json!({"type":"comparison", "repetition":repetition,
                "transcript_p95_delta_ms":delta("transcript"), "transcript_budget_ms":250,
                "translation_p95_delta_ms":delta("translation"), "translation_budget_ms":1000,
                "note":"Evaluate deltas together with sample counts, missing translations, request terminals, notices and timeout flags."
            })
        );
    }
    Ok(())
}
