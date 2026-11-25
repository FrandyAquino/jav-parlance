//! Registers the host with Chrome and Edge for the current user.
//!
//! Windows looks the manifest up through the registry, so it lives next to the
//! executable; macOS and Linux read it from fixed per-user directories.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde_json::{Value, json};

pub const HOST_NAME: &str = "app.parlance.native_host";
const EXTENSION_ID_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Edge,
}

impl Browser {
    pub const ALL: [Browser; 2] = [Browser::Chrome, Browser::Edge];

    /// Key under `HKEY_CURRENT_USER` whose default value points to the manifest.
    pub fn registry_key(self) -> String {
        let vendor = match self {
            Browser::Chrome => r"Google\Chrome",
            Browser::Edge => r"Microsoft\Edge",
        };
        format!(r"Software\{vendor}\NativeMessagingHosts\{HOST_NAME}")
    }

    /// Per-user manifest directory on macOS.
    pub fn macos_manifest_dir(self, home: &Path) -> PathBuf {
        let profile = match self {
            Browser::Chrome => "Google/Chrome",
            Browser::Edge => "Microsoft Edge",
        };
        home.join("Library/Application Support").join(profile).join("NativeMessagingHosts")
    }

    /// Per-user manifest directory on Linux.
    pub fn linux_manifest_dir(self, home: &Path) -> PathBuf {
        let profile = match self {
            Browser::Chrome => "google-chrome",
            Browser::Edge => "microsoft-edge",
        };
        home.join(".config").join(profile).join("NativeMessagingHosts")
    }
}

/// Chrome extension ids are 32 characters drawn from `a`..=`p`.
pub fn validate_extension_id(id: &str) -> anyhow::Result<()> {
    if id.len() != EXTENSION_ID_LEN || !id.bytes().all(|b| (b'a'..=b'p').contains(&b)) {
        bail!("`{id}` is not a Chromium extension id (32 letters a-p, see chrome://extensions)");
    }
    Ok(())
}

pub fn manifest_json(host_path: &Path, extension_id: &str) -> Value {
    json!({
        "name": HOST_NAME,
        "description": "Parlance browser-tab audio bridge",
        "path": host_path,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{extension_id}/")],
    })
}

/// Installs the manifest for every supported browser; returns where it was written.
pub fn install(extension_id: &str) -> anyhow::Result<Vec<String>> {
    validate_extension_id(extension_id)?;
    let host_path = std::env::current_exe().context("locating the host executable")?;
    let manifest = serde_json::to_vec_pretty(&manifest_json(&host_path, extension_id))?;
    platform::register(&host_path, &manifest)
}

fn write_manifest(dir: &Path, manifest: &[u8]) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{HOST_NAME}.json"));
    fs::write(&path, manifest).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    use anyhow::Context;
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    use super::{Browser, write_manifest};

    pub fn register(host_path: &Path, manifest: &[u8]) -> anyhow::Result<Vec<String>> {
        let dir = host_path.parent().context("host executable has no parent directory")?;
        let manifest_path = write_manifest(dir, manifest)?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let mut written = vec![manifest_path.display().to_string()];
        for browser in Browser::ALL {
            let key_path = browser.registry_key();
            let (key, _) = hkcu.create_subkey(&key_path).with_context(|| format!("creating HKCU\\{key_path}"))?;
            key.set_value("", &manifest_path.as_os_str()).with_context(|| format!("setting HKCU\\{key_path}"))?;
            written.push(format!("HKCU\\{key_path}"));
        }
        Ok(written)
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::{Path, PathBuf};

    use anyhow::Context;
    use directories::BaseDirs;

    use super::{Browser, write_manifest};

    pub fn register(_host_path: &Path, manifest: &[u8]) -> anyhow::Result<Vec<String>> {
        let home = BaseDirs::new().context("resolving the home directory")?.home_dir().to_path_buf();
        Browser::ALL
            .into_iter()
            .map(|browser| write_manifest(&manifest_dir(browser, &home), manifest).map(|p| p.display().to_string()))
            .collect()
    }

    fn manifest_dir(browser: Browser, home: &Path) -> PathBuf {
        if cfg!(target_os = "macos") { browser.macos_manifest_dir(home) } else { browser.linux_manifest_dir(home) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "abcdefghijklmnopabcdefghijklmnop";

    #[test]
    fn manifest_allows_only_the_given_extension() {
        let manifest = manifest_json(Path::new("/opt/parlance/parlance-native-host"), ID);
        assert_eq!(
            manifest,
            json!({
                "name": "app.parlance.native_host",
                "description": "Parlance browser-tab audio bridge",
                "path": "/opt/parlance/parlance-native-host",
                "type": "stdio",
                "allowed_origins": ["chrome-extension://abcdefghijklmnopabcdefghijklmnop/"],
            })
        );
    }

    #[test]
    fn accepts_well_formed_extension_ids() {
        assert!(validate_extension_id(ID).is_ok());
    }

    #[test]
    fn rejects_malformed_extension_ids() {
        for id in ["", "short", "ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP", "abcdefghijklmnopabcdefghijklmnoz", "a/b"] {
            assert!(validate_extension_id(id).is_err(), "{id} should be rejected");
        }
    }

    #[test]
    fn registry_keys_target_each_browser() {
        assert_eq!(
            Browser::Chrome.registry_key(),
            r"Software\Google\Chrome\NativeMessagingHosts\app.parlance.native_host"
        );
        assert_eq!(
            Browser::Edge.registry_key(),
            r"Software\Microsoft\Edge\NativeMessagingHosts\app.parlance.native_host"
        );
    }

    #[test]
    fn unix_manifest_dirs_follow_browser_docs() {
        let home = Path::new("/home/u");
        assert_eq!(
            Browser::Chrome.linux_manifest_dir(home),
            Path::new("/home/u/.config/google-chrome/NativeMessagingHosts")
        );
        assert_eq!(
            Browser::Edge.macos_manifest_dir(home),
            Path::new("/home/u/Library/Application Support/Microsoft Edge/NativeMessagingHosts")
        );
    }

    #[test]
    fn writes_manifest_file_named_after_host() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_manifest(dir.path(), b"{}").expect("write");
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("app.parlance.native_host.json"));
        assert_eq!(fs::read(path).expect("read"), b"{}");
    }
}
