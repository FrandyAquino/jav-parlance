---
name: native-audio
description: Workflow for adding or fixing a native capture adapter or real-time DSP stage in Parlance (WASAPI, Linux pactl/parec, macOS Core Audio taps, browser bridge, ring/resampler/VAD). Use for new platforms, device-loss handling, dropouts and timing bugs.
---

# Native audio

1. Read `docs/platform/<os>.md`, `docs/architecture/concurrency.md`, ADR 0008, `crates/capture/src/lib.rs`.
2. **Adapter layout.** `crates/capture/src/<os>/{mod.rs,sources.rs|<tool>.rs,stream.rs}` behind
   `#[cfg(target_os = "...")]`; register it in `platform_backend()`. Platform deps go under
   `[target.'cfg(...)'.dependencies]` in `crates/capture/Cargo.toml`.
3. **Source ids.** Reuse `system:default|<id>`, `mic:default|<id>`, `app:<id>`; implement `Target::parse` and unit
   tests for it. `default` must follow the OS default device.
4. **Stream thread.**
   - Named thread (`parlance-<backend>-capture`); `start()` waits on a ready channel and returns open errors.
   - Loop: wait for data with a timeout, check the stop flag and `sink.is_consumer_alive()`, copy into reused
     buffers, `sink.push`. Nothing else.
   - Fixed output format reported by `format()`; DSP resamples to 16 kHz.
   - Silence-fill gaps if the API delivers nothing during silence.
   - Device loss: emit `DeviceLost`, retry (500 ms × 40 like WASAPI/parec), then `Reconnected` or `Failed`.
   - `Drop`: set stop flag, join thread, kill children.
5. **Capabilities.** Report only what works (`CaptureCapabilities`); map permission errors to `PermissionDenied`
   with the fix in the message.
6. **DSP changes** (`crates/audio`, `crates/session/src/dsp.rs`): keep `AudioSink` allocation- and lock-free; add
   unit tests; benchmark with `cargo bench -p parlance-audio --bench dsp`.
7. **Verify.**
   ```sh
   cargo test -p parlance-capture -p parlance-audio -p parlance-session
   cargo clippy --workspace --all-targets -- -D warnings
   cargo run --release -p parlance-cli -- sources
   cargo run --release -p parlance-cli -- listen --source system:default --seconds 20
   ```
   Cross-check compile: `cargo check -p parlance-capture --target x86_64-unknown-linux-gnu` (target installed).
8. **Docs.** Update `docs/platform/<os>.md` status honestly (compiled vs. run vs. verified), the ADR, and
   `docs/roadmap.md`.
