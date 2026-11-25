---
name: streaming-inference
description: Workflow for changing streaming ASR, translation or copilot inference in Parlance - whisper params, LocalAgreement, VAD tuning, llama-server flags, prompts, or adding a model to the catalog. Use for latency/accuracy work and model upgrades.
---

# Streaming inference

1. Read ADRs 0002–0005 and 0009, `docs/architecture/concurrency.md`, and the latest `docs/benchmarks/`.
2. **Baseline** before changing anything:
   ```sh
   cargo run --release -p parlance-cli -- models
   cargo run --release -p parlance-cli -- bench fixtures/audio/jfk.wav --reference "And so my fellow Americans, ask not what your country can do for you, ask what you can do for your country."
   cargo run --release -p parlance-cli -- llm "Can you send me the report by Friday?" --from en --to es
   ```
3. **Where to change.**
   - Streaming policy: `crates/asr/src/streaming.rs` (`StreamingConfig`: step 1 s, min partial 700 ms),
     `agreement.rs` (LocalAgreement-2).
   - Whisper params: `crates/asr/src/whisper.rs` (`audio_ctx` for fast passes, `no_speech_thold`, threads).
   - VAD gate: `crates/audio/src/vad.rs` (`GateConfig`).
   - Sidecar flags/supervision: `crates/llm/src/server.rs` (`ServerConfig`); request body: `client.rs`.
   - Prompts: `crates/translation/src/translator.rs`, `crates/copilot/src/worker.rs` — always
     `quote_transcript` + `UNTRUSTED_DATA_RULE`, bounded `max_tokens`, `clean_completion`.
   - When to suggest: `crates/copilot/src/{detector,policy}.rs`.
4. **New model.** Add a `CatalogEntry` in `crates/models/src/catalog.rs`: HTTPS URL, exact `size_bytes`, SHA-256 from
   Hugging Face LFS metadata or GitHub release digest, license (permissive only; see
   `docs/research/model-licenses.md`). Keep one `is_default` per kind per platform (unit-tested). Download and
   verify: `parlance-cli download <id>`.
5. **Tests.** Update scripted-recognizer tests in `streaming.rs`, prompt tests in `translator.rs`/`prompt.rs`,
   worker tests with fake models. Run the real-model fixture:
   `PARLANCE_MODELS_DIR=<dir> cargo test -p parlance-asr --release --test whisper_fixture -- --nocapture`.
6. **Re-measure** with the same commands; compare WER, RTF, first-partial/final p50/p95, TTFT; confirm ASR latency
   did not regress while the LLM is busy (run a session with translation on in `pnpm dev`).
7. **Docs.** Record numbers in `docs/benchmarks/`, update the ADR (or supersede it) and `docs/roadmap.md`.
