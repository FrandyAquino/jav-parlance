# Platform Audio Capture

Verified 2026-11-24 against Microsoft Learn, Apple developer documentation (JSON), PipeWire docs, Chrome developer docs, crates.io and crate source.

## Rust crates

| Crate | Latest | Updated | License | Role and maturity |
|---|---|---|---|---|
| `cpal` | 0.18.2 | 2026-08-16 | Apache-2.0 | Mic on all OSes. WASAPI **endpoint loopback**: an input stream on a render device sets `AUDCLNT_STREAMFLAGS_LOOPBACK`. macOS loopback since 0.17 (macOS 14.6+, tap and aggregate device, several fixes in 0.18). Native PipeWire host added in 0.18, plus PulseAudio and JACK features. Mature, with about 11.7M recent downloads. |
| `wasapi` | 0.27.0 | 2026-11-06 | MIT | Windows. Endpoint loopback **and process loopback** (`AudioClient::new_application_loopback_client(pid, include_tree)`, example `record_application`). Used by Hyprnote/anarlog. Mature. |
| `cidre` | 0.30.1 | 2026-11-13 | MIT/Apache (check) | macOS. `core_audio::{TapDesc, Tap, TapGuard, AggregateDevice}` cover process taps (`macos_14_2` feature) and ScreenCaptureKit. Used by Meetily and anarlog. The API changes often, so pin it. |
| `objc2-core-audio` | 0.3.2 | 2025-10-04 | MIT/Apache/Zlib | macOS raw bindings. docs.rs lists `AudioHardwareCreateProcessTap` and `CATapDescription`. Auto-generated and low-level. |
| `coreaudio-rs` | 0.14.2 | 2026-04-29 | MIT/Apache | macOS AudioUnit wrapper, used by cpal. No tap-specific API (unverified). |
| `screencapturekit` | 12.0.0 | 2026-11-21 | MIT/Apache (check) | macOS ScreenCaptureKit bindings. Active. |
| `pipewire` / `libspa` | 0.10.1 | 2026-08-19 | MIT | Linux, official freedesktop bindings. Usable, but the API is still 0.x. |
| `libpulse-binding` | 2.30.1 | 2025-04-19 | MIT/Apache | Linux PulseAudio and `pipewire-pulse`. Stable. Used by Meetily and anarlog. |

## Windows (WASAPI)

**Endpoint loopback (all system audio):**
- `IAudioClient::Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, …)` on the **render** endpoint. Supported since Vista.
- It captures everything mixed to that endpoint, so you have to follow default-device changes. anarlog re-opens the stream on endpoint switch.
- It returns silence while nothing is playing (no packets), so the timeline has to be padded.

**Process loopback (per app, or "everything except me"):**
- Call `ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, IID_IAudioClient, &params, …)`.
- Set `AUDIOCLIENT_ACTIVATION_PARAMS { ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, ProcessLoopbackParams = { TargetProcessId, PROCESS_LOOPBACK_MODE_{INCLUDE,EXCLUDE}_TARGET_PROCESS_TREE } }`.
- **Minimum client: Windows 10 build 20348** (Microsoft Learn requirements table, ApplicationLoopback sample). In practice that means Windows 11 or Server 2022. Whether it works on consumer Windows 10 22H2 (19045) is unverified, so feature-detect and fall back to endpoint loopback.
- Capture is not tied to an endpoint, so there is no need to follow device switches. The format must be supplied explicitly. `GetMixFormat`, `GetDevicePeriod` and `GetCurrentPadding` return "not implemented" (documented in `wasapi-rs`).
- Targeting a browser or Teams PID with `include_tree = true` captures all of its child processes.

**`wasapi-rs` pitfall:** in `new_application_loopback_client`, `include_tree = false` maps to `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`, which captures **everything except** the target. The doc comment says it captures "only the target process", which is wrong (verified in `src/api.rs`). That makes `(own_pid, false)` the right way to get "system audio minus Parlance".

## macOS

