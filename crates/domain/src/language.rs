use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

/// ISO 639-1 (or 639-3 when no two-letter code exists) language code, lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash, TS)]
#[ts(export, type = "string")]
pub struct LanguageCode(String);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LanguageError {
    #[error("language code `{0}` must be 2 or 3 ASCII letters")]
    Invalid(String),
}

impl LanguageCode {
    pub fn new(code: impl AsRef<str>) -> Result<Self, LanguageError> {
        let code = code.as_ref().trim().to_ascii_lowercase();
        let valid = (2..=3).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase());
        if valid { Ok(Self(code)) } else { Err(LanguageError::Invalid(code)) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// English name for common meeting languages; falls back to the code.
    pub fn display_name(&self) -> &str {
        match self.0.as_str() {
            "en" => "English",
            "es" => "Spanish",
            "fr" => "French",
            "de" => "German",
            "it" => "Italian",
            "pt" => "Portuguese",
            "nl" => "Dutch",
            "ru" => "Russian",
            "zh" => "Chinese",
            "ja" => "Japanese",
            "ko" => "Korean",
            "ar" => "Arabic",
            "hi" => "Hindi",
            "tr" => "Turkish",
            "pl" => "Polish",
            "uk" => "Ukrainian",
            other => other,
        }
    }
}

impl FromStr for LanguageCode {
    type Err = LanguageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for LanguageCode {
    type Error = LanguageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<LanguageCode> for String {
    fn from(value: LanguageCode) -> Self {
        value.0
    }
}

impl Serialize for LanguageCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for LanguageCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_whitespace() {
        assert_eq!(LanguageCode::new(" ES ").unwrap().as_str(), "es");
    }

    #[test]
    fn rejects_invalid_codes() {
        assert!(LanguageCode::new("e").is_err());
        assert!(LanguageCode::new("english").is_err());
        assert!(LanguageCode::new("e5").is_err());
    }

    #[test]
    fn deserialization_validates() {
        assert!(serde_json::from_str::<LanguageCode>("\"xx1\"").is_err());
        let fr: LanguageCode = serde_json::from_str("\"fr\"").unwrap();
        assert_eq!(fr.display_name(), "French");
    }
}
