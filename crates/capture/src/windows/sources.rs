use std::collections::BTreeMap;

use parlance_domain::{SourceDescriptor, SourceId, SourceKind};
use wasapi::{Device, DeviceEnumerator, Direction, SessionState};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{PWSTR, w};

use super::backend_error;
use crate::CaptureError;

const SYSTEM_PREFIX: &str = "system:";
const MIC_PREFIX: &str = "mic:";
const APP_PREFIX: &str = "app:";
const DEFAULT: &str = "default";

/// Process loopback (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`) exists
/// since Windows 10 build 20348 / Windows 11.
const PROCESS_LOOPBACK_MIN_BUILD: u32 = 20_348;

/// Parsed form of a [`SourceId`] produced by [`list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Loopback(Endpoint),
    Microphone(Endpoint),
    Process(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Follows the user's default device, reopening it when it changes.
    Default,
    Id(String),
}

impl Target {
    pub fn parse(id: &SourceId) -> Result<Self, CaptureError> {
        let raw = id.0.as_str();
        let endpoint = |rest: &str| if rest == DEFAULT { Endpoint::Default } else { Endpoint::Id(rest.to_owned()) };
        if let Some(rest) = raw.strip_prefix(SYSTEM_PREFIX) {
            Ok(Target::Loopback(endpoint(rest)))
        } else if let Some(rest) = raw.strip_prefix(MIC_PREFIX) {
            Ok(Target::Microphone(endpoint(rest)))
        } else if let Some(pid) = raw.strip_prefix(APP_PREFIX).and_then(|p| p.parse().ok()) {
            Ok(Target::Process(pid))
        } else {
            Err(CaptureError::SourceNotFound(raw.to_owned()))
        }
    }

    /// Whether a lost device should be reopened rather than ending the stream.
    pub fn is_reconnectable(&self) -> bool {
        !matches!(self, Target::Process(_))
    }

    pub fn open_device(&self) -> Result<Device, CaptureError> {
        let enumerator = DeviceEnumerator::new().map_err(|e| backend_error("device enumerator", e))?;
        let (direction, endpoint) = match self {
            Target::Loopback(endpoint) => (Direction::Render, endpoint),
            Target::Microphone(endpoint) => (Direction::Capture, endpoint),
            Target::Process(_) => unreachable!("process loopback does not use an endpoint device"),
        };
        match endpoint {
            Endpoint::Default => enumerator.get_default_device(direction),
            Endpoint::Id(id) => enumerator.get_device(id),
        }
        .map_err(|e| CaptureError::SourceNotFound(format!("{endpoint:?}: {e}")))
    }
}

pub fn supports_process_loopback() -> bool {
    windows_build().is_some_and(|build| build >= PROCESS_LOOPBACK_MIN_BUILD)
}

fn windows_build() -> Option<u32> {
    let mut buffer = [0u16; 16];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: `buffer` is valid for `size` bytes and both strings are static.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let chars = (size as usize / 2).saturating_sub(1);
    String::from_utf16_lossy(&buffer[..chars]).parse().ok()
}

pub fn list() -> Result<Vec<SourceDescriptor>, CaptureError> {
    let _com = wasapi::initialize_mta();
    let enumerator = DeviceEnumerator::new().map_err(|e| backend_error("device enumerator", e))?;

    let mut sources = vec![descriptor(
        format!("{SYSTEM_PREFIX}{DEFAULT}"),
        SourceKind::SystemOutput,
        "System audio (default output)",
        true,
    )];
    sources.extend(endpoints(&enumerator, Direction::Render, SYSTEM_PREFIX, SourceKind::SystemOutput)?);
    if supports_process_loopback() {
        sources.extend(applications(&enumerator)?);
    }
    sources.push(descriptor(format!("{MIC_PREFIX}{DEFAULT}"), SourceKind::Microphone, "Default microphone", true));
    sources.extend(endpoints(&enumerator, Direction::Capture, MIC_PREFIX, SourceKind::Microphone)?);
    Ok(sources)
}

