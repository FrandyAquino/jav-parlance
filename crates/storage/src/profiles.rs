//! Project context is persisted independently of opt-in conversation history.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use parlance_domain::{ProfileSnapshot, ProjectProfile};
use parlance_protocol::{ProjectProfiles, SaveProjectProfileRequest};
use serde::{Deserialize, Serialize};

/// Version of the complete profile document, including its active selection.
const FORMAT_VERSION: u32 = 1;
/// Names remain readable in selectors; the limit counts Unicode characters.
const MAX_NAME_CHARS: usize = 80;
/// Stored context can exceed an individual model request's stricter budget.
const MAX_CONTENT_BYTES: usize = 12_000;

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("The selected project profile no longer exists")]
    NotFound,
    #[error(
        "Project profiles could not be read. The original file was preserved; repair profiles.json and restart Parlance"
    )]
    InvalidFile,
    #[error("Project profiles could not be accessed: {0}")]
    Io(#[from] std::io::Error),
}

/// A memory-backed profile port; snapshots never perform filesystem I/O.
pub trait ProfileRepository: Send + Sync {
    fn list(&self) -> Result<ProjectProfiles, ProfileError>;
    fn save(&self, request: SaveProjectProfileRequest) -> Result<ProjectProfiles, ProfileError>;
    fn delete(&self, id: &str) -> Result<ProjectProfiles, ProfileError>;
    fn set_active(&self, id: Option<&str>) -> Result<ProjectProfiles, ProfileError>;
    fn snapshot(&self, id: Option<&str>) -> Result<Option<ProfileSnapshot>, ProfileError>;
    fn active_snapshot(&self) -> Option<ProfileSnapshot>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    version: u32,
    next_id: u64,
    profiles: Vec<ProjectProfile>,
    active_profile_id: Option<String>,
}

impl Default for Document {
    fn default() -> Self {
        Self { version: FORMAT_VERSION, next_id: 1, profiles: Vec::new(), active_profile_id: None }
    }
}

impl Document {
    fn view(&self) -> ProjectProfiles {
        ProjectProfiles { profiles: self.profiles.clone(), active_profile_id: self.active_profile_id.clone() }
    }

    fn validate(&self) -> Result<(), ProfileError> {
        let mut ids = std::collections::HashSet::new();
        if self.version != FORMAT_VERSION || self.next_id == 0 {
            return Err(ProfileError::InvalidFile);
        }
        for profile in &self.profiles {
            if profile.id.is_empty()
                || profile.revision == 0
                || !ids.insert(&profile.id)
                || validate_text(&profile.name, &profile.content).is_err()
            {
                return Err(ProfileError::InvalidFile);
            }
        }
        if self.active_profile_id.as_ref().is_some_and(|id| !ids.contains(id)) {
            return Err(ProfileError::InvalidFile);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct JsonProfileRepository {
    path: PathBuf,
    current: Mutex<Document>,
    mutation: Mutex<()>,
}

impl JsonProfileRepository {
    pub fn open(path: PathBuf) -> Result<Self, ProfileError> {
        let document = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Document>(&bytes).map_err(|_| ProfileError::InvalidFile)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Document::default(),
            Err(error) => return Err(error.into()),
        };
        document.validate()?;
        Ok(Self { path, current: Mutex::new(document), mutation: Mutex::new(()) })
    }

    fn mutate(
        &self,
        change: impl FnOnce(&mut Document) -> Result<(), ProfileError>,
    ) -> Result<ProjectProfiles, ProfileError> {
        let _mutation = self.mutation.lock().unwrap_or_else(|p| p.into_inner());
        let mut next = self.current.lock().unwrap_or_else(|p| p.into_inner()).clone();
        change(&mut next)?;
        let parent = self.path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut temp, &next).map_err(std::io::Error::other)?;
        temp.flush()?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path).map_err(|error| error.error)?;
        let view = next.view();
        *self.current.lock().unwrap_or_else(|p| p.into_inner()) = next;
        Ok(view)
    }
}

impl ProfileRepository for JsonProfileRepository {
    fn list(&self) -> Result<ProjectProfiles, ProfileError> {
        Ok(self.current.lock().unwrap_or_else(|p| p.into_inner()).view())
    }

    fn save(&self, request: SaveProjectProfileRequest) -> Result<ProjectProfiles, ProfileError> {
        let name = request.name.trim().to_owned();
        validate_text(&name, &request.content)?;
        self.mutate(move |document| {
            if let Some(id) = request.id {
                let profile = document.profiles.iter_mut().find(|p| p.id == id).ok_or(ProfileError::NotFound)?;
                profile.revision = profile.revision.checked_add(1).ok_or(ProfileError::InvalidFile)?;
                profile.name = name;
                profile.content = request.content;
            } else {
                let id = loop {
                    let id = format!("profile-{}", document.next_id);
                    document.next_id = document.next_id.checked_add(1).ok_or(ProfileError::InvalidFile)?;
                    if !document.profiles.iter().any(|p| p.id == id) {
                        break id;
                    }
                };
                document.profiles.push(ProjectProfile { id, name, content: request.content, revision: 1 });
            }
            Ok(())
        })
    }

    fn delete(&self, id: &str) -> Result<ProjectProfiles, ProfileError> {
        self.mutate(|document| {
            let index = document.profiles.iter().position(|p| p.id == id).ok_or(ProfileError::NotFound)?;
            document.profiles.remove(index);
            if document.active_profile_id.as_deref() == Some(id) {
                document.active_profile_id = None;
            }
            Ok(())
        })
    }

    fn set_active(&self, id: Option<&str>) -> Result<ProjectProfiles, ProfileError> {
        self.mutate(|document| {
            if id.is_some_and(|id| !document.profiles.iter().any(|p| p.id == id)) {
                return Err(ProfileError::NotFound);
            }
            document.active_profile_id = id.map(str::to_owned);
            Ok(())
        })
    }

    fn snapshot(&self, id: Option<&str>) -> Result<Option<ProfileSnapshot>, ProfileError> {
        let current = self.current.lock().unwrap_or_else(|p| p.into_inner());
        id.map(|id| {
            current.profiles.iter().find(|p| p.id == id).map(ProfileSnapshot::from).ok_or(ProfileError::NotFound)
        })
        .transpose()
    }

    fn active_snapshot(&self) -> Option<ProfileSnapshot> {
        let current = self.current.lock().unwrap_or_else(|p| p.into_inner());
        let id = current.active_profile_id.as_ref()?;
        current.profiles.iter().find(|p| &p.id == id).map(ProfileSnapshot::from)
    }
}

fn validate_text(name: &str, content: &str) -> Result<(), ProfileError> {
    if name.trim().is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(ProfileError::InvalidRequest("Give the profile a name of 1–80 characters".into()));
    }
    if content.len() > MAX_CONTENT_BYTES {
        return Err(ProfileError::InvalidRequest("Project context must be no more than 12000 UTF-8 bytes".into()));
    }
    Ok(())
}