**Core Audio process taps (preferred):**
- `AudioHardwareCreateProcessTap(CATapDescription*, AudioObjectID*)` requires **macOS 14.2+**.
- `CATapDescription` initializers (`initStereoMixdownOfProcesses:`, `initStereoGlobalTapButExcludeProcesses:`, mono variants) are macOS 14.0+ in the headers. They take **process AudioObjectIDs**: translate a PID with `kAudioHardwarePropertyTranslatePIDToProcessObject`. The `bundleIDs` property is macOS 26+.
- To use a tap, create a **private aggregate device** (`AudioHardwareCreateAggregateDevice`) with the tap's UID in `kAudioAggregateDeviceTapListKey` and `tap auto start`, then run an IOProc on it. `muteBehavior` can mute the tapped process.
- **Permission:** the Info.plist key **`NSAudioCaptureUsageDescription`** (macOS 14.2+) is required. The system shows the "System Audio Recording" prompt on the first start of the aggregate device.
  - There is no public preflight API (unverified). Probe by starting capture and checking for silence or an error.
  - anarlog wraps TCC in its own `crates/tcc` (swift-rs).
- Both Meetily and anarlog use `TapDesc::with_mono_global_tap_excluding_processes([])` plus an aggregate device via `cidre`.
- cpal 0.17+ also implements this (it says loopback needs 14.6+).

**ScreenCaptureKit (fallback, macOS 13+):**
- Use `SCStreamConfiguration.capturesAudio` (13.0), `excludesCurrentProcessAudio` (13.0) and `captureMicrophone` (15.0).
- It needs **Screen Recording** permission, which is heavier and scarier than System Audio Recording. It also requires a display or window content filter even for audio-only capture.
- Meetily makes it the default and keeps Core Audio as the alternative.

## Linux

**PipeWire (preferred, `pipewire` 0.10):**
- Create a `pw_stream` with `media.class = Stream/Input/Audio`.
- **System mix:** set `stream.capture.sink = true` ("capture the sink output instead of source output") and optionally `target.object = <sink node.name or object.serial>`. This reads the sink's **monitor ports**.
- **Per-app:**
  - Enumerate `Stream/Output/Audio` nodes through the registry (use `application.name` and `application.process.id`).
  - Set `target.object` to that node's `object.serial` and `node.dont-reconnect = true`, so the capture is destroyed when the app stream ends.
  - Linking capture streams directly to app output nodes works with `pw-record --target <serial>` (unverified across WirePlumber versions).
  - Alternative: create a virtual null sink and link the app's ports to it manually. OBS `obs-pipewire-audio-capture` (GPL-2.0, needs PipeWire 0.3.62+ and WirePlumber) does app capture this way, but the exact mechanism is unverified.
- Set `stream.monitor = true` to allow a faster resampler.
- Flatpak needs `xdg-run/pipewire-0` access.