fn descriptor(id: String, kind: SourceKind, name: impl Into<String>, is_default: bool) -> SourceDescriptor {
    SourceDescriptor { id: SourceId(id), kind, name: name.into(), is_default }
}

fn endpoints(
    enumerator: &DeviceEnumerator,
    direction: Direction,
    prefix: &str,
    kind: SourceKind,
) -> Result<Vec<SourceDescriptor>, CaptureError> {
    let collection = enumerator.get_device_collection(direction).map_err(|e| backend_error("device collection", e))?;
    let count = collection.get_nbr_devices().map_err(|e| backend_error("device count", e))?;
    let mut found = Vec::new();
    for index in 0..count {
        let Ok(device) = collection.get_device_at_index(index) else { continue };
        let (Ok(id), Ok(name)) = (device.get_id(), device.get_friendlyname()) else { continue };
        found.push(descriptor(format!("{prefix}{id}"), kind.clone(), name, false));
    }
    Ok(found)
}

/// Processes that currently own an active or inactive (not expired) session
/// on the default output, deduplicated by pid.
fn applications(enumerator: &DeviceEnumerator) -> Result<Vec<SourceDescriptor>, CaptureError> {
    let device = enumerator.get_default_device(Direction::Render).map_err(|e| backend_error("default output", e))?;
    let sessions = device
        .get_iaudiosessionmanager()
        .and_then(|manager| manager.get_audiosessionenumerator())
        .map_err(|e| backend_error("session enumerator", e))?;
    let count = sessions.get_count().map_err(|e| backend_error("session count", e))?;

    let mut by_pid = BTreeMap::new();
    for index in 0..count {
        let Ok(session) = sessions.get_session(index) else { continue };
        if matches!(session.get_state(), Ok(SessionState::Expired)) {
            continue;
        }
        let Ok(pid) = session.get_process_id() else { continue };
        if pid == 0 {
            continue;
        }
        let name = process_name(pid)
            .or_else(|| session.get_display_name().ok().filter(|n| !n.is_empty() && !n.starts_with('@')))
            .unwrap_or_else(|| {
                if session_is_system_sounds(&session) { "System sounds".into() } else { format!("Process {pid}") }
            });
        by_pid.entry(pid).or_insert(name);
    }

    Ok(by_pid
        .into_iter()
        .map(|(pid, name)| descriptor(format!("{APP_PREFIX}{pid}"), SourceKind::Application { pid }, name, false))
        .collect())
}

/// The system-sounds session is owned by the audio service and reports a
/// resource string (`@%SystemRoot%\System32\AudioSrv.Dll,-202`) as its name.
fn session_is_system_sounds(session: &wasapi::AudioSessionControl) -> bool {
    session.get_display_name().is_ok_and(|name| name.contains("AudioSrv"))
}

fn process_name(pid: u32) -> Option<String> {
    // SAFETY: the handle is checked and closed on every path; the buffer
    // outlives the call and its capacity is passed in `len`.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 512];
        let mut len = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        let file = path.rsplit('\\').next()?;
        Some(file.trim_end_matches(".exe").to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Target, CaptureError> {
        Target::parse(&SourceId(raw.into()))
    }

    #[test]
    fn parses_source_ids() {
        assert_eq!(parse("system:default").unwrap(), Target::Loopback(Endpoint::Default));
        assert_eq!(parse("mic:{0.0.1}").unwrap(), Target::Microphone(Endpoint::Id("{0.0.1}".into())));
        assert_eq!(parse("app:1234").unwrap(), Target::Process(1234));
        assert!(parse("app:abc").is_err());
        assert!(parse("tab:1").is_err());
    }

    #[test]
    fn only_endpoints_reconnect() {
        assert!(parse("system:default").unwrap().is_reconnectable());
        assert!(!parse("app:42").unwrap().is_reconnectable());
    }

    #[test]
    fn detects_windows_build() {
        assert!(windows_build().is_some_and(|b| b > 10_000));
    }
}
