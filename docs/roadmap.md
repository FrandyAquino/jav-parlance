# Roadmap and progress

Source of truth for what works. "Verified" means exercised on real hardware, not only unit-tested.
Last updated: 2026-11-25.

## Status at a glance

| Area | State | Evidence |
| --- | --- | --- |
| Workspace, contracts, docs, CI | Done | `cargo test --workspace`, clippy `-D warnings`, frontend typecheck/lint/test |
| Windows capture (WASAPI loopback, process loopback, mic) | Verified (loopback, enumeration); process loopback and mic enumerated, not yet transcribed live | `parlance-cli sources`, live session in the desktop UI |
| VAD + streaming Whisper | Verified | JFK fixture WER 0.0; live system audio in the UI |
| Desktop UI vertical slice | Verified | Screenshots of live transcript, translation and copilot |
| Translation (Qwen3.5 via llama-server) | Verified | Live session; 0.3 s warm per sentence on GTX 1650 |
| Copilot (question detection + bilingual reply) | Verified | Live session with `meeting_question_en.wav` |
| Model manager (pinned SHA-256) | Verified | Downloaded and verified 4 artifacts |
| Meeting history (SQLite, opt-in) | Unit/integration tested | `crates/storage` tests; not yet exercised in the UI |
| Browser tab capture (MV3 + native host + bridge) | Tested in isolation | Extension 31 tests, host 18 tests, bridge TCP tests; never loaded in a real browser |
| Linux capture (pactl/parec) | Compiles for Linux | Cross `cargo check`; never run |
| macOS capture (Core Audio taps) | Not started | — |

## Milestones

### M0 — Foundation ✅
- [x] Cargo + pnpm workspace, crate boundaries, lints, rustfmt/clippy config
- [x] Generated TypeScript contracts (ts-rs), IPC error model
- [x] ADRs 0001–0010, research notes, CLAUDE.md, `.claude/` rules/agents/skills
- [x] Windows GNU toolchain support (vendored `whisper-rs-sys`, static CRT)

### M1 — Windows vertical slice ✅
- [x] WASAPI endpoint loopback with silence filling and default-device reconnection
- [x] Lock-free ring, downmix, FFT resampling, level meter, multi-source mixer
- [x] Silero VAD (ggml) + hysteresis gate; energy fallback
- [x] Streaming Whisper with LocalAgreement-2, stable ids, revisions, language lock
- [x] Session state machine, DSP/ASR threads, auto-stop on source end
- [x] Desktop UI: glass window, source picker, meter, live transcript, settings, model downloads
- [x] `parlance-cli` sources/listen/bench/download/llm

### M2 — Translation and copilot ✅
- [x] llama-server sidecar: loopback + API key, health, warm-up, restart/backoff, CPU fallback, Windows job object
- [x] Translator port + LLM translator with prompt isolation; streaming translations
- [x] Copilot: multilingual question detection, rolling context, cancellable bilingual replies
- [x] Opt-in history (SQLite) with copy/delete in Settings

### M3 — Capture breadth 🚧
- [x] Browser extension (MV3, tabCapture, offscreen, AudioWorklet) and native host with installer
- [x] Desktop bridge listener (`tab:*` sources) with token auth
- [x] Linux adapter (PulseAudio/PipeWire via pactl/parec, per-app via `--monitor-stream`)
- [ ] Load the extension in Chrome/Edge and run an end-to-end tab session
- [ ] Run the Linux adapter on PipeWire and PulseAudio hosts
- [ ] macOS adapter: Core Audio process taps (14.2+), ScreenCaptureKit fallback, TCC prompts
- [ ] Live-verify process loopback and microphone mixing on Windows

### M4 — Quality and performance
- [ ] Cache loaded Whisper contexts across sessions (start latency for large models)
- [ ] Translate stabilized partials (debounced) for lower perceived latency
- [ ] GPU Whisper builds (CUDA/Vulkan/Metal features) and runtime selection
- [ ] CTranslate2 translator adapter (MADLAD-400 / OPUS-MT) as an option
- [ ] Global shortcuts, window position persistence, tray icon
- [ ] Per-speaker separation (loopback vs microphone lanes) in the transcript
- [ ] E2E tests driving the packaged app (tauri-driver/WebDriver)
- [ ] Code signing and installers per OS

## Known issues (from review, 2026-11-25)

- [ ] On app exit an active session is not stopped (history entry keeps no end time); on Linux/macOS
      `llama-server` may outlive a crashed app (no parent-death signal; Windows uses a job object).
- [ ] Bridge: unlimited pre-auth connections (one thread each), accept thread never exits, no explicit ACL on
      the Windows discovery file, frame `sequence` unchecked.
- [ ] Bridge relay locks a mutex per frame and WASAPI buffers may grow: capture paths are not strictly
      allocation/lock-free yet.
- [ ] History commands run SQLite synchronously on the command thread; copilot/history queues are unbounded.
- [ ] Catalog lacks `llama-server` for macOS x64 and ARM Windows/Linux.
- [ ] UI: microphone cannot be the primary source; shortcuts show "Ctrl" on macOS; cancelling a download shows
      an error toast; settings write errors map to `InvalidRequest`.

## Known limitations

- Copilot reply quality is bounded by the 2B default model; 4B is noticeably better with a GPU.
- Whisper occasionally drops question marks (notably in Spanish); the detector compensates with request phrases.
- History titles are timestamps; renaming is not implemented.
- The desktop crate has no Rust unit tests (`test = false`) because Windows test executables lack the
  Common Controls manifest Tauri needs; its logic is thin wiring covered through the core crates.
