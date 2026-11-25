# Reference Projects

Verified 2026-11-24 against the repos' READMEs and source trees. "Unverified" means I could not confirm it from a primary source.

## Summary

| Project | Stack | License | Streaming policy | Local LLM |
|---|---|---|---|---|
| Meetily | Tauri 2.6 + Rust + Next.js | MIT | VAD-gated segments (Silero) → Whisper/Parakeet | External (Ollama, OpenAI-compatible) |
| Hyprnote → anarlog / char | Tauri 2.11 + Rust + React/TS | MIT (community), commercial (`enterprise/`) | Chunked; detailed policy unverified | External (LM Studio, Ollama, any OpenAI-compatible server) |
| Meet2Notes | Python 3.11 + FastAPI + web UI | MIT | Separate "live" and "final" engines | llama.cpp (managed), Ollama, LiteLLM |
| ufal/whisper_streaming | Python | MIT | LocalAgreement-n | n/a |
| ufal/SimulStreaming | Python (torch) | MIT | AlignAtt (+ LocalAgreement for the LLM step) | EuroLLM-9B via CTranslate2 |

## Meetily (Zackriya-Solutions/meetily, formerly meeting-minutes)

Source: https://github.com/Zackriya-Solutions/meetily (the old URL redirects), `frontend/src-tauri/Cargo.toml`, `frontend/src-tauri/src/audio/**`.

- **Stack:** Tauri 2.6.2, Rust backend, Next.js UI, SQLite via `sqlx`, version 0.5.0.
- **Audio capture:**
  - Windows: `cpal` 0.15 WASAPI host. Output devices are opened as input streams, and cpal sets `AUDCLNT_STREAMFLAGS_LOOPBACK` for render endpoints (`cpal/src/host/wasapi/device.rs`). This is endpoint loopback, not process loopback.
  - macOS: two backends (`capture/backend_config.rs`). **ScreenCaptureKit** is the default. **Core Audio** uses `cidre` `ca::TapDesc::with_mono_global_tap_excluding_processes([])` plus a private aggregate device (`capture/core_audio.rs`).
  - Linux: `libpulse-binding` 2.30, recording from sink `.monitor` sources (`capture/pulse_linux.rs`).
- **DSP:** `nnnoiseless` (RNNoise), `ebur128` loudness normalization, `rubato` resampling, plus a ducking mixer and limiter (`audio_v2/`).
- **ASR:** `whisper-rs` 0.13 (Metal/CoreML on macOS, Vulkan build on Windows) and Parakeet-TDT-0.6B-v3 ONNX through `ort` 2.0-rc.10.
- **VAD:** `silero_rs` (git), 30 ms frames at 16 kHz, thresholds 0.50/0.35, `min_speech_time` 250 ms. Redemption is 500 ms live and 2000 ms in batch (`audio/vad.rs`). The code comments note that earlier, tighter settings fragmented speech into 40 ms pieces that Whisper rejected.
- **LLM:** no llama.cpp crate. Summaries go to Ollama, Claude, Groq, OpenRouter, OpenAI or a custom OpenAI-compatible endpoint.
- **Lessons:**
  - Adopt: a dual macOS backend (Core Audio tap first, ScreenCaptureKit as fallback), VAD-gated segments with a redemption window, and RNNoise before ASR.
  - Avoid: Windows endpoint loopback when you need per-app isolation, because it also captures notification sounds and music.

## Hyprnote → anarlog / char (fastrepl)

Source: https://github.com/fastrepl/hyprnote (now `fastrepl/anarlog`), root `Cargo.toml`, `crates/audio-actual`, `crates/vad`, `crates/local-llm-core`.

- **Naming:** the team is building a new app called **char**. The open-source notetaker continues as **anarlog** ("anarlog is not being retired").
- **Stack:** Tauri 2.11.5, React/TS UI, a large Rust workspace (`crates/*`), and the `ractor` actor framework (`crates/supervisor`). It runs on macOS, Windows and Linux, with iOS and Android in beta.
- **Audio capture** (`crates/audio-actual/src/speaker/*`):
  - Windows: `wasapi` 0.23, endpoint loopback (`open_endpoint_loopback_client`), with automatic switching when the default endpoint changes.
  - macOS: `cidre` 0.15 Core Audio process tap: a mono global tap that excludes no processes, plus an aggregate device. Permissions go through a dedicated `crates/tcc` crate (swift-rs).
  - Linux: both `pipewire` 0.9.2 and `libpulse-binding` 2.30.1.
  - Browser: `crates/chrome-native-host`, a Chrome native-messaging host for meeting tabs.
  - Echo: a separate `crates/aec` (ONNX acoustic echo cancellation) and `native_meeting/echo.rs` remove speaker bleed from the mic.
