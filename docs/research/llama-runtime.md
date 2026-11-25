# LLM Runtime: Embedded `llama-cpp-2` vs `llama-server` Sidecar

Verified 2026-11-24 against the GitHub releases API, the llama.cpp server README, crates.io, and the llama-cpp-rs issue tracker.

## Current state

- **llama.cpp v1.0.0** was released on 2026-11-13. It removed 35 deprecated `llama.h` functions, updated **ggml to v1.0.0**, hardened `llama-server` (SSRF, CORS, DNS rebinding), and added `/v1/audio/speech`. Rolling builds continue as `bNNNNN` tags (b12686 on 2026-11-24).
- **`llama-cpp-2` / `llama-cpp-sys-2` 0.1.161** (2026-11-03), MIT OR Apache-2.0. Features: `cuda`, `vulkan`, `metal`, `rocm`, `openmp`, `mtmd`, `dynamic-link`, `dynamic-backends`, **`system-ggml`**, `system-ggml-static`.
- **`whisper-rs` 0.16.0 / `whisper-rs-sys` 0.15.0** (2026-03-12) bundles **whisper.cpp 1.8.3** with its own, older ggml. The crates.io release has no `system-ggml` feature.
- **Licenses:** llama.cpp MIT ("Copyright (c) 2023-2026 The ggml authors"). Prebuilt CUDA zips bundle NVIDIA cudart, which is redistributable under the CUDA EULA (review it before shipping).

## The ggml duplicate-symbol problem (verified)

Both `-sys` crates statically compile their **own copy of ggml**. Linking both into one binary gives:

- **MSVC:** `LNK2005` duplicate symbols (e.g. `ggml_backend_buft_name already defined`) — https://users.rust-lang.org/t/bindings-linking-conflicts/116919.
- **Runtime crashes** when the linker is forced to accept duplicates. llama-cpp-rs #484 ("Runtime exception when using along whisper-rs", still open) reports a crash in `ctx.decode` once whisper-rs is linked.
- **Silent ABI mismatch.** PR #1091 notes that `GGML_TYPE_COUNT` is 40 in whisper-rs-sys 0.15 and 42 in recent llama.cpp, so merging the copies "can corrupt the heap".
- **Status of fixes:**
  - #921 (an objcopy `llm_` symbol-prefix `compat` feature) was closed unmerged.
  - #1091 (merged 2026-08-04) exports `DEP_LLAMA_GGML_CMAKE_DIR` for `system-ggml`. Its author notes that **whisper-rs-sys does not consume it yet**.
- **Upstream C++ precedent:** whisper.cpp #1887 (Xcode duplicate ggml). Debian solved it by packaging ggml as a separate shared library.

**Conclusion:** a single process holding the current whisper-rs and llama-cpp-2 is not safe today unless you build one shared ggml yourself and patch whisper-rs-sys to use it. The ggml v1.0 bump makes version skew between the two copies worse.

## Prebuilt `llama-server` release assets (b12686 naming)

| Platform | Asset |
|---|---|
| Windows x64 CPU | `llama-bNNNNN-bin-win-cpu-x64.zip` |
| Windows x64 CUDA 12.4 | `llama-bNNNNN-bin-win-cuda-12.4-x64.zip` + `cudart-llama-bin-win-cuda-12.4-x64.zip` |
| Windows x64 CUDA 13.5 | `llama-bNNNNN-bin-win-cuda-13.5-x64.zip` + `cudart-llama-bin-win-cuda-13.5-x64.zip` |
| Windows x64 Vulkan | `llama-bNNNNN-bin-win-vulkan-x64.zip` |
| Windows other | `win-rocm-10.0-x64`, `win-sycl-x64`, `win-openvino-2026.4.1-x64`, `win-cpu-arm64`, `win-vulkan-arm64`, `win-opencl-adreno-arm64` |
| macOS Apple Silicon (Metal) | `llama-bNNNNN-bin-macos-arm64.tar.gz` |
| macOS Intel | `llama-bNNNNN-bin-macos-x64.tar.gz` |
| Linux x64 CPU / Vulkan | `llama-bNNNNN-bin-ubuntu-x64.tar.gz` / `…-ubuntu-vulkan-x64.tar.gz` |
| Linux x64 CUDA | `…-ubuntu-cuda-12.8-x64.tar.gz`, `…-ubuntu-cuda-13.5-x64.tar.gz` (+ `cudart-…`) |
| Linux other | `ubuntu-rocm-10.0-x64`, `ubuntu-sycl-fp16/fp32-x64`, `ubuntu-openvino-…`, `ubuntu-arm64`, `ubuntu-vulkan-arm64` |
| Apple frameworks | `llama-bNNNNN-xcframework.zip` |

- **Archive names:** the Windows cudart zips carry no build number. The `.tar.gz` Linux builds link against system glibc, so test them on the oldest supported distro.
- **GPU support:** CUDA 13.x dropped Maxwell, Pascal and Volta, while Turing (GTX 1650) remains supported. Vulkan is the most portable GPU build on Windows and Linux. It ships no NVIDIA runtime, so the installer stays small.

## `llama-server` flags (from `tools/server/README.md`)

