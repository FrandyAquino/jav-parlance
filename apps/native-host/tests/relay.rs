use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use parlance_native_host::{RelayError, RelayOutcome, TcpBridge, run_relay};
use parlance_protocol::native_messaging::{
    BRIDGE_SAMPLE_RATE, BridgeDiscovery, BridgeHello, ExtensionMessage, FramingError, HostMessage, MAX_MESSAGE_BYTES,
    PROTOCOL_VERSION, read_message, write_message,
};
use serde_json::Value;
use tempfile::TempDir;

const TOKEN: &str = "secret-token";

fn frames(messages: &[ExtensionMessage]) -> Vec<u8> {
    let mut buffer = Vec::new();
    for message in messages {
        write_message(&mut buffer, message).expect("frame");
    }
    buffer
}

fn host_replies(mut output: &[u8]) -> Vec<HostMessage> {
    std::iter::from_fn(|| read_message(&mut output).expect("host frame")).collect()
}

fn hello(protocol_version: u32) -> ExtensionMessage {
    ExtensionMessage::Hello { protocol_version, tab_title: "Weekly sync".into() }
}

fn audio(sequence: u64) -> ExtensionMessage {
    ExtensionMessage::Audio { sequence, sample_rate: BRIDGE_SAMPLE_RATE, pcm16_base64: "AAABAA==".into() }
}

/// Fake desktop bridge: accepts one connection and records every frame until EOF.
struct FakeDesktop {
    _dir: TempDir,
    discovery_path: PathBuf,
    received: JoinHandle<Vec<Value>>,
}

impl FakeDesktop {
    fn start() -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let dir = tempfile::tempdir().expect("tempdir");
        let discovery_path = dir.path().join("bridge.json");
        let discovery = BridgeDiscovery { port, token: TOKEN.into() };
        fs::write(&discovery_path, serde_json::to_vec(&discovery).expect("json")).expect("write discovery");
        let received = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            std::iter::from_fn(|| read_message::<Value>(&mut socket).expect("bridge frame")).collect()
        });
        Self { _dir: dir, discovery_path, received }
    }

    fn bridge(&self) -> TcpBridge {
        TcpBridge::new(&self.discovery_path)
    }
}

#[test]
fn forwards_hello_audio_and_end_to_the_desktop() {
    let desktop = FakeDesktop::start();
    let input = frames(&[hello(PROTOCOL_VERSION), audio(0), audio(1), ExtensionMessage::End]);
    let mut output = Vec::new();

    let outcome = run_relay(&mut input.as_slice(), &mut output, &desktop.bridge()).expect("relay");

    assert_eq!(outcome, RelayOutcome::ExtensionClosed);
    assert_eq!(host_replies(&output), vec![HostMessage::Ready { protocol_version: PROTOCOL_VERSION }]);
    let received = desktop.received.join().expect("desktop thread");
    let expected_hello = BridgeHello { token: TOKEN.into(), tab_title: "Weekly sync".into() };
    let expected: Vec<Value> = [serde_json::to_value(expected_hello).expect("json")]
        .into_iter()
        .chain([audio(0), audio(1), ExtensionMessage::End].iter().map(|m| serde_json::to_value(m).expect("json")))
        .collect();
    assert_eq!(received, expected);
}

#[test]
fn stops_forwarding_when_stdin_closes_without_end() {
    let desktop = FakeDesktop::start();
    let input = frames(&[hello(PROTOCOL_VERSION), audio(7)]);
    let mut output = Vec::new();

    let outcome = run_relay(&mut input.as_slice(), &mut output, &desktop.bridge()).expect("relay");

    assert_eq!(outcome, RelayOutcome::ExtensionClosed);
    assert_eq!(desktop.received.join().expect("desktop thread").len(), 2);
}

