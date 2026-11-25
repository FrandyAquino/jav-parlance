use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ModelKind {
    Speech,
    VoiceActivity,
    Language,
    /// Inference runtime binary (llama-server), managed like a model.
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ModelStatus {
    NotInstalled,
    Downloading,
    Installed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelDescriptor {
    pub id: String,
    pub kind: ModelKind,
    pub name: String,
    pub description: String,
    pub license: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub status: ModelStatus,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelProgress {
    pub id: String,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}
