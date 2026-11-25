# Architecture decision records

Short records of decisions that shape the codebase: context, decision, consequences.
Supersede a record by adding a new one and marking the old one `Superseded by NNNN`.

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-tauri-rust-react.md) | Tauri v2 shell, Rust core, React UI | Accepted |
| [0002](0002-whisper-localagreement.md) | whisper.cpp ASR with LocalAgreement-2 streaming | Accepted |
| [0003](0003-silero-via-whisper-cpp.md) | Silero VAD through whisper.cpp's ggml port | Accepted |
| [0004](0004-llama-server-sidecar.md) | LLM runtime as a supervised `llama-server` sidecar | Accepted |
| [0005](0005-translation-shares-llm.md) | Translation shares the copilot LLM | Accepted |
| [0006](0006-vendored-whisper-rs-sys.md) | Vendored `whisper-rs-sys` for MinGW builds | Accepted |
| [0007](0007-generated-contracts.md) | IPC contracts generated from Rust with ts-rs | Accepted |
| [0008](0008-wasapi-capture.md) | WASAPI capture with fixed format and silence filling | Accepted |
| [0009](0009-model-integrity.md) | Pinned SHA-256 and staged model downloads | Accepted |
| [0010](0010-browser-tab-bridge.md) | Browser tab audio via MV3 extension + native messaging | Accepted |
| [0011](0011-macos-capture.md) | Core Audio taps, ScreenCaptureKit fallback and native Mac microphones | Accepted |
