use parlance_domain::{AudioLevel, LanguageCode, SessionState, SourceId, Suggestion, TranscriptSegment, Translation};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartSessionRequest {
    pub source_id: SourceId,
    /// Mixed into the primary source when present (e.g. the user's own voice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub microphone_id: Option<SourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// Non-fatal condition worth surfacing (device lost, audio dropped, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

impl Notice {
    pub fn info(message: impl Into<String>) -> Self {
        Self { level: NoticeLevel::Info, message: message.into() }
    }
    pub fn warning(message: impl Into<String>) -> Self {
        Self { level: NoticeLevel::Warning, message: message.into() }
    }
    pub fn error(message: impl Into<String>) -> Self {
        Self { level: NoticeLevel::Error, message: message.into() }
    }
}

/// Everything the core streams to the UI over a single Tauri channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum SessionEvent {
    StateChanged { state: SessionState },
    Level { level: AudioLevel },
    Transcript { segment: TranscriptSegment },
    Translation { translation: Translation },
    Suggestion { suggestion: Suggestion },
    LanguageDetected { language: LanguageCode },
    Notice { notice: Notice },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_internally_tagged_camel_case() {
        let event = SessionEvent::Level { level: AudioLevel::silent() };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "level");
        assert_eq!(json["level"]["rmsDb"], -100.0);
    }

    #[test]
    fn state_changes_carry_tagged_state() {
        let event = SessionEvent::StateChanged { state: SessionState::Failed { reason: "x".into() } };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["state"]["state"], "failed");
        assert_eq!(json["state"]["reason"], "x");
    }
}
