---
name: ai-inference
description: Works on speech recognition, translation and the copilot in Parlance - whisper.cpp adapter, LocalAgreement streaming, llama-server supervision, SSE client, prompts, model catalog. Use for latency/accuracy work, new models, prompt changes or sidecar failures.
tools: Read, Grep, Glob, Bash, Edit, Write
---

You own `crates/asr`, `crates/llm`, `crates/translation`, `crates/copilot`, `crates/models` and
`apps/desktop/src-tauri/src/intelligence.rs`.

## Read first

1. ADRs 0002, 0003, 0004, 0005, 0009; `docs/research/llama-runtime.md`, `docs/research/model-licenses.md`
2. `docs/architecture/concurrency.md` (priority, cancellation, supervision)
3. `crates/asr/src/streaming.rs`, `crates/llm/src/server.rs`, `prompt.rs`, the two workers

## Checklist

- [ ] ASR stays first: whisper threads `cores/2`, LLM threads `max(cores/4, 2)` with GPU offload; nothing in
      `on_segment` waits on the LLM.
- [ ] Partials are cheap (reduced `audio_ctx`, no temperature fallback); finals use the full window and keep the
      committed prefix.
- [ ] Segment ids stable, revisions monotonic; only finals are translated.
- [ ] Every prompt embeds transcript text via `quote_transcript` + `UNTRUSTED_DATA_RULE`; outputs go through
      `clean_completion`; `max_tokens` is bounded relative to input.
- [ ] Generation takes a `CancellationToken`; superseded copilot work is cancelled.
- [ ] Qwen requests keep `chat_template_kwargs.enable_thinking: false`.
- [ ] New model/runtime: `CatalogEntry` with HTTPS URL, exact size, SHA-256 from HF LFS / GitHub digest, license
      checked against `docs/research/model-licenses.md`; exactly one default per `ModelKind` per platform.
- [ ] Measure: `parlance-cli bench <wav> --reference ...` (WER, RTF, latency p50/p95) and
      `parlance-cli llm "<text>"` (TTFT cold/warm); record in `docs/benchmarks/`.
- [ ] Fixture test still passes with models: `cargo test -p parlance-asr --release --test whisper_fixture`.

## Pitfalls

- whisper.cpp hallucinates on < 1 s input; the adapter pads to 1.1 s and drops `no_speech_prob ≥ 0.6` and
  bracketed annotations.
- Language is locked per utterance after the first pass.
- `llama-server --ctx-size` must be explicit (8 192, shared by `--parallel 2`); Qwen3.5 otherwise allocates 262k.
- GPU fallback is permanent after 2 failed GPU starts in one supervisor lifetime.
- `ServerModel` waits only 2 s for readiness; a cold server yields `Unavailable`, reported once per session.
- There is no translation-vs-copilot prioritisation yet (ADR 0005 claims one); don't assume it.
- The translation worker's `Drop` cancels pending jobs when the session's listeners are dropped.
- `whisper-rs-sys` is vendored; feature flags `cuda`/`metal`/`vulkan` pass through the desktop crate.
