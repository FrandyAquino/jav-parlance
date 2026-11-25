//! Chromium native messaging contract between the browser extension and the
//! `parlance-native-host` bridge.
//!
//! Framing: a 32-bit length in native byte order followed by UTF-8 JSON.
//! Chrome caps host→browser messages at 1 MiB; Parlance applies the same cap
//! in both directions so a compromised page cannot make the host allocate
//! unbounded buffers.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use ts_rs::TS;

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export, rename = "ExtensionMessage")]
pub enum ExtensionMessage {
    Hello {
        protocol_version: u32,
        tab_title: String,
    },
    /// Mono PCM16 little-endian samples, base64-encoded.
    Audio {
        #[ts(type = "number")]
        sequence: u64,
        sample_rate: u32,
        pcm16_base64: String,
    },
    End,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export, rename = "HostMessage")]
pub enum HostMessage {
    Ready {
        protocol_version: u32,
    },
    /// The desktop app is not running or refused the stream.
    Unavailable {
        reason: String,
    },
    Error {
        message: String,
    },
}

/// First frame the native host sends to the desktop app's bridge socket.
/// The token proves the host read the user-private discovery file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeHello {
    pub token: String,
    pub tab_title: String,
}

/// Written by the desktop app to `<local data dir>/parlance/bridge.json`
/// (readable only by the user) so the native host can find the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeDiscovery {
    pub port: u16,
    pub token: String,
}

/// Sample rate the extension resamples tab audio to before sending.
pub const BRIDGE_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum FramingError {
    #[error("message of {0} bytes exceeds the {MAX_MESSAGE_BYTES} byte limit")]
    TooLarge(usize),
    #[error("malformed message: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Reads one framed message; `Ok(None)` on clean end of stream.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, FramingError> {
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_ne_bytes(header) as usize;
    if len > MAX_MESSAGE_BYTES {
        return Err(FramingError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

pub fn write_message<T: Serialize>(writer: &mut impl Write, message: &T) -> Result<(), FramingError> {
    let body = serde_json::to_vec(message)?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(FramingError::TooLarge(body.len()));
    }
    writer.write_all(&(body.len() as u32).to_ne_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_framed_messages() {
        let mut buffer = Vec::new();
        let message = ExtensionMessage::Hello { protocol_version: 1, tab_title: "Standup".into() };
        write_message(&mut buffer, &message).unwrap();
        write_message(&mut buffer, &ExtensionMessage::End).unwrap();

        let mut reader = buffer.as_slice();
        assert_eq!(read_message::<ExtensionMessage>(&mut reader).unwrap(), Some(message));
        assert_eq!(read_message::<ExtensionMessage>(&mut reader).unwrap(), Some(ExtensionMessage::End));
        assert_eq!(read_message::<ExtensionMessage>(&mut reader).unwrap(), None);
    }

    #[test]
    fn rejects_oversized_frames_before_allocating() {
        let header = ((MAX_MESSAGE_BYTES + 1) as u32).to_ne_bytes();
        let result = read_message::<ExtensionMessage>(&mut header.as_slice());
        assert!(matches!(result, Err(FramingError::TooLarge(_))));
    }

    #[test]
    fn fields_are_camel_case_on_the_wire() {
        let json = serde_json::to_value(ExtensionMessage::Audio {
            sequence: 1,
            sample_rate: BRIDGE_SAMPLE_RATE,
            pcm16_base64: String::new(),
        })
        .unwrap();
        assert_eq!(json["sampleRate"], 16_000);
        assert!(json.get("pcm16Base64").is_some());
    }

    #[test]
    fn rejects_unknown_message_types() {
        let body = br#"{"type":"shell","cmd":"rm"}"#;
        let mut frame = (body.len() as u32).to_ne_bytes().to_vec();
        frame.extend_from_slice(body);
        assert!(matches!(read_message::<ExtensionMessage>(&mut frame.as_slice()), Err(FramingError::Malformed(_))));
    }
}
