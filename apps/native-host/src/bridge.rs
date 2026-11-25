use std::fs;
use std::io::{self, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use directories::BaseDirs;
use parlance_protocol::native_messaging::{BridgeDiscovery, BridgeHello, write_message};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const DISCOVERY_FILE: &str = "bridge.json";
const APP_DIR: &str = "parlance";

/// Why the desktop app cannot accept a stream; the message is shown to the user as-is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BridgeUnavailable {
    #[error("Open Parlance on this computer, then start capture again.")]
    NotRunning,
    #[error("Parlance's bridge file is unreadable. Restart Parlance and start capture again.")]
    BadDiscovery,
    #[error("Could not reach Parlance. Open Parlance on this computer, then start capture again.")]
    Unreachable,
}

/// Opens the authenticated stream to the desktop app.
pub trait Bridge {
    type Stream: Write;

    fn open(&self, tab_title: &str) -> Result<Self::Stream, BridgeUnavailable>;
}

/// Connects to the loopback port advertised in the discovery file.
#[derive(Debug, Clone)]
pub struct TcpBridge {
    discovery_path: PathBuf,
}

impl TcpBridge {
    pub fn new(discovery_path: impl Into<PathBuf>) -> Self {
        Self { discovery_path: discovery_path.into() }
    }

    /// `<local data dir>/parlance/bridge.json`, the location the desktop app writes to.
    pub fn default_discovery_path() -> Option<PathBuf> {
        BaseDirs::new().map(|dirs| dirs.data_local_dir().join(APP_DIR).join(DISCOVERY_FILE))
    }
}

impl Bridge for TcpBridge {
    type Stream = TcpStream;

    fn open(&self, tab_title: &str) -> Result<TcpStream, BridgeUnavailable> {
        let discovery = read_discovery(&self.discovery_path)?;
        let mut stream = connect(discovery.port).map_err(|_| BridgeUnavailable::Unreachable)?;
        let hello = BridgeHello { token: discovery.token, tab_title: tab_title.to_owned() };
        write_message(&mut stream, &hello).map_err(|_| BridgeUnavailable::Unreachable)?;
        Ok(stream)
    }
}

fn read_discovery(path: &Path) -> Result<BridgeDiscovery, BridgeUnavailable> {
    let bytes = fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => BridgeUnavailable::NotRunning,
        _ => BridgeUnavailable::BadDiscovery,
    })?;
    serde_json::from_slice(&bytes).map_err(|_| BridgeUnavailable::BadDiscovery)
}

fn connect(port: u16) -> io::Result<TcpStream> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
    // Audio frames are small and latency-sensitive.
    stream.set_nodelay(true)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_discovery_file_means_not_running() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bridge = TcpBridge::new(dir.path().join(DISCOVERY_FILE));
        assert_eq!(bridge.open("Tab").err(), Some(BridgeUnavailable::NotRunning));
    }

    #[test]
    fn malformed_discovery_file_is_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(DISCOVERY_FILE);
        fs::write(&path, b"not json").expect("write");
        assert_eq!(TcpBridge::new(path).open("Tab").err(), Some(BridgeUnavailable::BadDiscovery));
    }

    #[test]
    fn closed_port_is_unreachable() {
        let port = {
            let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
            listener.local_addr().expect("addr").port()
        };
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(DISCOVERY_FILE);
        let discovery = BridgeDiscovery { port, token: "t".into() };
        fs::write(&path, serde_json::to_vec(&discovery).expect("json")).expect("write");
        assert_eq!(TcpBridge::new(path).open("Tab").err(), Some(BridgeUnavailable::Unreachable));
    }
}
