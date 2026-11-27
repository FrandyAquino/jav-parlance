use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectProfile {
    pub id: String,
    pub name: String,
    pub content: String,
    #[ts(type = "number")]
    pub revision: u64,
}

/// Owned context captured at admission, independent of subsequent profile edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSnapshot {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub content: String,
}

impl From<&ProjectProfile> for ProfileSnapshot {
    fn from(profile: &ProjectProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            revision: profile.revision,
            content: profile.content.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileAttribution {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub revision: u64,
}

impl From<&ProfileSnapshot> for ProfileAttribution {
    fn from(profile: &ProfileSnapshot) -> Self {
        Self { id: profile.id.clone(), name: profile.name.clone(), revision: profile.revision }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_survives_source_changes_and_deletion() {
        let mut profile =
            ProjectProfile { id: "a".into(), name: "Original".into(), content: "Redis".into(), revision: 1 };
        let snapshot = ProfileSnapshot::from(&profile);
        profile.content = "Postgres".into();
        profile.revision = 2;
        drop(profile);
        assert_eq!(snapshot.content, "Redis");
        assert_eq!(ProfileAttribution::from(&snapshot).revision, 1);
    }
}
