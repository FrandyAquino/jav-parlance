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
| macOS capture and desktop | Verified on Apple Silicon | System/app/mic probes, live ASR/translation, universal app, Cmd+Q history flush; see [Mac evidence](benchmarks/2026-11-25-macos.md) |
| macOS ScreenCaptureKit fallback (13.3–14.1) | Implemented, runtime unverified | Availability-guarded native adapter; needs older OS acceptance |

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
- [x] Streaming Whisper with LocalAgreement-2, stable ids, revisions, revisable automatic language detection
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
- [x] macOS adapter: Core Audio process taps (14.2+), ScreenCaptureKit fallback, TCC purpose strings and mic authorization
- [ ] Validate fallback on macOS 13.3–14.1 and audio on an Intel Mac
- [ ] Live-verify process loopback and microphone mixing on Windows

### M4 — Quality and performance
- [ ] Cache loaded Whisper contexts across sessions (start latency for large models)
- [x] Real-time translation of partial hypotheses (latest-wins), 500 ms ASR step
- [x] Metal Whisper enabled in normal Mac builds; portable CPU build flags
- [ ] Validate CUDA/Vulkan Whisper builds and GPU selection on Windows/Linux
- [ ] CTranslate2 translator adapter (MADLAD-400 / OPUS-MT) as an option
- [ ] Global shortcuts, window position persistence, tray icon
- [ ] Per-speaker separation (loopback vs microphone lanes) in the transcript
- [ ] E2E tests driving the packaged app (tauri-driver/WebDriver)
- [x] Installer build workflow for Windows, Linux and universal macOS
- [ ] Developer ID signing/notarization and signed release acceptance per OS

## Known issues (from review, 2026-11-25)

- [x] Graceful exit stops capture, awaits the LLM and flushes history; Unix lifetime-pipe guardian covers app crashes.
- [ ] Bridge: unlimited pre-auth connections (one thread each), accept thread never exits, no explicit ACL on
      the Windows discovery file, frame `sequence` unchecked.
- [ ] Bridge relay locks a mutex per frame and WASAPI buffers may grow: capture paths are not strictly
      allocation/lock-free yet.
- [ ] History commands run SQLite synchronously on the command thread; copilot/history queues are unbounded.
- [x] Pinned official runtimes for macOS x64 and ARM Windows/Linux added.
- [x] Primary microphone selection, Cmd labels on Mac, typed cancellation without an error toast.
- [ ] Settings write errors map to `InvalidRequest`.

## Known limitations

- Copilot reply quality is bounded by the 2B default model; 4B is noticeably better with a GPU.
- Whisper occasionally drops question marks (notably in Spanish); the detector compensates with request phrases.
- History titles are timestamps; renaming is not implemented.
- The desktop crate has no Rust unit tests (`test = false`) because Windows test executables lack the
  Common Controls manifest Tauri needs; its logic is thin wiring covered through the core crates.

## Spanish audio and repeated translation correction — 2026-11-25

- Auto re-evaluates the language as more audio arrives; only an explicit source selection forces it.
- Translation cancels and rejects generated short-phrase repetition beyond repetitions already present in
  the source. Rejected provisional output is cleared and never added to translation context.
- Same-text partials with a corrected language are processed again, allowing Spanish-to-Spanish passthrough.
- Music/vocal recognition remains less reliable than speech; the exact user song is not part of the fixtures.

Validation: the real Spanish fixture's 500 ms prefix initially detected Arabic; the complete phrase now
recovers Spanish instead of inheriting that guess. Full Rust workspace: 163 passed plus one ignored subprocess
helper; the final punctuation-preservation regression brings translation tests to 12. Clippy passed. A fake-model
containment probe that previously streamed/accepted 60 repetitions now streams three and returns an error;
this proves containment, not the exact trigger of the user's song. Live Qwen English-to-Spanish translation
and the full Spanish fixture remained functional. The repetition allowance is heuristic and count-based,
not a semantic proof that repeated output matches the source.

CPU timing on this Apple Silicon Mac (Whisper Base Q8, four CPU threads, GPU disabled, warm shared context,
22 growing-prefix decodes of the 11-second JFK fixture): Auto re-detection used compute RTF 0.244–0.257,
with maximum individual decodes of 342–364 ms. This is below the 500 ms update step here, but costs more
than locking the initial language and does not certify older Windows/Linux CPUs. The corrected universal
app was rebuilt, ad-hoc signature verified, installed and reopened with the source picker ready.
