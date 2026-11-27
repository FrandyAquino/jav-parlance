# Roadmap and progress

## Audit blocker corrections — 2026-11-27

Local corrections add session-scoped translation presentation, causal transcript/history ordering, preservation
of successful ASR output after another decode fails, bounded VAD failure recovery, absolute LLM deadlines and
bounded final draining. Linux child ownership and Chromium offscreen lifecycle now coordinate cleanup with
reconnect/retry. Regression tests use scripted ports, temporary SQLite, loopback HTTP and controlled processes
or Chrome API promises; see [testing](testing.md). These protections do not certify real devices, native
permissions, model accuracy, browser capture or installer CPU compatibility. Verification evidence for this
local implementation is kept separately from the prior audit snapshot.

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
| Adaptive window, local project profiles and written copilot | Implemented locally; native flow exercised on Apple Silicon | [Contextual copilot evidence](benchmarks/2026-11-25-adaptive-copilot.md); Windows/Linux native acceptance remains unrun for this change |

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
- [ ] History commands run SQLite synchronously on the command thread; the history queue is unbounded. Copilot now uses bounded ingress and one active/latest pending request.
- [x] Pinned official runtimes for macOS x64 and ARM Windows/Linux added.
- [x] Primary microphone selection, Cmd labels on Mac, typed cancellation without an error toast.
- [ ] Settings write errors map to `InvalidRequest`.

## Known limitations

- Copilot reply quality is bounded by the 2B default model; 4B is noticeably better with a GPU.
- Whisper occasionally drops question marks (notably in Spanish); the detector compensates with request phrases.
- History titles are timestamps; renaming is not implemented.
- The desktop crate has no Rust unit tests (`test = false`) because Windows test executables lack the
  Common Controls manifest Tauri needs; its logic is thin wiring covered through the core crates.

## Adaptive window and contextual copilot — 2026-11-25

- One window switches between compact subtitles, expanded preparation and a frozen reading view; session and manual drafts survive mode changes.
- Written questions run independently of capture and the automatic-suggestions setting, with explicit reply language and an optional idea.
- Local text profiles have stable identities/revisions, atomic persistence and per-request snapshots; saving profiles never enables meeting history.
- Manual inference takes priority over automatic suggestions; ASR hands off through a bounded queue without waiting for copilot event delivery.
- Stream interruption and token-limit endings are visible failures. Capture shortcuts ignore editable fields and dialogs; Command/Ctrl+Enter submits from either composer field.
- Native macOS checks exercised saved-profile restart, attribution after editing, written replies with audio off, live system capture/translation alongside a manual query, and pinned reading while capture continued.
- The 2B model gave one imprecise general cache-aside explanation and initially confused an undocumented TTL with an absent setting. Prompt wording was tightened and the subsequent native result acknowledged that the TTL was unknown; general technical correctness is not guaranteed by these checks.

Implementation and review remain uncommitted by user request. See [ADR 0012](adr/0012-contextual-copilot-and-adaptive-window.md) and the linked evidence for validation scope.

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

## Music decoding, live translation and typography — 2026-11-25

- Restore whisper.cpp's temperature fallback for fast partial decodes. Disabling it allowed failed greedy
  decodes to escape as repetition loops; a real official “God's Plan” preview reproduced the failure.
- Preserve the visible translation while replacement prefixes stream; completed corrections remain revisable.
- Preempt obsolete partial requests and interleave live progress with ordered final work. Do not restart a
  partial on every same-segment word update, which would starve output during continuous speech.
- Bundle Google Sans Flex locally with its OFL and trademark notices; no font request is sent to Google.