| Flag | Default | Parlance setting |
|---|---|---|
| `--host` | `127.0.0.1` | keep `127.0.0.1` (never `0.0.0.0`) |
| `--port` | **9931** (changed from the historical 8080) | pick a free ephemeral port and pass it explicitly |
| `-m` / `-hf repo[:quant]` | — | `-m <path>` (models are downloaded by Parlance and hash-checked) |
| `-ngl, --gpu-layers` | `auto` (`N`/`auto`/`all`) | `auto`; `-fit on` (default) fits unset args to free VRAM (`-fitt` margin 1024 MiB) |
| `-c, --ctx-size` | `0` (= model max) | **set explicitly** (8192), otherwise Qwen3.5 allocates 262k context |
| `-t, --threads` | `-1` | physical cores − threads reserved for whisper |
| `-np, --parallel` | `-1` (auto) | `1`–`2` (copilot plus a background summary) |
| `-fa, --flash-attn` | `auto` | default |
| `--jinja` | on | keep on (Qwen templates) |
| `--reasoning-budget` | `-1` | `0` for low-latency copilot replies |
| `--chat-template-kwargs` | — | `{"enable_thinking":false}` for Qwen3.5 |
| `--no-mmproj` | auto-loads with `-hf` | set for text-only use |
| `--api-key` | none | random per-launch token, to stop other local processes from using the server |
| `--no-webui` | UI on | set |
| `--sleep-idle-seconds` | `-1` | optional, to free VRAM when idle |

**Endpoints:**
- `GET /health`: returns 503 "Loading model", then 200 `{"status":"ok"}`. It needs no API key. Use it as the readiness probe.
- `POST /v1/chat/completions`: with `"stream": true` it returns OpenAI-format SSE chunks plus `timings` and `usage`.
- `GET /v1/models`, `GET /slots`, and `--models-dir` router mode for multiple models (`--models-max`, default 4).

## Comparison

| Aspect | Embedded `llama-cpp-2` | `llama-server` sidecar |
|---|---|---|
| ggml conflict with whisper-rs | **Yes**: LNK2005, or heap corruption with forced linking. Needs a shared-ggml build plus patched whisper-rs-sys. | **None**: separate process, separate ggml. |
| Crash isolation | An OOM or driver fault takes down the app and the live transcript. | The sidecar dies, gets restarted, and ASR keeps running. |
| GPU backend choice | Fixed at compile time per build matrix (CUDA, Vulkan, Metal). | Pick the zip at install time or on first run (CPU/Vulkan/CUDA), detected per machine. |
| Upgrades and new models | Wait for the llama-cpp-2 bump (v1.0 removed APIs, so expect churn). | Swap the binary. New architectures (Qwen3.5 hybrid DeltaNet) arrive the same day. |
| Streaming API | Own token loop and sampler code. | OpenAI SSE out of the box. The same client also targets Ollama or LM Studio. |
| Encoder-decoder (MADLAD/T5) | **Works** (`encode` then `decode`). | **Not supported** (#26565 closed as not planned, PR #17956 open). |
| Latency overhead | none | about 1 ms of localhost HTTP per request; negligible against token time |
| Packaging | One binary, but large CUDA link and long builds. | +15–40 MB per backend zip (unverified per asset). Needs code-signing and notarization on macOS and a Tauri `externalBin` sidecar. |
| Security | n/a | Must bind to loopback, use an API key, and pin the binary hash. |

## Recommendation

1. **Copilot LLM:** ship the **official prebuilt `llama-server` as a supervised sidecar**.
   - Use Tauri `externalBin` or a downloaded backend zip verified by SHA-256.
   - Launch with `--host 127.0.0.1 --port <free> --api-key <random> -m <gguf> -c 8192 -ngl auto -np 1 --no-webui --reasoning-budget 0`.
   - A supervisor task polls `/health`, restarts with backoff, and kills the child on app exit (Windows: a Job Object with `KILL_ON_JOB_CLOSE`; Unix: a process group).
   - The client is a thin OpenAI-compatible SSE client, so users can also point Parlance at Ollama or LM Studio.
2. **ASR:** keep `whisper-rs` **in-process**. It is the only ggml copy in the main binary.
3. **MADLAD translation:** do not use llama.cpp for it. Use **CTranslate2 (`ct2rs`)** in-process, or a second small sidecar if ct2's native dependencies (oneDNN, cuBLAS) cause packaging trouble. Alternatively, translate with the Qwen sidecar for major languages.
4. **Revisit embedding** only if whisper-rs-sys gains `system-ggml` support and both crates can share one ggml build of the same version.

The prompt's assumption holds: the sidecar is the safest way to avoid duplicate ggml symbols. It is also the only route that can never hit the ABI mismatch, because the two ggml copies never share an address space.

## Sources

- https://github.com/ggml-org/llama.cpp/releases (v1.0.0, b12686), https://huggingface.co/blog/ggml-org/llama-cpp-v1
- https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md, https://github.com/ggml-org/llama.cpp/blob/master/LICENSE
- https://crates.io/crates/llama-cpp-2, https://crates.io/crates/whisper-rs, https://docs.rs/crate/whisper-rs-sys/0.15.0
- https://github.com/utilityai/llama-cpp-rs/issues/484, https://github.com/utilityai/llama-cpp-rs/pull/1091, https://github.com/utilityai/llama-cpp-rs/pull/921, https://github.com/utilityai/llama-cpp-rs/issues/884
- https://users.rust-lang.org/t/bindings-linking-conflicts/116919, https://github.com/ggml-org/whisper.cpp/issues/1887, https://lists.debian.org/debian-ai/2025/03/msg00025.html
- https://github.com/ggml-org/llama.cpp/issues/26565, https://github.com/ggml-org/llama.cpp/pull/17956
