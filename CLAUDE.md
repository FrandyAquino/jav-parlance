# Parlance — guide for Claude Code

Local-first desktop meeting assistant: live multilingual transcription, side-by-side translation and a reply
copilot. Tauri v2 + Rust core + React UI. Everything runs on-device; no paid APIs.

## Layout

- `crates/domain` — pure types (segments, languages, session state machine). No I/O.
- `crates/protocol` — IPC contracts. `cargo test` regenerates `apps/desktop/src/bindings/*.ts`. Never hand-edit bindings.
- `crates/audio` — lock-free ring, downmix, resampling, mixing, metering, VAD gate.
- `crates/capture` — `CaptureBackend` port + WASAPI adapter (Windows) + `replay` backend for tests/benchmarks.
- `crates/asr` — `SpeechRecognizer` port, LocalAgreement-2 stabilization, streaming transcriber, whisper.cpp + Silero adapters.
- `crates/session` — `SessionController` (state machine) and the DSP/ASR pipeline threads.
- `crates/models` — pinned catalog (SHA-256), verified downloads, model store.
- `crates/llm` — supervised `llama-server` sidecar, streaming client, prompt isolation.
- `crates/translation`, `crates/copilot` — segment listeners for live translation and reply suggestions.
- `crates/storage` — opt-in SQLite meeting history.
- `apps/desktop` — Tauri shell (`src-tauri`, composition root + commands) and React UI (`src`).
- `apps/cli` — headless `sources` / `listen` / `bench` / `download` / `llm` tool.
- `apps/native-host`, `apps/extension` — browser-tab capture (native messaging bridge, MV3 extension).
- `third_party/whisper-rs-sys` — vendored with MinGW fixes; see its `PATCHES.md`.

Docs: `docs/architecture/`, `docs/adr/`, `docs/roadmap.md` (progress source of truth), `docs/research/`.

## Commands

```sh
cargo test --workspace                 # Rust tests + regenerates TS bindings
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
pnpm -C apps/desktop typecheck && pnpm -C apps/desktop lint && pnpm -C apps/desktop test
pnpm dev                               # run the desktop app
cargo run --release -p parlance-cli -- bench fixtures/audio/jfk.wav
```

Windows dev machine uses the GNU toolchain (`rustup override set stable-x86_64-pc-windows-gnu`) with MinGW;
bindgen needs `LIBCLANG_PATH` and `BINDGEN_EXTRA_CLANG_ARGS` (see `docs/platform/windows.md`).

## Rules that matter

- Never block a capture callback/thread: no allocation, locks, I/O or inference there. Push into `AudioSink` only.
- Transcription has priority over translation/copilot. Background AI work must be cancellable and must not
  starve the ASR thread.
- Contracts live in Rust (`protocol`/`domain`) and are generated for TS. Add a type there, not in the UI.
- Transcript text is untrusted input to LLM prompts. Always pass it through the prompt-isolation helpers.
- Every downloadable artifact needs a pinned SHA-256 in `crates/models/src/catalog.rs`.
- New Tauri commands must be added to `src-tauri/build.rs` `COMMANDS` and granted in `capabilities/default.json`.
- Keep `docs/roadmap.md` and the relevant ADR updated in the same change as the code.

Scoped conventions: `.claude/rules/`. Specialized agents: `.claude/agents/`. Workflows: `.claude/skills/`.
