# macOS

Parlance supports **macOS 13.3+**, Apple Silicon and Intel. The official pinned llama.cpp runtime requires
13.3. Metal acceleration is included in normal Mac builds. No virtual audio driver is required.

## Setup

Install Xcode (or its command-line tools), stable Rust, Node 22+, pnpm and CMake. The Apple SDK must include
Core Audio process taps (14.2+); the produced app still supports 13.3 through runtime availability checks.

```sh
./scripts/setup-macos.sh
cargo run -p parlance-cli -- download whisper-base-q8 silero-vad-5.1.2 qwen3.5-2b-q4 llama-server
pnpm dev
```

Models live in `~/Library/Application Support/parlance/models`. Downloads are checked against pinned
size and SHA-256 before installation. Once downloaded, transcription, translation and copilot run offline.

## Capture

| Source | Implementation |
| --- | --- |
| System audio, 14.2+ | Private Core Audio stereo process tap; excludes Parlance's audio processes |
| Application, 14.2+ | Stereo tap of the selected PID and its current audio-producing descendants |
| System/application, 13.3–14.1 | ScreenCaptureKit audio stream, with a display filter and no video output attached |
| Microphone | Core Audio input device, selected by persistent device UID; `mic:default` also works in the CLI |

The adapter is in `crates/capture/src/macos/`. An Objective-C bridge owns Apple objects and performs
availability checks; Rust owns the ring buffer and monitor thread. Callbacks only copy float PCM into the
nonblocking ring, using fixed stack storage to interleave planar audio. No captured audio is written to disk.

Mic-only capture is selectable in the desktop source picker. A microphone can also accompany system/app audio.
The picker prevents selecting the same microphone twice. Application taps require the application to have an
audio stream: start playback before recording. New child processes created after a tap starts require restarting
the session. Changes to the default device, disconnections and format changes stop capture with a notice;
restart the session to use the new device or format.

## Permissions

The packaged app includes `NSAudioCaptureUsageDescription`, `NSMicrophoneUsageDescription` and the audio-input
entitlement. macOS requests the relevant permission at first use:

- Core Audio taps: **Privacy & Security → Screen & System Audio Recording** (wording varies by OS).
- ScreenCaptureKit: **Screen Recording**. It requires a display even though Parlance consumes audio only.
- Microphone: **Microphone**.

Explicit microphone denial and recognized native permission errors are reported as `permissionDenied`.
Digital silence is not classified as denial: it may simply mean nothing is playing. If the meter stays silent
while another app plays audio, check permission and the selected source. CLI permissions may be attributed to
the terminal or host app; permission granted there does not certify the packaged application's permissions.

## Build and install

```sh
CI=true pnpm build                              # app + DMG, without Finder automation
rustup target add aarch64-apple-darwin x86_64-apple-darwin
CI=true pnpm -C apps/desktop tauri build --target universal-apple-darwin
```

Bundles are under `target/release/bundle/`, or `target/universal-apple-darwin/release/bundle/` for universal builds.
Copy `Parlance.app` to Applications. The CI installer job builds a universal Mac DMG, Windows installers and
Linux packages as downloadable artifacts; it does not publish a release. Mac bundles use the ad-hoc signing identity `-` by default and are **not notarized releases**. Override
`bundle.macOS.signingIdentity` with a Developer ID for public distribution, which still requires a
Developer ID, signing credentials and notarization. Do not disable Gatekeeper to work around distribution setup.

## Hardware verification

```sh
cargo run -p parlance-capture --example capture_probe
cargo run -p parlance-capture --example capture_probe -- system:default 10
cargo run -p parlance-capture --example capture_probe -- app:1234 10
cargo run -p parlance-capture --example capture_probe -- mic:default 10
cargo run -p parlance-cli -- sources
cargo run -p parlance-cli -- listen --source system:default --seconds 20
cargo run -p parlance-cli -- bench fixtures/audio/jfk.wav
cargo run -p parlance-cli -- llm "Can we finish the proposal by Friday?"
```

The probe prints sample counts, peak level and drops without retaining audio. Play a known fixture from a
different process during a system/app probe. See [the Mac evidence record](../benchmarks/2026-11-25-macos.md)
and [roadmap](../roadmap.md) for what has actually been exercised. A successful build does not certify Intel
hardware, older macOS, Windows, Linux or browser-extension runtime behavior.
