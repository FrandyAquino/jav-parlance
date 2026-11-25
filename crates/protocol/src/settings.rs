use parlance_domain::LanguageCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// User preferences persisted by the desktop app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AppSettings {
    /// Spoken language; `None` auto-detects per utterance.
    pub source_language: Option<LanguageCode>,
    /// Language the user reads translations and suggestions in.
    pub target_language: LanguageCode,
    pub speech_model: String,
    pub language_model: String,
    pub translation_enabled: bool,
    pub copilot_enabled: bool,
    /// Meeting history is opt-in; nothing is written to disk otherwise.
    pub history_enabled: bool,
    pub theme: Theme,
    pub always_on_top: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            source_language: None,
            target_language: LanguageCode::new("es").expect("valid literal"),
            speech_model: "whisper-base-q8".into(),
            language_model: "qwen3.5-2b-q4".into(),
            translation_enabled: true,
            copilot_enabled: true,
            history_enabled: false,
            theme: Theme::System,
            always_on_top: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let settings: AppSettings = serde_json::from_str(r#"{"targetLanguage":"fr"}"#).unwrap();
        assert_eq!(settings.target_language.as_str(), "fr");
        assert!(!settings.history_enabled);
        assert_eq!(settings.speech_model, AppSettings::default().speech_model);
    }
}
