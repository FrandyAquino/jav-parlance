use std::process::Command;

use parlance_domain::{SourceDescriptor, SourceId, SourceKind};
use serde_json::Value;

use crate::CaptureError;

fn pactl(args: &[&str]) -> Result<String, CaptureError> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|e| CaptureError::Unsupported(format!("pactl not found ({e}); install pulseaudio-utils")))?;
    if !output.status.success() {
        return Err(CaptureError::Backend(String::from_utf8_lossy(&output.stderr).trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// JSON output needs PulseAudio 16+ or pipewire-pulse.
fn pactl_json(kind: &str) -> Result<Vec<Value>, CaptureError> {
    let raw = pactl(&["--format=json", "list", kind])?;
    serde_json::from_str(&raw).map_err(|e| CaptureError::Backend(format!("unexpected pactl output: {e}")))
}

pub fn server_available() -> bool {
    pactl(&["info"]).is_ok()
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn property(value: &Value, key: &str) -> Option<String> {
    value.get("properties").and_then(|p| p.get(key)).and_then(Value::as_str).map(str::to_owned)
}

fn descriptor(id: String, kind: SourceKind, name: String, is_default: bool) -> SourceDescriptor {
    SourceDescriptor { id: SourceId(id), kind, name, is_default }
}

pub fn list_sources() -> Result<Vec<SourceDescriptor>, CaptureError> {
    let mut sources = vec![descriptor(
        "system:default".into(),
        SourceKind::SystemOutput,
        "System audio (default output)".into(),
        true,
    )];
    sources.extend(parse_sinks(&pactl_json("sinks")?));
    sources.extend(parse_sink_inputs(&pactl_json("sink-inputs")?));
    sources.push(descriptor("mic:default".into(), SourceKind::Microphone, "Default microphone".into(), true));
    sources.extend(parse_microphones(&pactl_json("sources")?));
    Ok(sources)
}

fn parse_sinks(sinks: &[Value]) -> Vec<SourceDescriptor> {
    sinks
        .iter()
        .filter_map(|sink| {
            let name = text(sink, "name")?;
            let label = text(sink, "description").unwrap_or_else(|| name.clone());
            Some(descriptor(format!("system:{name}"), SourceKind::SystemOutput, label, false))
        })
        .collect()
}

/// Monitor sources mirror sinks and are already offered as system audio.
fn parse_microphones(sources: &[Value]) -> Vec<SourceDescriptor> {
    sources
        .iter()
        .filter(|source| text(source, "monitor_of_sink").is_none_or(|m| m.is_empty() || m == "n/a"))
        .filter_map(|source| {
            let name = text(source, "name")?;
            if name.ends_with(".monitor") {
                return None;
            }
            let label = text(source, "description").unwrap_or_else(|| name.clone());
            Some(descriptor(format!("mic:{name}"), SourceKind::Microphone, label, false))
        })
        .collect()
}

fn parse_sink_inputs(inputs: &[Value]) -> Vec<SourceDescriptor> {
    inputs
        .iter()
        .filter_map(|input| {
            let index = input.get("index").and_then(Value::as_u64)?;
            let pid = property(input, "application.process.id").and_then(|p| p.parse().ok()).unwrap_or(0);
            let name = property(input, "application.name").unwrap_or_else(|| format!("Stream {index}"));
            Some(descriptor(format!("app:{index}"), SourceKind::Application { pid }, name, false))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pactl_json() {
        let sinks: Vec<Value> =
            serde_json::from_str(r#"[{"index":1,"name":"alsa_output.pci","description":"Speakers"}]"#).unwrap();
        let sources: Vec<Value> = serde_json::from_str(
            r#"[{"name":"alsa_output.pci.monitor","monitor_of_sink":"alsa_output.pci"},
                {"name":"alsa_input.usb","description":"USB Mic","monitor_of_sink":"n/a"}]"#,
        )
        .unwrap();
        let inputs: Vec<Value> = serde_json::from_str(
            r#"[{"index":7,"properties":{"application.name":"Firefox","application.process.id":"4242"}}]"#,
        )
        .unwrap();

        assert_eq!(parse_sinks(&sinks)[0].id.0, "system:alsa_output.pci");
        let mics = parse_microphones(&sources);
        assert_eq!(mics.len(), 1);
        assert_eq!(mics[0].name, "USB Mic");
        let apps = parse_sink_inputs(&inputs);
        assert_eq!(apps[0].id.0, "app:7");
        assert_eq!(apps[0].kind, SourceKind::Application { pid: 4242 });
    }
}