- **ASR:** `whisper-local` wraps `whisper-rs` (cuda, metal, coreml, hipblas features). Also Soniqo, Apple SpeechAnalyzer, and Soniox in the cloud.
- **VAD:** `crates/vad` with two features: `earshot` (pure-Rust VAD, 1.2.2 on crates.io) and `silero-onnx`.
- **LLM:** `local-llm-core/src/server.rs` is a stub that returns "Local LLM is not supported on this platform". Local inference goes through external OpenAI-compatible servers (LM Studio, Ollama) instead of an embedded llama.cpp. Whether it earlier used llama-cpp-2 in-process, and why that changed, is unverified.
- **Lessons:**
  - Adopt: an actor or supervisor model for the pipelines, endpoint-change recovery, AEC between the mic and system audio, and a Chrome native host.
  - Avoid: tying the LLM to a single in-process runtime. A model server behind an HTTP boundary is the pattern they settled on.

## Meet2Notes (estebanstifli/Meet2Notes)

Found at https://github.com/estebanstifli/Meet2Notes (alpha, v0.9.0, about 47 stars). There is also a Gitee mirror.

- **Stack:** Python 3.11+, FastAPI, server-rendered UI at `127.0.0.1:8765`, Pinokio installer.
- **Capture:**
  - Windows: WASAPI mic plus WASAPI loopback.
  - macOS: CoreAudio input. System audio needs a virtual or tap-backed device.
  - Linux: PipeWire, Pulse or ALSA, using monitor inputs.
- **ASR:** Faster-Whisper (tiny → large-v3/turbo), with separately selectable **live** and **final** engines. Optional NVIDIA Nemotron streaming ASR and Parakeet-TDT, plus experimental VibeVoice ASR.
- **Diarization:** Nemotron, Sherpa-ONNX, pyannote Community-1, or `diarize`.
- **LLM:** managed llama.cpp with Qwen3, LFM2.5 and custom GGUF files, plus Ollama and LiteLLM.
- **Lessons:**
  - Adopt: two-pass design (a fast live model, then a high-quality final re-transcription) and a pluggable engine interface.
  - Avoid: a Python runtime. Packaging and startup cost conflict with Parlance's single-binary goal.

## ufal/whisper_streaming

Source: https://github.com/ufal/whisper_streaming. Paper: Macháček et al., 2023, "Turning Whisper into Real-Time Transcription System".

- **Status:** the README says it is "becoming outdated in 2025" and being replaced by SimulStreaming. License: MIT.
- **LocalAgreement-n:** after each new audio chunk, re-transcribe the buffer. A prefix is **confirmed** when *n* consecutive updates agree on it (n=2 in practice). Confirmed text is passed to Whisper as the prompt for later passes.
- **Buffer trimming:** `--buffer_trimming {segment,sentence}`, default `segment` ("performs better according to our tests"). `--buffer_trimming_sec` sets the length threshold that triggers trimming at the last confirmed segment or sentence end.
- **`--min-chunk-size`:** the minimum audio, in seconds, collected before each processing step.
- **VAD/VAC:** `--vac` (Voice Activity Controller, recommended, needs torch/Silero) and `--vad`.
- **Latency:** "3.3 seconds latency on unsegmented long-form speech" (paper abstract). That figure is for large models on a GPU.
- **For Parlance:** port LocalAgreement-2 to Rust on top of whisper-rs. It is simple (prefix comparison over word timestamps) and backend-agnostic. Trim the buffer at segment boundaries and cap it at about 15–30 s so Whisper never sees more than 30 s of audio.

## ufal/SimulStreaming

Source: https://github.com/ufal/SimulStreaming (IWSLT 2025 system).

- **License:** MIT. The README asks for feedback on "licensing models", so check the LICENSE file at the pinned commit before reusing code.
- **AlignAtt:** uses Whisper's encoder-decoder cross-attention to find which audio frame each new token attends to. Decoding stops when attention reaches within `--frame_threshold` frames of the buffer end (1 frame = 20 ms). Larger `--frame_threshold` and `--min-chunk-size` give higher quality and higher latency.
- **Speed:** "~5-times faster" than WhisperStreaming. The README gives no absolute latency figures (unverified).
- **Models:** optimized for large-v3 (1.5B) with EuroLLM-9B on one or two GPUs. Smaller distilled models are possible. The CIF truncation model does not exist for large-v3. Backend: torch only.
- **Translation:** Whisper ASR → EuroLLM-9B-Instruct (CTranslate2) cascade, with LocalAgreement on the LLM output, covering 35 target languages.
- **For Parlance:**
  - AlignAtt needs per-token cross-attention. whisper.cpp exposes DTW token timestamps, but there is no AlignAtt-style early-stop hook (unverified), so a port means patching whisper.cpp.
  - Start with LocalAgreement-2. Treat AlignAtt as a later optimization.
  - Reuse the cascade idea: confirm source text first, translate only confirmed text, and apply LocalAgreement to the translation as well.

## Cross-cutting recommendations

1. Capture the mic and system/peer audio as **separate tracks** (all three apps do this). That gives cheap "me vs. them" diarization and lets AEC run.
2. VAD before ASR. Silero (via whisper.cpp's built-in VAD or ONNX) is the common choice. Use about a 500 ms redemption window when live.
3. Run a live pass with a small model and LocalAgreement-2, then an optional final pass with a larger model over the stored audio.
4. Keep the LLM behind an OpenAI-compatible HTTP boundary (Hyprnote/anarlog and Meetily both converge on this).
