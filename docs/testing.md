# Testing

Goal: every layer is testable headlessly and deterministically. Hardware, models and browsers are only needed for
the explicitly marked tests.

## Quick reference

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                       # also regenerates apps/desktop/src/bindings
git diff --exit-code apps/desktop/src/bindings

pnpm install
pnpm -C apps/desktop typecheck && pnpm -C apps/desktop lint && pnpm -C apps/desktop test
pnpm -C apps/extension typecheck && pnpm -C apps/extension lint && pnpm -C apps/extension test
```

`cargo test --workspace` builds `apps/desktop/src-tauri`, whose `tauri::generate_context!` embeds
`apps/desktop/dist`. Run `pnpm -C apps/desktop build` first (or `mkdir apps/desktop/dist`). The desktop crate itself
has `test = false` (Windows test binaries lack the Common Controls manifest); it is pure wiring.

Clippy: `unwrap_used` warns outside tests (`clippy.toml` allows `unwrap`/`expect` in tests); integration test files
start with `#![allow(clippy::unwrap_used, clippy::expect_used)]`.

## Layers

| Layer | What | Where | Run |
| --- | --- | --- | --- |
| Unit | State machine, language codes, event JSON shape, framing limits, VAD gate, mixer, LocalAgreement, streaming transcriber with a scripted recognizer, WER, prompt isolation, SSE parser, translation/copilot workers with fake models, question detector, catalog invariants, pactl JSON parsing, source-id parsing | `#[cfg(test)]` modules in each crate | `cargo test -p <crate>` |
| Concurrency | SPSC ring: 100 000 samples across threads in order; overflow drops and counts; abandoned halves | `crates/audio/src/ring.rs` | `cargo test -p parlance-audio ring` |
| Integration: pipeline | `SessionController` + `ReplayBackend` (48 kHz stereo tone clips) + `EchoRecognizer` + `EnergyVad`: final segment and auto-stop, manual stop finalizes, unknown source fails, double start/stop rejected | `crates/session/tests/pipeline.rs` | `cargo test -p parlance-session --test pipeline` |
| Integration: downloads | Verified download against a local `tokio` HTTP server: install on match, discard on checksum mismatch, size overrun, cancellation | `crates/models/tests/download.rs` | `cargo test -p parlance-models --test download` |
| Integration: bridge TCP | Authenticated tab streams PCM into an attached sink; wrong token rejected; discovery file removed on drop | `crates/capture/src/bridge.rs` tests | `cargo test -p parlance-capture bridge` |
| Integration: native host | Relay through a real loopback listener: forwarding, stdin close, missing discovery, protocol mismatch, ordering, sample rate, oversized frames | `apps/native-host/tests/relay.rs` | `cargo test -p parlance-native-host` |
| LLM supervisor | Missing executable → `Restarting` → `Stopped` on shutdown; loopback + API key args | `crates/llm/src/server.rs` tests | `cargo test -p parlance-llm` |
| Audio fixture (real models) | Streams `fixtures/audio/jfk.wav` through Silero + Whisper base Q8; asserts partials exist, WER < 0.15, language `en` | `crates/asr/tests/whisper_fixture.rs` | see below |
| Frontend | Session reducer (revisions, ordering, empty finals, translation alignment, row cap, notices), history export | `apps/desktop/src/**/*.test.ts` (vitest, jsdom) | `pnpm -C apps/desktop test` |
| Extension | Downmix, sinc resampler (gain, tone, anti-aliasing, block-size invariance), PCM16, base64, frame batcher, pipeline, wire shape, host message parsing, error reasons, guards | `apps/extension/src/lib/*.test.ts` (vitest, node) | `pnpm -C apps/extension test` |

## Audio fixtures

`fixtures/audio/`: `jfk.wav` (16 kHz mono 16-bit), `meeting_question_en.wav`, `meeting_question_es.wav`.

