use std::path::{Path, PathBuf};

use parlance_protocol::{ModelDescriptor, ModelStatus};

use crate::catalog::{CatalogEntry, catalog};

/// Written next to extracted archives once extraction fully succeeded.
pub(crate) const INSTALLED_MARKER: &str = ".installed";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not determine the local data directory")]
    NoDataDir,
    #[error("model `{0}` is not installed")]
    NotInstalled(String),
}

#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `%LOCALAPPDATA%\parlance\models`, `~/Library/Application Support/parlance/models`
    /// or `~/.local/share/parlance/models`.
    pub fn default_location() -> Result<Self, StoreError> {
        let base = directories::BaseDirs::new().ok_or(StoreError::NoDataDir)?;
        Ok(Self::new(base.data_local_dir().join("parlance").join("models")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// File path for plain models, directory path for extracted archives.
    pub fn path(&self, entry: &CatalogEntry) -> PathBuf {
        match entry.archive {
            Some(_) => self.root.join(entry.id),
            None => self.root.join(entry.file_name),
        }
    }

    pub(crate) fn staging_dir(&self) -> PathBuf {
        self.root.join(".downloads")
    }

    /// Cheap check used for listings; the full digest is verified at download time.
    pub fn is_installed(&self, entry: &CatalogEntry) -> bool {
        let path = self.path(entry);
        match entry.archive {
            Some(_) => path.join(INSTALLED_MARKER).is_file(),
            None => std::fs::metadata(&path).is_ok_and(|m| m.len() == entry.size_bytes),
        }
    }

    pub fn require(&self, entry: &CatalogEntry) -> Result<PathBuf, StoreError> {
        if self.is_installed(entry) { Ok(self.path(entry)) } else { Err(StoreError::NotInstalled(entry.id.into())) }
    }

    /// Locates an executable inside an extracted archive (release archives
    /// nest binaries at varying depths across platforms).
    pub fn find_executable(&self, entry: &CatalogEntry, stem: &str) -> Option<PathBuf> {
        let file = format!("{stem}{}", std::env::consts::EXE_SUFFIX);
        find_file(&self.require(entry).ok()?, &file, 4)
    }

    pub fn descriptors(&self, is_downloading: impl Fn(&str) -> bool) -> Vec<ModelDescriptor> {
        catalog()
            .iter()
            .map(|entry| ModelDescriptor {
                id: entry.id.into(),
                kind: entry.kind,
                name: entry.name.into(),
                description: entry.description.into(),
                license: entry.license.into(),
                size_bytes: entry.size_bytes,
                status: if is_downloading(entry.id) {
                    ModelStatus::Downloading
                } else if self.is_installed(entry) {
                    ModelStatus::Installed
                } else {
                    ModelStatus::NotInstalled
                },
                is_default: entry.is_default,
            })
            .collect()
    }
}

fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.into_iter().find_map(|sub| find_file(&sub, name, depth - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::find;

    #[test]
    fn reports_status_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        let vad = find("silero-vad-5.1.2").unwrap();
        assert!(!store.is_installed(vad));

        std::fs::write(store.path(vad), vec![0u8; 10]).unwrap();
        assert!(!store.is_installed(vad), "size mismatch must not count as installed");

        std::fs::write(store.path(vad), vec![0u8; vad.size_bytes as usize]).unwrap();
        assert!(store.is_installed(vad));
        let descriptors = store.descriptors(|_| false);
        let status = descriptors.iter().find(|d| d.id == vad.id).unwrap().status;
        assert_eq!(status, ModelStatus::Installed);
    }

    #[test]
    fn finds_nested_executables() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        let runtime = find("llama-server").unwrap();
        let nested = store.path(runtime).join("build").join("bin");
        std::fs::create_dir_all(&nested).unwrap();
        let exe = nested.join(format!("llama-server{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&exe, b"").unwrap();
        std::fs::write(store.path(runtime).join(INSTALLED_MARKER), b"").unwrap();
        assert_eq!(store.find_executable(runtime, "llama-server"), Some(exe));
    }
}
