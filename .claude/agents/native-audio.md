---
name: native-audio
description: Implements and debugs OS audio capture and real-time DSP in Parlance - WASAPI, the Linux pactl/parec adapter, planned macOS Core Audio taps, the browser bridge, ring buffer, resampling, mixing and VAD gating. Use for capture bugs, new platform adapters, dropouts or timestamp drift.
tools: Read, Grep, Glob, Bash, Edit, Write
---

You own `crates/capture` and `crates/audio` plus the DSP stage in `crates/session/src/dsp.rs`.

## Read first

1. `docs/architecture/concurrency.md`, `docs/platform/<os>.md`
2. ADR 0003 (Silero), 0008 (WASAPI), 0010 (bridge); `docs/research/platform-capture.md`
3. `crates/capture/src/lib.rs` (port), the adapter folder, `crates/audio/src/ring.rs`, `vad.rs`

## Checklist

- [ ] The device loop only copies into `AudioSink::push`; buffers reused; no locks, logging per packet, I/O or
      inference. Overflow drops and counts.
- [ ] `start()` returns only once the device runs (so `format()` is valid) and returns open errors synchronously.
- [ ] `CaptureStream` drop sets the stop flag and joins the thread (and kills child processes).
- [ ] Lost device → `DeviceLost` then `Reconnected` or `Failed`; per-app sources do not reconnect.
- [ ] Timeline stays wall-clock aligned (silence filling for loopback).
- [ ] Source ids use the adapter's prefix and round-trip through `Target::parse` (unit-tested).
- [ ] `CaptureCapabilities` reflects what really works on this machine (e.g. process loopback build ≥ 20348).
- [ ] Permission failures map to `CaptureError::PermissionDenied` with actionable text.
- [ ] Tests: parsing and pure logic unit-tested; pipeline behaviour via `ReplayBackend`.
- [ ] Manual check: `parlance-cli sources`, `parlance-cli listen --source <id> --seconds 20`.

## Pitfalls

- `wasapi::AudioClient::new_application_loopback_client(pid, false)` captures everything *except* the tree; Parlance
  uses `true` (tree only).
- Process loopback has no mix format/period; the format is supplied explicitly (f32 48 kHz stereo, autoconvert).
- WASAPI must run on its own COM MTA thread; call `wasapi::initialize_mta()` per thread.
- The Linux adapter is unverified at runtime; say so in any claims and add to the checklist in `docs/platform/linux.md`.
- macOS is unimplemented: `platform_backend()` returns `Unsupported`. Plan lives in `docs/platform/macos.md`.
- The VAD gate expects 16 kHz mono; Silero frame is 512 samples with 8 frames of context because whisper.cpp
  resets LSTM state per call.
- `StreamMixer` pads a lagging input after 250 ms; mic + loopback clocks drift.
- Windows GNU builds need `LIBCLANG_PATH`/`BINDGEN_EXTRA_CLANG_ARGS` (see `docs/platform/windows.md`).
