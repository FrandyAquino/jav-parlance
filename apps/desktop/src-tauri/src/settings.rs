use std::path::PathBuf;
use std::sync::RwLock;

use parlance_protocol::AppSettings;

/// JSON-backed settings with atomic writes (temp file + rename), so a crash
/// mid-save never corrupts the user's preferences.
pub struct SettingsRepository {
    path: PathBuf,
    current: RwLock<AppSettings>,
}

impl SettingsRepository {
    pub fn load(path: PathBuf) -> Self {
        let current = std::fs::read(&path)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice(&bytes).map_err(|e| tracing::warn!(%e, "invalid settings; using defaults")).ok()
            })
            .unwrap_or_default();
        Self { path, current: RwLock::new(current) }
    }

    pub fn get(&self) -> AppSettings {
        self.current.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn replace(&self, settings: AppSettings) -> std::io::Result<()> {
        let mut current = self.current.write().unwrap_or_else(|p| p.into_inner());
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(&settings)?)?;
        std::fs::rename(&temp, &self.path)?;
        *current = settings;
        Ok(())
    }
}
