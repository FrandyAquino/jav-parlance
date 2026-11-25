use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::language::LanguageCode;
use crate::transcript::SegmentId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, type = "number")]
pub struct SuggestionId(pub u64);

/// A suggested reply to a question detected in the conversation, rendered in
/// the speaker's language and in the user's preferred language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: SuggestionId,
    pub question_segment_id: SegmentId,
    pub question: String,
    pub speaker_language: Option<LanguageCode>,
    pub user_language: LanguageCode,
    pub reply_in_speaker_language: String,
    pub reply_in_user_language: String,
    pub is_complete: bool,
}
