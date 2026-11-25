# 0004 — LLM runtime as a supervised `llama-server` sidecar

**Context.** Translation and the copilot need a local instruct LLM with CPU, CUDA, Vulkan and Metal support.
Linking `llama-cpp-2` next to `whisper-rs` puts two different ggml versions into one binary: duplicate symbols
or, when forced, heap corruption (see `docs/research/llama-runtime.md`). llama.cpp also just reached v1.0 with
breaking API removals.

**Decision.** Run the official prebuilt `llama-server` (pinned build b12686, SHA-256 verified; Vulkan build on
Windows and Linux, Metal on macOS) as a child process bound to `127.0.0.1` on an ephemeral port with a random
API key. A Rust supervisor (`crates/llm`) owns its lifecycle: readiness probe, restart with backoff, and
shutdown with the app. Requests use the OpenAI-compatible `/v1/chat/completions` endpoint with SSE streaming and
can be cancelled.

**Consequences.** Crash and memory isolation from the real-time pipeline, and GPU backends without compiling
CUDA ourselves, at the cost of one more supervised process. The port is loopback-only and authenticated, so
other local users cannot drive the model. Encoder-decoder models (MADLAD) cannot run in `llama-server`; see 0005.
