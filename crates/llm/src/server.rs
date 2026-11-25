use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::LlmError;

const READY_TIMEOUT: Duration = Duration::from_secs(180);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Consecutive GPU start failures before retrying on CPU only.
const GPU_ATTEMPTS: u32 = 2;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub executable: PathBuf,
    pub model: PathBuf,
    /// Total context, shared by `parallel` slots.
    pub context_tokens: u32,
    pub parallel: u32,
    /// Layers offloaded to the GPU; 0 forces CPU.
    pub gpu_layers: u32,
    pub threads: usize,
}

impl ServerConfig {
    pub fn new(executable: PathBuf, model: PathBuf) -> Self {
        let cores = std::thread::available_parallelism().map_or(4, usize::from);
        // Speech recognition is latency-critical and CPU-bound, so the LLM
        // gets a minority of cores and relies on GPU offload where possible.
        Self { executable, model, context_tokens: 8_192, parallel: 2, gpu_layers: 99, threads: (cores / 4).max(2) }
    }

    fn args(&self, port: u16, api_key: &str, gpu_layers: u32) -> Vec<String> {
        [
            "--model",
            &self.model.to_string_lossy(),
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            api_key,
            "--ctx-size",
            &self.context_tokens.to_string(),
            "--parallel",
            &self.parallel.to_string(),
            "--n-gpu-layers",
            &gpu_layers.to_string(),
            "--threads",
            &self.threads.to_string(),
            // Only the primary GPU: splitting layers onto an integrated GPU is slower.
            "--split-mode",
            "none",
            "--reasoning",
            "off",
            "--reasoning-budget",
            "0",
            "--jinja",
            "--no-webui",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    Starting,
    Ready { endpoint: Endpoint, gpu: bool },
    Restarting { reason: String },
    Stopped,
}

/// Supervises one `llama-server` process: picks a free loopback port and a
/// random API key, waits for `/health`, restarts with exponential backoff,
/// falls back to CPU when GPU start-up keeps failing, and kills the child on
/// shutdown or drop.
#[derive(Debug)]
pub struct LlamaServer {
    status: watch::Receiver<ServerStatus>,
    shutdown: CancellationToken,
}

impl LlamaServer {
    pub fn spawn(config: ServerConfig) -> Arc<Self> {
        let (tx, rx) = watch::channel(ServerStatus::Starting);
        let shutdown = CancellationToken::new();
        tokio::spawn(supervise(config, tx, shutdown.clone()));
        Arc::new(Self { status: rx, shutdown })
    }

    pub fn status(&self) -> ServerStatus {
        self.status.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<ServerStatus> {
        self.status.clone()
    }

    /// Waits until the server is ready (or fails) and returns its endpoint.
    pub async fn endpoint(&self, timeout: Duration) -> Result<Endpoint, LlmError> {
        let mut status = self.status.clone();
        let wait = status.wait_for(|s| matches!(s, ServerStatus::Ready { .. } | ServerStatus::Stopped));
        match tokio::time::timeout(timeout, wait).await {
            Ok(Ok(guard)) => match &*guard {
                ServerStatus::Ready { endpoint, .. } => Ok(endpoint.clone()),
                _ => Err(LlmError::Unavailable("language model server stopped".into())),
            },
            Ok(Err(_)) => Err(LlmError::Unavailable("language model supervisor ended".into())),
            Err(_) => Err(LlmError::Unavailable("language model is still loading".into())),
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn supervise(config: ServerConfig, status: watch::Sender<ServerStatus>, shutdown: CancellationToken) {
    let http = reqwest::Client::new();
    let mut backoff = Duration::from_secs(1);
    let mut gpu_failures = 0;

    while !shutdown.is_cancelled() {
        let gpu_layers = if gpu_failures >= GPU_ATTEMPTS { 0 } else { config.gpu_layers };
        let outcome = run_once(&config, gpu_layers, &http, &status, &shutdown).await;
        let reason = match outcome {
            RunOutcome::Shutdown => break,
            RunOutcome::FailedToStart(reason) => {
                if gpu_layers > 0 {
                    gpu_failures += 1;
                }
                reason
            }
            RunOutcome::Exited(reason) => {
                backoff = Duration::from_secs(1);
                reason
            }
        };
        tracing::warn!(%reason, "llama-server stopped; restarting in {backoff:?}");
        let _ = status.send(ServerStatus::Restarting { reason });
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
    let _ = status.send(ServerStatus::Stopped);
}

enum RunOutcome {
    Shutdown,
    FailedToStart(String),
    Exited(String),
}

async fn run_once(
    config: &ServerConfig,
    gpu_layers: u32,
    http: &reqwest::Client,
    status: &watch::Sender<ServerStatus>,
    shutdown: &CancellationToken,
) -> RunOutcome {
    let port = match free_port() {
        Ok(port) => port,
        Err(e) => return RunOutcome::FailedToStart(format!("no free port: {e}")),
    };
    let endpoint = Endpoint { base_url: format!("http://127.0.0.1:{port}"), api_key: random_key() };
    let mut child = match spawn_child(config, port, &endpoint.api_key, gpu_layers) {
        Ok(child) => child,
        Err(e) => return RunOutcome::FailedToStart(format!("cannot launch {}: {e}", config.executable.display())),
    };
    #[cfg(windows)]
    let _job = child.raw_handle().and_then(|handle| {
        crate::job::KillOnClose::assign(handle).map_err(|e| tracing::warn!(%e, "cannot attach job object")).ok()
    });

    tokio::select! {
        _ = shutdown.cancelled() => {
            let _ = child.kill().await;
            return RunOutcome::Shutdown;
        }
        exit = child.wait() => {
            return RunOutcome::FailedToStart(format!("exited during start-up: {exit:?}"));
        }
        ready = wait_healthy(http, &endpoint) => {
            if let Err(reason) = ready {
                let _ = child.kill().await;
                return RunOutcome::FailedToStart(reason);
            }
        }
    }

    warm_up(http, &endpoint).await;
    tracing::info!(url = %endpoint.base_url, gpu_layers, "llama-server ready");
    let _ = status.send(ServerStatus::Ready { endpoint, gpu: gpu_layers > 0 });
    tokio::select! {
        _ = shutdown.cancelled() => {
            let _ = child.kill().await;
            RunOutcome::Shutdown
        }
        exit = child.wait() => RunOutcome::Exited(format!("{exit:?}")),
    }
}

fn spawn_child(config: &ServerConfig, port: u16, api_key: &str, gpu_layers: u32) -> std::io::Result<Child> {
    let mut command = Command::new(&config.executable);
    command
        .args(config.args(port, api_key, gpu_layers))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = config.executable.parent() {
        // Backend DLLs (ggml-vulkan, ggml-cpu-*) are loaded from beside the binary.
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn()?;
    forward_logs(child.stdout.take());
    forward_logs(child.stderr.take());
    Ok(child)
}

fn forward_logs(stream: Option<impl tokio::io::AsyncRead + Unpin + Send + 'static>) {
    let Some(stream) = stream else { return };
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "llama_server", "{line}");
        }
    });
}

async fn wait_healthy(http: &reqwest::Client, endpoint: &Endpoint) -> Result<(), String> {
    let url = format!("{}/health", endpoint.base_url);
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        let response = http.get(&url).bearer_auth(&endpoint.api_key).send().await;
        if response.is_ok_and(|r| r.status().is_success()) {
            return Ok(());
        }
        tokio::time::sleep(HEALTH_INTERVAL).await;
    }
    Err(format!("not healthy after {READY_TIMEOUT:?}"))
}

/// The first generation pays for GPU shader compilation and cache setup
/// (seconds on Vulkan). Paying it before reporting ready keeps the first real
/// translation fast.
async fn warm_up(http: &reqwest::Client, endpoint: &Endpoint) {
    let body = serde_json::json!({
        "messages": [{ "role": "user", "content": "Hi" }],
        "max_tokens": 1,
        "chat_template_kwargs": { "enable_thinking": false },
    });
    let request = http
        .post(format!("{}/v1/chat/completions", endpoint.base_url))
        .bearer_auth(&endpoint.api_key)
        .json(&body)
        .send();
    if let Err(error) = request.await {
        tracing::debug!(%error, "llama-server warm-up failed");
    }
}

fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn random_key() -> String {
    let mut bytes = [0u8; 24];
    // getrandom only fails on exotic platforms without an entropy source.
    getrandom::fill(&mut bytes).expect("OS entropy source unavailable");
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_loopback_only_with_auth() {
        let config = ServerConfig::new("llama-server".into(), "model.gguf".into());
        let args = config.args(1234, "secret", 99);
        let value_of = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
        assert_eq!(value_of("--host").as_deref(), Some("127.0.0.1"));
        assert_eq!(value_of("--api-key").as_deref(), Some("secret"));
        assert_eq!(value_of("--n-gpu-layers").as_deref(), Some("99"));
    }

    #[test]
    fn keys_are_random() {
        assert_ne!(random_key(), random_key());
        assert_eq!(random_key().len(), 48);
    }

    #[tokio::test]
    async fn missing_executable_reports_restarting_then_stops() {
        let server = LlamaServer::spawn(ServerConfig::new("does-not-exist-llama".into(), "m.gguf".into()));
        let mut status = server.subscribe();
        let restarting = status.wait_for(|s| matches!(s, ServerStatus::Restarting { .. })).await.unwrap().clone();
        assert!(matches!(restarting, ServerStatus::Restarting { reason } if reason.contains("cannot launch")));
        server.shutdown();
        status.wait_for(|s| *s == ServerStatus::Stopped).await.unwrap();
    }
}
