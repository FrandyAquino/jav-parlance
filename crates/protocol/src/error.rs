use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stable, machine-readable error categories the UI can map to actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorCode {
    InvalidRequest,
    InvalidState,
    ModelMissing,
    ModelCorrupt,
    CaptureUnavailable,
    PermissionDenied,
    InferenceFailed,
    Storage,
    Network,
    Internal,
}

/// Error returned by every Tauri command: a code plus a human-readable,
/// actionable message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[error("{message}")]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
}

impl IpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}