**PulseAudio fallback (`libpulse-binding` 2.30):**
- **System:** record from `<sink>.monitor` (the sink's `monitor_source_name`).
- **Per-app:** `pa_stream_set_monitor_stream(stream, sink_input_index)` before `connect_record` on the monitor captures one sink-input. This works under `pipewire-pulse` too.
- Meetily uses Pulse monitors only. anarlog links both pulse and pipewire.

## Chromium extension path (tab audio)

- **Manifest:** permissions `tabCapture`, `offscreen`, `nativeMessaging`.
- **Flow:**
  1. The user clicks the action (a gesture is required).
  2. The service worker calls `chrome.tabCapture.getMediaStreamId({ targetTabId })` (Chrome 71+, promise form 116+).
  3. It creates an offscreen document with `reasons: ["USER_MEDIA"]` (Chrome 109+). Only one offscreen document is allowed per extension, and USER_MEDIA has no lifetime limit (only AUDIO_PLAYBACK closes after 30 s of silence).
  4. The offscreen document calls `getUserMedia({ audio: { mandatory: { chromeMediaSource: "tab", chromeMediaSourceId: id } } })`. From Chrome 116, an ID minted in the service worker is usable in the offscreen document of the same render process. The ID is single-use and expires within seconds.
- **Keep the tab audible:** capture mutes local playback, so route it back with `new AudioContext()`, `createMediaStreamSource(stream)` and `.connect(ctx.destination)`.
- **Offscreen documents only get `chrome.runtime`.** Forward PCM (for example 16 kHz Int16 frames) to the service worker, which owns `runtime.connectNative()`. Native messaging is available in "extension pages and service worker". Whether it is callable directly from an offscreen document is unverified, so assume it is not.
- **Native messaging:**
  - Framing: a 32-bit native-endian length followed by UTF-8 JSON.
  - Limits: **host → Chrome 1 MB**, **Chrome → host 64 MiB**.
  - Binary audio must be base64-encoded in JSON (about 33% overhead). Send roughly 100–250 ms chunks.
  - `connectNative` keeps the host alive. `sendNativeMessage` spawns a host per message.
  - On Windows, set stdin/stdout to `O_BINARY`, and log only to stderr.
  - The host gets the caller's origin as `argv[1]`.
- **Host manifest `<name>.json`** with `allowed_origins: ["chrome-extension://<id>/"]` (no wildcards):

| OS | User scope | System scope |
|---|---|---|
| Windows | `HKCU\SOFTWARE\Google\Chrome\NativeMessagingHosts\<name>` (default value = manifest path) | `HKLM\…` (same path; 32-bit view checked first) |
| macOS | `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` | `/Library/Google/Chrome/NativeMessagingHosts/` |
| Linux | `~/.config/google-chrome/NativeMessagingHosts/` | `/etc/opt/chrome/native-messaging-hosts/` |

  Chromium paths: macOS `~/Library/Application Support/Chromium/NativeMessagingHosts/`; Linux `~/.config/chromium/NativeMessagingHosts/` and `/etc/chromium/native-messaging-hosts/`. Edge and Brave use their own paths and registry keys (not covered here).
- **Prior art:** anarlog ships `crates/chrome-native-host`, which validates the origin argument and relays to the desktop app through a local endpoint directory.

## Recommendation

| OS | Primary | Fallback |
|---|---|---|
| Windows | `wasapi` process loopback, excluding Parlance's own PID (build ≥ 20348) | `wasapi`/`cpal` endpoint loopback |
| macOS | Core Audio process tap via `cidre` (14.2+) | ScreenCaptureKit (13+) |
| Linux | PipeWire `stream.capture.sink` / `target.object` | PulseAudio `.monitor` / `set_monitor_stream` |
| Browser | tabCapture → offscreen → SW → native host | — |

Use `cpal` for microphones everywhere. Put the system-audio sources behind one `SystemAudioSource` trait (strategy pattern) with the per-OS implementations above.

## Sources

- https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ne-audioclientactivationparams-audioclient_activation_type
- https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/
- https://github.com/HEnquist/wasapi-rs (`src/api.rs`, `examples/record_application.rs`)
- https://github.com/RustAudio/cpal (README, CHANGELOG, `src/host/wasapi/device.rs`)
- https://developer.apple.com/documentation/coreaudio/audiohardwarecreateprocesstap(_:_:), https://developer.apple.com/documentation/coreaudio/catapdescription, https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps, https://developer.apple.com/documentation/bundleresources/information-property-list/nsaudiocaptureusagedescription
- https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration
- https://docs.pipewire.org/group__pw__keys.html, https://docs.pipewire.org/page_man_pipewire-props_7.html, https://github.com/dimtpap/obs-pipewire-audio-capture
- https://developer.chrome.com/docs/extensions/reference/api/tabCapture, https://developer.chrome.com/docs/extensions/reference/api/offscreen, https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging
- https://github.com/yury/cidre, https://github.com/fastrepl/anarlog, https://github.com/Zackriya-Solutions/meetily
