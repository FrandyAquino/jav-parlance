//! Receives browser-tab audio from `parlance-native-host` (ADR 0010).
//!
//! A loopback TCP listener on an ephemeral port accepts connections that
//! authenticate with a random token published in a user-private discovery
//! file. Each authenticated connection becomes a `tab:<n>` source; its PCM16
//! frames flow into a session's [`AudioSink`] while one is attached.

use std::collections::HashMap;
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use base64::Engine;
use parlance_audio::StreamFormat;
use parlance_audio::convert::i16_to_f32;
use parlance_audio::ring::AudioSink;
use parlance_domain::{SourceDescriptor, SourceId, SourceKind};
use parlance_protocol::native_messaging::{
    BRIDGE_SAMPLE_RATE, BridgeDiscovery, BridgeHello, ExtensionMessage, read_message,
};

use crate::{CaptureBackend, CaptureCapabilities, CaptureError, CaptureEventHandler, CaptureStream};

const PREFIX: &str = "tab:";
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const FORMAT: StreamFormat = StreamFormat { sample_rate: BRIDGE_SAMPLE_RATE, channels: 1 };

type SinkSlot = Arc<Mutex<Option<AudioSink>>>;

struct Tab {
    title: String,
    sink: SinkSlot,
}

#[derive(Default)]
struct Registry {
    tabs: Mutex<HashMap<u64, Tab>>,
    next_id: AtomicU64,
}

impl Registry {
    fn tabs(&self) -> MutexGuard<'_, HashMap<u64, Tab>> {
        self.tabs.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub struct BridgeBackend {
    registry: Arc<Registry>,
    discovery: PathBuf,
}

impl std::fmt::Debug for BridgeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeBackend").field("discovery", &self.discovery).finish()
    }
}

impl BridgeBackend {
    /// Binds the listener and publishes `discovery` (port + token).
    pub fn start(discovery: PathBuf) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let token = random_token()?;
        write_discovery(&discovery, &BridgeDiscovery { port: listener.local_addr()?.port(), token: token.clone() })?;

        let registry = Arc::new(Registry::default());
        let accept_registry = Arc::clone(&registry);
        std::thread::Builder::new().name("parlance-bridge".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let (registry, token) = (Arc::clone(&accept_registry), token.clone());
                let _ = std::thread::Builder::new()
                    .name("parlance-bridge-conn".into())
                    .spawn(move || serve(stream, &registry, &token));
            }
        })?;
        Ok(Self { registry, discovery })
    }
}

impl Drop for BridgeBackend {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.discovery);
    }
}

fn random_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn write_discovery(path: &Path, discovery: &BridgeDiscovery) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec(discovery)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(path)?, &json)
}

/// Compares without early exit so the token cannot be guessed byte by byte.
fn tokens_match(expected: &str, received: &str) -> bool {
    expected.len() == received.len()
        && expected.bytes().zip(received.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn serve(stream: TcpStream, registry: &Registry, token: &str) {
    let _ = stream.set_read_timeout(Some(HELLO_TIMEOUT));
    let mut reader = BufReader::new(stream);
    let hello: BridgeHello = match read_message(&mut reader) {
        Ok(Some(hello)) => hello,
        _ => return,
    };
    if !tokens_match(token, &hello.token) {
        tracing::warn!("rejected bridge connection with an invalid token");
        return;
    }
    let _ = reader.get_ref().set_read_timeout(None);

    let id = registry.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let sink: SinkSlot = Arc::default();
    let title = if hello.tab_title.trim().is_empty() { "Browser tab".to_owned() } else { hello.tab_title };
    registry.tabs().insert(id, Tab { title, sink: Arc::clone(&sink) });
    tracing::info!(id, "browser tab connected");

    relay(&mut reader, &sink);

    registry.tabs().remove(&id);
    // Dropping the sink signals end-of-stream to a running session.
    sink.lock().unwrap_or_else(|p| p.into_inner()).take();
}

fn relay(reader: &mut BufReader<TcpStream>, sink: &SinkSlot) {
    let mut pcm = Vec::new();
    let mut samples = Vec::new();
    while let Ok(Some(message)) = read_message::<ExtensionMessage>(reader) {
        match message {
            ExtensionMessage::Audio { sample_rate, pcm16_base64, .. } if sample_rate == BRIDGE_SAMPLE_RATE => {
                pcm.clear();
                if base64::engine::general_purpose::STANDARD.decode_vec(&pcm16_base64, &mut pcm).is_err() {
                    continue;
                }
                let frames: Vec<i16> = pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
                samples.clear();
                i16_to_f32(&frames, &mut samples);
                if let Some(sink) = sink.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
                    sink.push(&samples);
                }
            }
            ExtensionMessage::End => return,
            _ => {}
        }
    }
}

impl CaptureBackend for BridgeBackend {
    fn name(&self) -> &'static str {
        "browser-bridge"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities { browser_tab: true, ..CaptureCapabilities::default() }
    }

    fn handles(&self, source: &SourceId) -> bool {
        source.0.starts_with(PREFIX)
    }

    fn list_sources(&self) -> Result<Vec<SourceDescriptor>, CaptureError> {
        let mut sources: Vec<_> = self
            .registry
            .tabs()
            .iter()
            .map(|(id, tab)| SourceDescriptor {
                id: SourceId(format!("{PREFIX}{id}")),
                kind: SourceKind::BrowserTab,
                name: tab.title.clone(),
                is_default: false,
            })
            .collect();
        sources.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        Ok(sources)
    }

    fn start(
        &self,
        source: &SourceId,
        sink: AudioSink,
        _on_event: CaptureEventHandler,
    ) -> Result<Box<dyn CaptureStream>, CaptureError> {
        let not_found = || CaptureError::SourceNotFound(source.0.clone());
        let id: u64 = source.0.strip_prefix(PREFIX).and_then(|id| id.parse().ok()).ok_or_else(not_found)?;
        let slot = self.registry.tabs().get(&id).map(|tab| Arc::clone(&tab.sink)).ok_or_else(not_found)?;
        *slot.lock().unwrap_or_else(|p| p.into_inner()) = Some(sink);
        Ok(Box::new(BridgeStream { slot }))
    }
}

