# macOS

Status: **native capture not implemented.** `parlance_capture::platform_backend()` returns
`CaptureError::Unsupported` on macOS; the desktop app logs a warning and starts with only the browser-tab bridge,
which should work there in principle (unverified).

What already exists for macOS:

- Catalog entry `llama-server` = `llama-b12686-bin-macos-arm64.tar.gz` (Apple Silicon only; no Intel entry).
- `parlance-asr` / desktop feature `metal` forwards to `whisper-rs/metal`.
- Tauri bundle `minimumSystemVersion: 13.0`; window effect `hudWindow` in `tauri.conf.json`.
- Native host install writes manifests to `~/Library/Application Support/{Google/Chrome,Microsoft Edge}/NativeMessagingHosts/`.

## Plan

### Primary: Core Audio process taps (macOS 14.2+)

1. Translate PIDs to process objects (`kAudioHardwarePropertyTranslatePIDToProcessObject`).
2. Build a `CATapDescription` — global tap excluding Parlance's own process for `system:default`, or a stereo
   mixdown of one process tree for `app:<pid>`.
3. `AudioHardwareCreateProcessTap`, then a private aggregate device with the tap UID in
   `kAudioAggregateDeviceTapListKey` and tap auto-start.
4. Run an IOProc on the aggregate device that only copies into the `AudioSink` (same rules as WASAPI: no locks,
   allocation or I/O).
5. Bindings: `cidre` (`core_audio::{TapDesc, Tap, AggregateDevice}`, feature `macos_14_2`; pin the version, the API
   moves) or the lower-level `objc2-core-audio`.
6. Implement as `crates/capture/src/macos/` behind `#[cfg(target_os = "macos")]` and return it from
   `platform_backend()`.

### Permission

- Add `NSAudioCaptureUsageDescription` to the app's Info.plist. The "System Audio Recording" prompt appears on the
  first start of the aggregate device.
- No public preflight API is known; detect denial by start errors or sustained digital silence and surface it as
  `CaptureError::PermissionDenied`.
- Microphones need `NSMicrophoneUsageDescription`.

### Fallback: ScreenCaptureKit (macOS 13+)

`SCStreamConfiguration.capturesAudio` + `excludesCurrentProcessAudio`; needs Screen Recording permission and a
display/window content filter even for audio only. Use when taps are unavailable (13.x–14.1).

### Metal

Build with `--features metal` for whisper.cpp; the downloaded `llama-server` macOS build uses Metal itself.

### Packaging (planned)

The sidecar must be code-signed and notarized together with the app.

See `docs/research/platform-capture.md` for sources.
