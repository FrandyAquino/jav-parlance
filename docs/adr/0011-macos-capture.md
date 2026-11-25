# 0011 — Native macOS capture through a narrow Objective-C bridge

Status: Accepted (2026-11-25).

## Context

The Mac adapter must capture system audio, individual applications and microphones without changing
`CaptureBackend`. Core Audio taps require 14.2; the pinned llama-server requires macOS 13.3.

## Decision

Keep the adapter in `crates/capture/src/macos`, behind the platform cfg. Compile an ARC Objective-C bridge
with `cc` against the installed Apple SDK. Native `@available` checks keep older OS support without pulling
in a large framework-binding dependency or raising the minimum OS to the newest API.

- 14.2+: private, unmuted stereo process tap, private aggregate device and IOProc.
- 13.3–14.1: ScreenCaptureKit audio output, excluding the current process or filtering one application.
- Microphones: Core Audio input IOProc after AVFoundation's explicit authorization check.
- Source IDs carry microphone UIDs or application PIDs; resolve current process descendants at tap creation.
- Callbacks pass borrowed float PCM to Rust. Rust interleaves using fixed stack chunks and pushes into
  `AudioSink`; no allocation, locks, inference or I/O in the callback.
- A monitor thread owns capture. Stop fences callbacks before releasing the boxed Rust context, destroys
  the IOProc/aggregate/tap, and joins the monitor.
- Device/default/format changes produce a failure notice; restart is explicit. Silence is not treated as
  proof of permission denial. Info.plist declares system-audio and microphone purpose strings.

Metal is enabled in normal Mac ASR builds. Mac whisper.cpp builds disable build-host CPU targeting so
installers remain portable. The domain, session, CLI and desktop retain the existing capture port.

## Consequences

Building needs an Apple SDK with process-tap declarations. Intel execution and the 13.3–14.1 fallback need
separate hardware evidence. Process membership is a snapshot; new subprocesses require restarting capture.
Signing and notarization remain distribution steps, separate from local correctness.

Sources: Apple SDK headers `AudioHardwareTapping.h`, `CATapDescription.h`, `AudioHardware.h`, `SCStream.h`;
[Apple process-tap documentation](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps).
