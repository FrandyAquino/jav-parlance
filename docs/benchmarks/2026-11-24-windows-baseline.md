# Baseline — Windows 11, CPU Whisper, Vulkan LLM (2026-11-24)

Machine: Windows 11 Home (build 26300), Intel laptop CPU with AVX2/AVX-VNNI, 16 GB RAM, NVIDIA GTX 1650 4 GB
(Vulkan) + Intel UHD. Release builds, `x86_64-pc-windows-gnu`. whisper.cpp 1.8.3 (CPU only), llama.cpp b12686
(Vulkan, single GPU).

## Speech recognition (`parlance-cli bench`)

Replay at real time (1×) through the full pipeline: replay capture → resample → Silero VAD → streaming Whisper.
Latencies are measured from when the audio position is played.

| Fixture | Model | WER | First partial p50 | Final after speech end p50 / p95 | Peak RSS | CPU avg |
| --- | --- | --- | --- | --- | --- | --- |
| `jfk.wav` (11 s, EN) | Whisper base Q8 | 0.000 | 1 708 ms | 648 / 761 ms | 293 MiB | 2.4 cores |
| `meeting_question_en.wav` (TTS) | Whisper base Q8 | 0.000 | 1 742 ms | 691 / 738 ms | 296 MiB | 2.8 cores |
| `meeting_question_es.wav` (TTS) | Whisper base Q8 | 0.080 | 1 730 ms | 700 / 714 ms | 294 MiB | 2.8 cores |

Throughput (replay as fast as possible, `--speed 0`): JFK processing RTF **0.41** (2.4× faster than real time),
5 cores average.

Notes:
- First-partial latency is dominated by policy, not compute: 700 ms minimum utterance + 1 s decode step.
  Lowering `StreamingConfig::step_ms` trades CPU for earlier partials.
- Spanish errors: one merged word ("senviarme") and a dropped question mark.

## Language model (`parlance-cli llm`)

Qwen3.5 2B Q4_K_M through `llama-server`, translating one ~12-word sentence.

| Backend | Server ready | First request (after warm-up) | Warm request | Warm first token |
| --- | --- | --- | --- | --- |
| Vulkan (GTX 1650, all layers) | 2.2 s | 555 ms | 298 ms | 45 ms |
| CPU (4 threads) | 1.8 s | 1 538 ms | 485 ms | 111 ms |

Before the supervisor's warm-up request existed, the first Vulkan request took 4.4 s (shader compilation).

## Not yet measured

- VRAM use (needs NVML/DXGI query), Whisper small/turbo models, GPU Whisper builds, long-meeting soak
  (memory over 1 h), WER on a standard multilingual corpus (FLEURS/Common Voice subsets).

Reproduce:

```sh
cargo build --release -p parlance-cli
target/release/parlance-cli bench fixtures/audio/jfk.wav --reference "And so my fellow Americans, ask not what your country can do for you, ask what you can do for your country."
target/release/parlance-cli llm "Could you send me the quarterly numbers before Friday's meeting?" --to es
```