struct BridgeStream {
    slot: SinkSlot,
}

impl std::fmt::Debug for BridgeStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeStream").finish()
    }
}

impl CaptureStream for BridgeStream {
    fn format(&self) -> StreamFormat {
        FORMAT
    }
}

impl Drop for BridgeStream {
    fn drop(&mut self) {
        self.slot.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use parlance_audio::ring;
    use parlance_protocol::native_messaging::write_message;

    use super::*;

    fn backend() -> (BridgeBackend, BridgeDiscovery, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.json");
        let backend = BridgeBackend::start(path.clone()).unwrap();
        let discovery = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        (backend, discovery, dir)
    }

    fn connect(discovery: &BridgeDiscovery, token: &str) -> TcpStream {
        let mut stream = TcpStream::connect(("127.0.0.1", discovery.port)).unwrap();
        write_message(&mut stream, &BridgeHello { token: token.into(), tab_title: "Standup".into() }).unwrap();
        stream
    }

    fn wait_for_sources(backend: &BridgeBackend, count: usize) -> Vec<SourceDescriptor> {
        for _ in 0..200 {
            let sources = backend.list_sources().unwrap();
            if sources.len() == count {
                return sources;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("expected {count} sources");
    }

    #[test]
    fn authenticated_tab_streams_audio_into_attached_sink() {
        let (backend, discovery, _dir) = backend();
        let mut stream = connect(&discovery, &discovery.token);
        let sources = wait_for_sources(&backend, 1);
        assert_eq!(sources[0].name, "Standup");

        let (sink, mut source) = ring::channel(1024);
        let _capture = backend.start(&sources[0].id, sink, Arc::new(|_| {})).unwrap();
        let pcm: Vec<u8> = [16_384i16, -16_384].iter().flat_map(|s| s.to_le_bytes()).collect();
        let audio = ExtensionMessage::Audio {
            sequence: 1,
            sample_rate: BRIDGE_SAMPLE_RATE,
            pcm16_base64: base64::engine::general_purpose::STANDARD.encode(pcm),
        };
        write_message(&mut stream, &audio).unwrap();
        stream.flush().unwrap();

        let mut received = Vec::new();
        for _ in 0..200 {
            source.drain_into(&mut received);
            if received.len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(received, [0.5, -0.5]);

        write_message(&mut stream, &ExtensionMessage::End).unwrap();
        wait_for_sources(&backend, 0);
    }

    #[test]
    fn rejects_wrong_token() {
        let (backend, discovery, _dir) = backend();
        let _stream = connect(&discovery, &"0".repeat(discovery.token.len()));
        std::thread::sleep(Duration::from_millis(100));
        assert!(backend.list_sources().unwrap().is_empty());
    }

    #[test]
    fn removes_discovery_file_on_drop() {
        let (backend, _discovery, dir) = backend();
        drop(backend);
        assert!(!dir.path().join("bridge.json").exists());
    }

    #[test]
    fn token_comparison_requires_exact_match() {
        assert!(tokens_match("abc", "abc"));
        assert!(!tokens_match("abc", "abd"));
        assert!(!tokens_match("abc", "abcd"));
    }
}
