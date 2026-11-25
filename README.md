# Parlance

A floating, always-on-top desktop meeting assistant that runs **entirely on your machine**:

- live multilingual transcription with automatic language detection,
- the original and a live translation side by side,
- a copilot that spots questions and drafts replies in the speaker's language and yours,
- capture from system audio, a single application, a microphone, or a browser tab.

No account, no cloud, no paid API. Models download once, are SHA-256 verified, and everything works offline
afterwards.

> Status: early development. See [`docs/roadmap.md`](docs/roadmap.md) for what works today and what is planned.
> Windows has end-to-end evidence. macOS has native system/app/microphone capture and Metal support; see
> [Mac validation](docs/benchmarks/2026-11-25-macos.md). Linux runtime acceptance remains pending.

## How it works

```text
system / app / mic / tab audio ─▶ capture (WASAPI · Core Audio/ScreenCaptureKit · PulseAudio via parec · browser bridge)
   ─▶ lock-free ring buffer ─▶ downmix + resample to 16 kHz ─▶ Silero VAD
   ─▶ streaming Whisper (LocalAgreement-2 stabilization) ─▶ partial + final segments
   ─▶ translation + copilot (local Qwen via llama.cpp) ─▶ typed events ─▶ UI
```

Details: [architecture overview](docs/architecture/overview.md), [concurrency](docs/architecture/concurrency.md),
[IPC contracts](docs/architecture/ipc.md), [decision records](docs/adr/).

## Default models

| Purpose | Model | Size | License |
| --- | --- | --- | --- |
| Speech recognition | Whisper base (Q8, whisper.cpp) | 82 MB | MIT |
| Speech detection | Silero VAD v5.1.2 (ggml) | 0.9 MB | MIT |
| Translation & copilot | Qwen3.5 2B Instruct (Q4_K_M GGUF) | 1.3 GB | Apache-2.0 |
| LLM runtime | llama.cpp `llama-server` b12686 | 25–40 MB | MIT |

Larger options (Whisper small / large-v3-turbo, Qwen3.5 4B) are available in Settings → Models.

## Develop

Prerequisites: Rust (stable), Node 22+, pnpm, CMake, a C/C++ compiler, and libclang for bindgen.
Platform setup: [Windows](docs/platform/windows.md), [macOS 13.3+](docs/platform/macos.md), [Linux](docs/platform/linux.md).
On Mac, `./scripts/setup-macos.sh` prepares dependencies and builds the workspace; normal builds include Metal.

```sh
pnpm install
cargo test --workspace          # also regenerates the TypeScript contracts
pnpm dev                        # launches the desktop app
```

Headless tools:

```sh
cargo run --release -p parlance-cli -- sources
cargo run --release -p parlance-cli -- listen --source system:default --seconds 30
cargo run --release -p parlance-cli -- bench fixtures/audio/jfk.wav --reference "..."
```

## Privacy

Audio never leaves the device and is never written to disk. Meeting history is off by default; when enabled,
only text is stored locally in SQLite. The app makes network requests only to download models you choose.
See [`docs/security.md`](docs/security.md).

## License

MIT. Model licenses are listed above and in [`docs/research/model-licenses.md`](docs/research/model-licenses.md).
