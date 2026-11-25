---
name: benchmark
description: Runs and records Parlance performance benchmarks - streaming ASR (WER, RTF, latency, RSS, CPU), LLM sidecar latency, and DSP micro-benchmarks. Use before/after performance-sensitive changes or when updating docs/benchmarks.
---

# Benchmark

1. **Prepare.** Release builds only. Close heavy apps; plug in the laptop. Note CPU, GPU, RAM, OS build, toolchain
   (`rustc -vV`), and the git revision.
   ```sh
   cargo run --release -p parlance-cli -- models
   cargo run --release -p parlance-cli -- download whisper-base-q8 silero-vad-5.1.2 qwen3.5-2b-q4 llama-server
   ```
2. **ASR pipeline** (real-time replay gives latency; `--speed 0` gives max-throughput RTF only):
   ```sh
   cargo run --release -p parlance-cli -- bench fixtures/audio/jfk.wav --reference "And so my fellow Americans, ask not what your country can do for you, ask what you can do for your country."
   cargo run --release -p parlance-cli -- bench fixtures/audio/jfk.wav --speed 0
   cargo run --release -p parlance-cli -- bench fixtures/audio/meeting_question_es.wav --model whisper-small-q8
   ```
   Record: model load, processing RTF, first partial p50/p95, final p50/p95, segments, peak RSS, CPU, WER.
3. **LLM sidecar:**
   ```sh
   cargo run --release -p parlance-cli -- llm "Can you send me the report by Friday?" --from en --to es
   cargo run --release -p parlance-cli -- llm "Can you send me the report by Friday?" --from en --to es --cpu
   ```
   Record: server ready time, GPU offload yes/no, cold and warm first-token and total ms.
4. **DSP micro-benchmarks:** `cargo bench -p parlance-audio --bench dsp` (downmix 10 ms stereo; 48k→16k resample).
5. **Repeat** each run 3 times; report the median. Run before and after the change on the same machine.
6. **Record** in `docs/benchmarks/` (one dated section per run: hardware, revision, command, results table, notes)
   and update any figures quoted in ADRs (e.g. ADR 0002) if they change.
7. Treat a regression in ASR latency or RTF as blocking unless the change explicitly trades it for accuracy.
