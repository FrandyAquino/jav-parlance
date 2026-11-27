use parlance_domain::{LanguageCode, ProjectProfile};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectProfiles {
    pub profiles: Vec<ProjectProfile>,
    pub active_profile_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SaveProjectProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AskCopilotRequest {
    pub request_id: String,
    pub question: String,
    pub idea: String,
    pub reply_language: LanguageCode,
    pub user_language: LanguageCode,
    pub profile_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_request_has_camel_case_and_explicit_null_profile() {
        let request = AskCopilotRequest {
            request_id: "manual-1".into(),
            question: "Why?".into(),
            idea: String::new(),
            reply_language: LanguageCode::new("en").unwrap(),
            user_language: LanguageCode::new("es").unwrap(),
            profile_id: None,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["requestId"], "manual-1");
        assert_eq!(json["replyLanguage"], "en");
        assert!(json["profileId"].is_null());
        assert_eq!(serde_json::from_value::<AskCopilotRequest>(json).unwrap(), request);
    }
}

#[cfg(test)]
mod suggestion_contract_tests {
    use crate::SessionEvent;
    use parlance_domain::{
        LanguageCode, ProfileAttribution, Suggestion, SuggestionId, SuggestionOrigin, SuggestionStatus,
    };
    #[test]
    fn manual_and_automatic_events_keep_identity_and_nullable_segment() {
        let mut suggestion = Suggestion {
            id: SuggestionId(1),
            question_segment_id: None,
            request_id: "request".into(),
            origin: SuggestionOrigin::Manual,
            question: "Why?".into(),
            idea: "Use cache".into(),
            profile: Some(ProfileAttribution { id: "p".into(), name: "Project A".into(), revision: 3 }),
            status: SuggestionStatus::Generating,
            error: None,
            conversation_truncated: true,
            speaker_language: Some(LanguageCode::new("en").unwrap()),
            user_language: LanguageCode::new("es").unwrap(),
            reply_in_speaker_language: "Partial".into(),
            reply_in_user_language: String::new(),
            is_complete: false,
        };
        let value = serde_json::to_value(SessionEvent::Suggestion { suggestion: suggestion.clone() }).unwrap();
        assert_eq!(value["type"], "suggestion");
        assert_eq!(value["suggestion"]["origin"], "manual");
        assert!(value["suggestion"]["questionSegmentId"].is_null());
        assert_eq!(value["suggestion"]["profile"]["revision"], 3);
        assert_eq!(value["suggestion"]["conversationTruncated"], true);
        suggestion.origin = SuggestionOrigin::Automatic;
        suggestion.status = SuggestionStatus::Failed;
        suggestion.error = Some("Translation failed".into());
        let value = serde_json::to_value(SessionEvent::Suggestion { suggestion }).unwrap();
        assert_eq!(value["suggestion"]["status"], "failed");
        assert_eq!(value["suggestion"]["requestId"], "request");
        assert_eq!(value["suggestion"]["isComplete"], false);
    }
}