The fixture test **skips (passes) when models are absent**, so CI stays green without downloads. To run it:

```sh
cargo run --release -p parlance-cli -- download whisper-base-q8 silero-vad-5.1.2
PARLANCE_MODELS_DIR=<models dir> cargo test -p parlance-asr --release --test whisper_fixture -- --nocapture
```

Without `PARLANCE_MODELS_DIR` it looks in `%LOCALAPPDATA%\parlance\models` or `$XDG_DATA_HOME/parlance/models`.
On macOS, or on Linux with `XDG_DATA_HOME` unset, that fallback does not match the app's store; set the variable.

New TTS fixtures (Windows, `System.Speech`):

```powershell
scripts/make-fixture.ps1 -Text "Could you send me the numbers by Friday?" -Out fixtures/audio/question_en.wav -Culture en-US
```

The script writes 16 kHz mono 16-bit WAV and fails if no voice for the culture is installed.

## Benchmarks

Results are recorded in `docs/benchmarks/`.

```sh
# Streaming ASR on a WAV via ReplayBackend (real time by default; --speed 0 = as fast as possible)
cargo run --release -p parlance-cli -- bench fixtures/audio/jfk.wav \
  --reference "And so my fellow Americans, ask not what your country can do for you, ask what you can do for your country."
#   reports: model load, processing RTF, first-partial and final latency p50/p95 (speed > 0 only),
#   segment counts, peak RSS, average CPU, WER, transcript

# LLM sidecar: start-up time, GPU offload, cold/warm translation first-token and total latency
cargo run --release -p parlance-cli -- llm "Can you send me the report by Friday?" --from en --to es [--cpu] [--model qwen3.5-4b-q4]

# Live capture smoke test
cargo run --release -p parlance-cli -- sources
cargo run --release -p parlance-cli -- listen --source system:default --seconds 30 [--language en]

# DSP micro-benchmarks (criterion): downmix and 48k→16k resample of 10 ms
cargo bench -p parlance-audio --bench dsp
```

`bench` and `llm` need installed models (`parlance-cli models`, `parlance-cli download <ids>`).

## Manual / not automated

- Real devices: WASAPI loopback, process loopback, mic privacy denial, device unplug/replug.
- Linux adapter on PulseAudio and PipeWire (never run).
- Extension in Chrome/Edge (never loaded).
- Desktop UI end-to-end (no E2E harness yet).

## Writing tests

- Fake the ports, not the internals: `SpeechRecognizer`, `VoiceActivityDetector`, `SpeechEngine`, `CaptureBackend`
  (`ReplayBackend`), `LanguageModel`, `Translator`.
- Pipeline tests use `#[tokio::test(flavor = "multi_thread")]` and poll with a bounded wait instead of fixed sleeps
  where possible.
- Anything needing models or hardware must skip cleanly when they are missing.

## macOS acceptance

Run `cargo run -p parlance-capture --example capture_probe` to enumerate sources without requesting recording.
Pass `system:default`, an `app:<pid>` or `mic:default` and a duration to measure real PCM without saving audio.
Play a fixture through a different process; nonzero samples and zero drops demonstrate capture only. Use
`parlance-cli listen`, `bench` and `llm` separately to verify recognition and translation. Real-model tests
resolve the same platform data directory as the application, or `PARLANCE_MODELS_DIR` when supplied.

`cargo test -p parlance-llm` includes Unix subprocess stop/drop and owner-SIGKILL cases. These verify the
lifetime pipe on the current Unix host, not Windows Job Objects or power-loss behavior. History completion
is tested by joining its writer and reopening SQLite; packaged quit acceptance is a separate check.

Build an installer using `pnpm build`, or use the universal command in `docs/platform/macos.md`. The release
profile deliberately avoids Rust's stripping pass because macOS 27 rejects misaligned Mach-O LINKEDIT
sections produced by affected Rust/LLVM versions (rust-lang/rust#157750).
