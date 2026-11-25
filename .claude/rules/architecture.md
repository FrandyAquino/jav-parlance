---
paths:
  - "crates/**"
  - "apps/**"
  - "Cargo.toml"
  - "docs/architecture/**"
---

# Architecture rules

- Dependencies point toward `crates/domain`. `domain` and `llm` depend on no Parlance crate. `session` must never
  depend on `translation`, `copilot`, `storage` or `llm`; those plug in as `SegmentListener`s from the composition
  root. Graph: `docs/architecture/overview.md`.
- Core crates have no Tauri dependency. `apps/desktop/src-tauri` only wires adapters and maps errors; business logic
  belongs in a crate (so `apps/cli` can reuse it).
- New capability = port (trait) in the owning crate + adapter behind it (`CaptureBackend`, `SpeechRecognizer`,
  `VoiceActivityDetector`, `LanguageModel`, `Translator`, `SpeechEngine`). Tests use fakes of the port.
- Contracts that cross a process or language boundary live in `crates/protocol` (or `domain`) with
  `#[derive(TS)] #[ts(export)]`; u64/i64 fields get `#[ts(type = "number")]`.
- Everything the UI sees flows through the single `SessionEvent` channel or a per-call `Channel`. Raw audio never
  crosses into the webview.
- ASR has priority: real-time stages are dedicated OS threads with bounded queues; AI work is async, cancellable and
  goes through the `llama-server` sidecar (never link `llama-cpp-2` next to `whisper-rs`, ADR 0004).
- Queues are bounded or rate-limited and their overflow behaviour is explicit (drop + count + notice).
- Session lifecycle changes go through `SessionState::apply`; add states/commands there with tests.
- A decision that changes structure gets an ADR in `docs/adr/` and updates `docs/architecture/*` and
  `docs/roadmap.md` in the same change.
