# 0003 — Silero VAD through whisper.cpp's ggml port

**Context.** Silero VAD is the de facto permissive speech detector, normally run with ONNX Runtime. `ort`
ships prebuilt binaries for MSVC targets only and adds about 20 MB.

**Decision.** Use whisper.cpp's built-in Silero port (`ggml-silero-v5.1.2.bin`, 885 KB) through
`whisper_rs::WhisperVadContext`. whisper.cpp resets the LSTM state on every call, so each 32 ms frame is
scored with 256 ms of preceding context. An energy detector is the fallback until the model is downloaded.

**Consequences.** No ONNX dependency and a single ggml copy in the process. Compatibility of v6.2.0 with the
bundled whisper.cpp 1.8.3 is unverified, so v5.1.2 is pinned. A hysteresis gate (`audio::vad::VadGate`) adds
pre-roll, minimum silence and maximum utterance length on top of raw probabilities.
