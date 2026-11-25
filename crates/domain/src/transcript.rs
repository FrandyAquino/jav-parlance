use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::language::LanguageCode;

/// Stable identifier of a transcript segment. A segment keeps its id across
/// revisions; consumers replace the previous revision in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export, type = "number")]
pub struct SegmentId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum Stability {
    /// Hypothesis that may still change.
    Partial,
    /// Committed text; it will never be revised again.
    Final,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub id: SegmentId,
    pub revision: u32,
    /// Text already agreed upon by consecutive hypotheses.
    pub stable_text: String,
    /// Tail that may still change; empty once the segment is final.
    pub unstable_text: String,
    pub language: Option<LanguageCode>,
    #[ts(type = "number")]
    pub start_ms: u64,
    #[ts(type = "number")]
    pub end_ms: u64,
    pub stability: Stability,
}

impl TranscriptSegment {
    pub fn text(&self) -> String {
        match (self.stable_text.is_empty(), self.unstable_text.is_empty()) {
            (_, true) => self.stable_text.clone(),
            (true, false) => self.unstable_text.clone(),
            (false, false) => format!("{} {}", self.stable_text, self.unstable_text),
        }
    }

    pub fn is_final(&self) -> bool {
        self.stability == Stability::Final
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub segment_id: SegmentId,
    /// Revision of the source segment this translation was produced from.
    pub source_revision: u32,
    pub target_language: LanguageCode,
    pub text: String,
    pub is_final: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(stable: &str, unstable: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: SegmentId(1),
            revision: 0,
            stable_text: stable.into(),
            unstable_text: unstable.into(),
            language: None,
            start_ms: 0,
            end_ms: 0,
            stability: Stability::Partial,
        }
    }

    #[test]
    fn joins_stable_and_unstable_text() {
        assert_eq!(segment("hello", "world").text(), "hello world");
        assert_eq!(segment("", "world").text(), "world");
        assert_eq!(segment("hello", "").text(), "hello");
    }
}
