//! `parlance-cli`: exercises the real pipeline without the desktop UI.
//!
//! ```text
//! parlance-cli sources
//! parlance-cli listen --source system:default --seconds 20
//! parlance-cli bench fixtures/audio/jfk.wav --reference "And so my fellow Americans..."
//! ```

mod bench;
mod engine;
mod llm;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use parlance_capture::platform_backend;
use parlance_domain::{LanguageCode, SourceId};
use parlance_models::{ModelStore, find};
use parlance_protocol::SessionEvent;
use parlance_session::{EventSink, SessionConfig, SessionController};
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List capturable audio sources.
    Sources,
    /// Transcribe a live source and print segments as they stabilize.
    Listen {
        #[arg(long, default_value = "system:default")]
        source: String,
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        #[arg(long, default_value = "whisper-base-q8")]
        model: String,
        /// Force the spoken language instead of auto-detecting.
        #[arg(long)]
        language: Option<String>,
    },
    /// Replay a 16-bit WAV through the pipeline and report latency, RTF and WER.
    Bench(bench::BenchArgs),
    /// List catalog models and their install status.
    Models,
    /// Download and verify catalog models by id.
    Download { ids: Vec<String> },
    /// Start the local LLM sidecar and measure translation latency.
    Llm(llm::LlmArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    match Cli::parse().command {
        Command::Sources => list_sources(),
        Command::Listen { source, seconds, model, language } => listen(source, seconds, &model, language).await,
        Command::Bench(args) => bench::run(args).await,
        Command::Models => list_models(),
        Command::Download { ids } => download(ids).await,
        Command::Llm(args) => llm::run(args).await,
    }
}

fn list_models() -> anyhow::Result<()> {
    let store = ModelStore::default_location()?;
    for model in store.descriptors(|_| false) {
        println!("{:<28} {:>9} MB  {:?}  {}", model.id, model.size_bytes / 1_048_576, model.status, model.name);
    }
    Ok(())
}

async fn download(ids: Vec<String>) -> anyhow::Result<()> {
    let store = ModelStore::default_location()?;
    let client = reqwest::Client::new();
    for id in ids {
        let entry = find(&id).with_context(|| format!("unknown model `{id}`"))?;
        let path = parlance_models::download(&client, &store, entry, CancellationToken::new(), |done, total| {
            eprint!("\r{id}: {:>5.1}%", done as f64 * 100.0 / total as f64);
        })
        .await?;
        eprintln!("\r{id}: verified -> {}", path.display());
    }
    Ok(())
}

fn list_sources() -> anyhow::Result<()> {
    let backend = platform_backend()?;
    println!("backend: {} {:?}", backend.name(), backend.capabilities());
    for source in backend.list_sources()? {
        println!("{:<60} {:?} {}", source.id.0, source.kind, source.name);
    }
    Ok(())
}

async fn listen(source: String, seconds: u64, model: &str, language: Option<String>) -> anyhow::Result<()> {
    let engine = Arc::new(engine::FileEngine::from_catalog(model)?);
    let sink: EventSink = Arc::new(|event| match event {
        SessionEvent::Transcript { segment } if segment.is_final() => {
            println!("[{:>6} ms] {}", segment.start_ms, segment.text());
        }
        SessionEvent::Transcript { segment } => eprintln!("  … {}", segment.text()),
        SessionEvent::Notice { notice } => eprintln!("! {}", notice.message),
        SessionEvent::LanguageDetected { language } => eprintln!("# language: {language}"),
        _ => {}
    });
    let controller = SessionController::new(platform_backend()?, engine, sink);
    let language = language.map(LanguageCode::new).transpose().context("invalid --language")?;
    controller.start(SessionConfig::new(vec![SourceId(source)], language), Vec::new()).await?;
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    if controller.state().is_active() {
        controller.stop().await?;
    }
    Ok(())
}
