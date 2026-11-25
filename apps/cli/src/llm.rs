use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Args;
use parlance_domain::LanguageCode;
use parlance_llm::{LanguageModel, LlamaServer, ServerConfig, ServerModel, ServerStatus};
use parlance_models::{ModelStore, find};
use parlance_translation::{LlmTranslator, TranslationRequest, Translator};
use tokio_util::sync::CancellationToken;

#[derive(Args)]
pub struct LlmArgs {
    /// Sentence to translate.
    text: String,
    #[arg(long, default_value = "en")]
    from: String,
    #[arg(long, default_value = "es")]
    to: String,
    #[arg(long, default_value = "qwen3.5-2b-q4")]
    model: String,
    /// Force CPU inference.
    #[arg(long)]
    cpu: bool,
}

pub async fn run(args: LlmArgs) -> anyhow::Result<()> {
    let store = ModelStore::default_location()?;
    let runtime = find("llama-server").context("runtime missing from catalog")?;
    let executable = store.find_executable(runtime, "llama-server").context("download `llama-server` first")?;
    let model_entry = find(&args.model).with_context(|| format!("unknown model `{}`", args.model))?;
    let model_path = store.require(model_entry).with_context(|| format!("download `{}` first", args.model))?;

    let mut config = ServerConfig::new(executable, model_path);
    if args.cpu {
        config.gpu_layers = 0;
    }
    let started = Instant::now();
    let server = LlamaServer::spawn(config);
    server.endpoint(Duration::from_secs(180)).await?;
    let gpu = matches!(server.status(), ServerStatus::Ready { gpu: true, .. });
    println!("server ready in {:.1} s (gpu offload: {gpu})", started.elapsed().as_secs_f64());

    let model: Arc<dyn LanguageModel> = Arc::new(ServerModel::new(Arc::clone(&server)));
    let translator = LlmTranslator::new(model);
    for attempt in ["cold", "warm"] {
        let request = TranslationRequest {
            text: args.text.clone(),
            source: Some(LanguageCode::new(&args.from)?),
            target: LanguageCode::new(&args.to)?,
            context: Vec::new(),
        };
        let first_token: Arc<Mutex<Option<Duration>>> = Arc::default();
        let begin = Instant::now();
        let marker = Arc::clone(&first_token);
        let on_token = move |_: &str| {
            marker.lock().expect("marker").get_or_insert_with(|| begin.elapsed());
        };
        let text = translator.translate(request, CancellationToken::new(), &on_token).await?;
        let ttft = first_token.lock().expect("marker").unwrap_or_default();
        println!(
            "{attempt:<5} first token {:>5.0} ms  total {:>5.0} ms  → {text}",
            ttft.as_secs_f64() * 1000.0,
            begin.elapsed().as_secs_f64() * 1000.0
        );
    }
    server.shutdown();
    Ok(())
}
