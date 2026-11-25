---
paths:
  - "crates/**"
  - "apps/**"
  - "fixtures/**"
---

# Testing rules

- Every behaviour change ships with a test in the same change. Details: `docs/testing.md`.
- Unit tests live in `#[cfg(test)] mod tests` next to the code; cross-crate flows go in `<crate>/tests/`.
- Fake ports instead of mocking internals: `ReplayBackend` for capture, scripted `SpeechRecognizer`, `EnergyVad` or a
  scripted `VoiceActivityDetector`, fake `LanguageModel`/`Translator`, local `TcpListener` for HTTP and the bridge.
- Tests needing models, devices or a browser must **skip cleanly** when unavailable (see
  `crates/asr/tests/whisper_fixture.rs`), never fail CI.
- Prefer bounded polling (`wait_until`, retry loops with a cap) over fixed sleeps.
- Integration test files start with `#![allow(clippy::unwrap_used, clippy::expect_used)]`.
- Audio fixtures are 16 kHz mono 16-bit WAV in `fixtures/audio/`; generate speech with `scripts/make-fixture.ps1`.
- After touching a contract, run `cargo test --workspace` and commit the regenerated `apps/desktop/src/bindings`.
- Frontend: keep logic in pure functions (`reduceSession`, `toPlainText`, extension `lib/`) and test those with vitest.
- Performance-sensitive changes: run `parlance-cli bench` and/or `cargo bench -p parlance-audio` and record results
  in `docs/benchmarks/`.