#[test]
fn missing_discovery_file_replies_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = TcpBridge::new(dir.path().join("bridge.json"));
    let input = frames(&[hello(PROTOCOL_VERSION), audio(0)]);
    let mut output = Vec::new();

    let outcome = run_relay(&mut input.as_slice(), &mut output, &bridge).expect("relay");

    assert_eq!(outcome, RelayOutcome::Unavailable);
    match host_replies(&output).as_slice() {
        [HostMessage::Unavailable { reason }] => {
            assert!(reason.starts_with("Open Parlance on this computer"))
        }
        other => panic!("unexpected replies: {other:?}"),
    }
}

#[test]
fn rejects_mismatched_protocol_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = TcpBridge::new(dir.path().join("bridge.json"));
    let input = frames(&[hello(PROTOCOL_VERSION + 1)]);
    let mut output = Vec::new();

    let result = run_relay(&mut input.as_slice(), &mut output, &bridge);

    assert!(matches!(result, Err(RelayError::Protocol(_))));
    assert!(matches!(host_replies(&output).as_slice(), [HostMessage::Error { .. }]));
}

#[test]
fn rejects_audio_before_hello() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = TcpBridge::new(dir.path().join("bridge.json"));
    let input = frames(&[audio(0)]);

    let result = run_relay(&mut input.as_slice(), &mut Vec::new(), &bridge);

    assert!(matches!(result, Err(RelayError::Protocol(_))));
}

#[test]
fn rejects_audio_at_the_wrong_sample_rate() {
    let desktop = FakeDesktop::start();
    let wrong_rate = ExtensionMessage::Audio { sequence: 0, sample_rate: 48_000, pcm16_base64: String::new() };
    let input = frames(&[hello(PROTOCOL_VERSION), wrong_rate]);
    let mut output = Vec::new();

    let result = run_relay(&mut input.as_slice(), &mut output, &desktop.bridge());

    assert!(matches!(result, Err(RelayError::Protocol(_))));
    assert_eq!(desktop.received.join().expect("desktop thread").len(), 1, "only the bridge hello is sent");
}

#[test]
fn rejects_oversized_frames_without_forwarding() {
    let desktop = FakeDesktop::start();
    let mut input = frames(&[hello(PROTOCOL_VERSION)]);
    input.extend_from_slice(&((MAX_MESSAGE_BYTES + 1) as u32).to_ne_bytes());
    let mut output = Vec::new();

    let result = run_relay(&mut input.as_slice(), &mut output, &desktop.bridge());

    assert!(matches!(result, Err(RelayError::Extension(FramingError::TooLarge(_)))));
    let replies = host_replies(&output);
    assert!(matches!(replies.as_slice(), [HostMessage::Ready { .. }, HostMessage::Error { .. }]));
    assert_eq!(desktop.received.join().expect("desktop thread").len(), 1);
}

#[test]
fn empty_stdin_exits_cleanly_without_connecting() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = TcpBridge::new(dir.path().join("bridge.json"));
    let mut output = Vec::new();

    let outcome = run_relay(&mut [].as_slice(), &mut output, &bridge).expect("relay");

    assert_eq!(outcome, RelayOutcome::ExtensionClosed);
    assert!(output.is_empty());
}

struct BrokenPipe;

impl std::io::Write for BrokenPipe {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct BrokenBridge;

impl parlance_native_host::Bridge for BrokenBridge {
    type Stream = BrokenPipe;

    fn open(&self, _: &str) -> Result<BrokenPipe, parlance_native_host::BridgeUnavailable> {
        Ok(BrokenPipe)
    }
}

#[test]
fn tells_the_extension_when_the_desktop_goes_away() {
    let input = frames(&[hello(PROTOCOL_VERSION), audio(0), audio(1)]);
    let mut output = Vec::new();

    let outcome = run_relay(&mut input.as_slice(), &mut output, &BrokenBridge).expect("relay");

    assert_eq!(outcome, RelayOutcome::BridgeClosed);
    assert!(matches!(host_replies(&output).as_slice(), [HostMessage::Ready { .. }, HostMessage::Unavailable { .. }]));
}
