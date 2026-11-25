use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Capture features actually available on this machine, so the UI can hide
/// options instead of failing at start time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CaptureSupport {
    pub system_output: bool,
    pub application: bool,
    pub microphone: bool,
    pub browser_tab: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    /// `windows`, `macos` or `linux`.
    pub platform: String,
    pub capture_backend: String,
    pub capture: CaptureSupport,
    /// CPU/GPU features reported by the speech runtime.
    pub speech_runtime: String,
    pub models_dir: String,
}
