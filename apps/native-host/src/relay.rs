use std::io::{Read, Write};

use parlance_protocol::native_messaging::{
    BRIDGE_SAMPLE_RATE, ExtensionMessage, FramingError, HostMessage, PROTOCOL_VERSION, read_message, write_message,
};

use crate::bridge::Bridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayOutcome {
    /// The extension sent `end` or closed stdin.
    ExtensionClosed,
    /// The desktop app was not available; the extension was told why.
    Unavailable,
    /// The desktop socket broke mid-stream.
    BridgeClosed,
}

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("extension channel: {0}")]
    Extension(#[from] FramingError),
    #[error("protocol violation: {0}")]
    Protocol(String),
}

/// Runs one capture session: handshake with the extension, open the bridge,
/// then forward audio until either side closes.
pub fn run_relay<B: Bridge>(
    extension_in: &mut impl Read,
    extension_out: &mut impl Write,
    bridge: &B,
) -> Result<RelayOutcome, RelayError> {
    let result = relay(extension_in, extension_out, bridge);
    if let Err(error) = &result {
        // Best effort: the channel itself may be what failed.
        let _ = write_message(extension_out, &HostMessage::Error { message: error.to_string() });
    }
    result
}

fn relay<B: Bridge>(
    extension_in: &mut impl Read,
    extension_out: &mut impl Write,
    bridge: &B,
) -> Result<RelayOutcome, RelayError> {
    let Some(tab_title) = read_hello(extension_in)? else {
        return Ok(RelayOutcome::ExtensionClosed);
    };
    let mut stream = match bridge.open(&tab_title) {
        Ok(stream) => stream,
        Err(unavailable) => {
            write_message(extension_out, &HostMessage::Unavailable { reason: unavailable.to_string() })?;
            return Ok(RelayOutcome::Unavailable);
        }
    };
    write_message(extension_out, &HostMessage::Ready { protocol_version: PROTOCOL_VERSION })?;
    let outcome = forward(extension_in, &mut stream)?;
    if outcome == RelayOutcome::BridgeClosed {
        let reason = "Parlance stopped receiving audio. Check the app, then start capture again.".to_owned();
        write_message(extension_out, &HostMessage::Unavailable { reason })?;
    }
    Ok(outcome)
}

fn read_hello(extension_in: &mut impl Read) -> Result<Option<String>, RelayError> {
    match read_message(extension_in)? {
        None => Ok(None),
        Some(ExtensionMessage::Hello { protocol_version, tab_title }) if protocol_version == PROTOCOL_VERSION => {
            Ok(Some(tab_title))
        }
        Some(ExtensionMessage::Hello { protocol_version, .. }) => Err(RelayError::Protocol(format!(
            "extension speaks protocol {protocol_version}, host expects {PROTOCOL_VERSION}; update both to the same release"
        ))),
        Some(_) => Err(RelayError::Protocol("expected hello as the first message".to_owned())),
    }
}

fn forward(extension_in: &mut impl Read, bridge_out: &mut impl Write) -> Result<RelayOutcome, RelayError> {
    while let Some(message) = read_message::<ExtensionMessage>(extension_in)? {
        validate_stream_message(&message)?;
        let is_end = message == ExtensionMessage::End;
        if write_message(bridge_out, &message).is_err() {
            return Ok(RelayOutcome::BridgeClosed);
        }
        if is_end {
            break;
        }
    }
    Ok(RelayOutcome::ExtensionClosed)
}

fn validate_stream_message(message: &ExtensionMessage) -> Result<(), RelayError> {
    match message {
        ExtensionMessage::Audio { sample_rate, .. } if *sample_rate != BRIDGE_SAMPLE_RATE => {
            Err(RelayError::Protocol(format!("audio must be {BRIDGE_SAMPLE_RATE} Hz, got {sample_rate} Hz")))
        }
        ExtensionMessage::Hello { .. } => Err(RelayError::Protocol("unexpected second hello".to_owned())),
        _ => Ok(()),
    }
}
