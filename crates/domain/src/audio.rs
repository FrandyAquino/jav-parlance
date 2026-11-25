use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Opaque, backend-defined identifier of a capturable audio source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, type = "string")]
pub struct SourceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SourceKind {
    /// Everything played on an output endpoint (loopback).
    SystemOutput,
    /// Audio rendered by one process tree.
    Application {
        pid: u32,
    },
    Microphone,
    BrowserTab,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SourceDescriptor {
    pub id: SourceId,
    pub kind: SourceKind,
    pub name: String,
    pub is_default: bool,
}

/// Loudness snapshot of a short audio window, in dBFS.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AudioLevel {
    pub rms_db: f32,
    pub peak_db: f32,
}

impl AudioLevel {
    pub const SILENCE_DB: f32 = -100.0;

    pub fn silent() -> Self {
        Self { rms_db: Self::SILENCE_DB, peak_db: Self::SILENCE_DB }
    }
}
