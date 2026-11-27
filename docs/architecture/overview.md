# Architecture overview

Parlance is a single-user desktop app. Audio is captured, transcribed, translated and answered on the
device. The only network traffic is the model downloads the user starts.

## System context

```mermaid
flowchart LR
  user([User]) --> ui[React UI<br/>WebView]
  ui <-- "Tauri commands +<br/>SessionEvent channel" --> core[Rust core<br/>parlance-desktop]
  os[(OS audio<br/>WASAPI · Core Audio/SCK · PulseAudio)] --> core
  browser[Chromium + extension] -- native messaging --> host[parlance-native-host]
  host -- "TCP 127.0.0.1 + token" --> core
  core -- "child process, HTTP 127.0.0.1 + API key" --> llama[llama-server sidecar]
  core -- "HTTPS, user-initiated" --> hf[(Hugging Face / GitHub releases)]
  core --> disk[(settings.json · profiles.json · models/ · history.db opt-in)]
```

- Raw audio never crosses into the webview. The UI receives only typed events (ADR 0001).
- `apps/cli` reuses the same core crates headlessly (`sources`, `listen`, `bench`, `models`, `download`, `llm`).

## Crate map

| Crate | Responsibility | Must not |
| --- | --- | --- |
| `crates/domain` | Pure types: `SourceId/Kind`, `TranscriptSegment`, `Translation`, `Suggestion`, `LanguageCode`, `SessionState` machine | Do I/O or depend on other crates |
| `crates/protocol` | Wire contracts (`SessionEvent`, `IpcError`, `AppSettings`, models, native messaging framing). ts-rs export | Contain behaviour beyond (de)serialization and framing |
| `crates/audio` | Lock-free ring (`rtrb`), downmix, i16→f32, resampling (`rubato`), mixer, level meter, VAD port + hysteresis gate | Know about devices or models |
| `crates/capture` | `CaptureBackend` port, WASAPI (Windows), Core Audio/SCK (macOS), pactl/parec (Linux), browser bridge, replay, composite router | Do inference, I/O beyond the device, or block in the device loop |
| `crates/asr` | `SpeechRecognizer` port, LocalAgreement-2, `StreamingTranscriber`, whisper.cpp + Silero adapters (feature `whisper`), WER metric | Know about sessions or the UI |
| `crates/session` | `SessionController` (state machine owner), DSP and ASR threads, `SpeechEngine` / `SegmentListener` / `EventSink` ports | Talk to LLMs or storage directly (only via listeners) |
| `crates/llm` | `LanguageModel` port, `LlamaServer` supervisor, SSE client, prompt isolation | Depend on any Parlance crate |
| `crates/translation` | `Translator` port, `LlmTranslator`, `TranslationWorker` listener | Block the ASR thread |
| `crates/copilot` | App-owned `CopilotService`, session adapters, bounded contextual requests and reply generation | Block the ASR thread |
| `crates/storage` | Opt-in SQLite history and independent local project profiles | Store audio |
| `crates/models` | Pinned catalog (URL, size, SHA-256), verified download, `ModelStore` | Load models |
| `apps/desktop/src-tauri` | Composition root, Tauri commands, event hub, settings, LLM lifecycle (`intelligence.rs`) | Contain business logic |
| `apps/cli` | Headless developer tool and benchmarks | — |
| `apps/native-host` | Chromium native messaging host; relays tab audio to the desktop bridge | Depend on anything but `protocol` |

### Dependency graph (from `Cargo.toml`)

```mermaid
flowchart TD
  domain
  protocol --> domain
  audio --> domain
  capture --> domain & audio & protocol
  asr --> domain & audio
  session --> domain & protocol & audio & capture & asr
  llm
  translation --> domain & protocol & session & llm
  copilot --> domain & protocol & session & llm & translation
  storage --> domain & protocol & session
  models --> protocol
  desktop[apps/desktop/src-tauri] --> domain & protocol & audio & capture & asr & session & models & llm & translation & copilot & storage
  cli[apps/cli] --> domain & protocol & audio & capture & asr & session & models & llm & translation
  nativehost[apps/native-host] --> protocol
```

Allowed directions:

- Dependencies point toward `domain`. `domain` depends on nothing internal; `llm` depends on nothing internal.
- `translation`, `copilot` and `storage` depend on `session` only for the `SegmentListener`/`EventSink` ports.
  `session` never depends on them; the composition root injects them as listeners.
- Only `apps/*` may depend on `tauri`, `clap` or wire everything together. Core crates have no Tauri dependency.
- `parlance-asr` is pulled in with `default-features = false` at workspace level; apps opt into `whisper`
  (and `cuda` / `metal` / `vulkan` through the desktop crate's features).

## Runtime components

| Component | Kind | Owner | Lifetime |
| --- | --- | --- | --- |
| Tauri main thread + webview | OS thread | Tauri | App |
| Tokio runtime (Tauri async runtime) | thread pool | Tauri | App |
| `parlance-wasapi-capture` / `parlance-core-audio` / `parlance-parec` | OS thread per source | `CaptureStream` | Session |
| `parec` | child process per source (Linux) | `ParecStream` | Session |
| `parlance-dsp` | OS thread | `RunningPipeline` | Session |
| `parlance-asr` | OS thread | `RunningPipeline` | Session |
| Translation worker | Tokio task | `TranslationWorker` | Session |
| Copilot worker, one active request + latest automatic pending | Tokio task | `CopilotService` | App; session-scoped audio adapter |
| `parlance-history` | OS thread | `HistoryRecorder` (only if history is on) | Session |
| LLM supervisor + log forwarders | Tokio tasks | `LlamaServer` | While translation, automatic copilot or an explicit manual query needs the installed model |
| `llama-server` | child process | `LlamaServer` (Windows Job Object; Unix lifetime-pipe guardian) | Same |
| `parlance-bridge` (accept) + `parlance-bridge-conn` (one per tab) | OS threads | `BridgeBackend` | App / per connection |
| Model downloads | Tokio tasks (command futures) | `download_model` | Per download |

Details, queue sizes and shutdown order: [concurrency.md](concurrency.md).

## Streaming data flow

```mermaid
flowchart LR
  dev[Device / parec / bridge] -->|interleaved f32| ring[(SPSC ring<br/>10 s per source)]
  ring --> dsp
  subgraph dsp[DSP thread]
    direction TB
    d1[downmix] --> d2[resample to 16 kHz] --> d3[mix sources] --> d4[level meter 20 Hz] --> d5[VAD gate]
  end
  dsp -->|GateEvent, bounded 512| asr
  subgraph asr[ASR thread]
    direction TB
    a1[StreamingTranscriber] --> a2[LocalAgreement-2]
  end
  asr -->|TranscriptSegment| listeners
  asr -->|SessionEvent| hub[EventHub → Tauri Channel]
  dsp -->|Level, Notice| hub
  subgraph listeners[SegmentListeners]
    tw[TranslationWorker<br/>queue 64] --> llm[llama-server]
    cw[Copilot session adapter] --> cs[Application CopilotService]
    cs --> llm
    hr[HistoryRecorder] --> db[(SQLite)]
  end
  tw -->|Translation| hub
  cs -->|Suggestion| hub
  manual[Written question + profile snapshot] --> cs
```

Per utterance:

1. The VAD gate emits `SpeechStarted` (with 200 ms pre-roll), `Speech(samples)`, `SpeechEnded` (600 ms silence or
   20 s cap). Silero via whisper.cpp; energy VAD when the Silero model is missing.
2. While open, the utterance is re-decoded after every 500 ms of new audio once it is ≥ 500 ms long, with a reduced
   `audio_ctx` ("fast" pass). LocalAgreement-2 commits the prefix two consecutive hypotheses agree on.
3. On close, a full-context pass produces the `Final` revision; the committed prefix stays frozen within the
   same language. Auto re-detects on each pass and resets agreement if the language changes.
4. Segment ids are stable; `revision` increases monotonically; the UI replaces rows in place.
5. Partials are translated latest-wins; final segments are translated in order, using two successful prior lines
   as context. If source and target match, translation is passthrough. Generated repetition exceeding the source
   repetition allowance is cancelled, cleared from the view and excluded from context.
6. Final segments also go to the copilot (question detection → reply in the speaker's language → translation
   into the user's language).

## Session state machine

From `crates/domain/src/session.rs`. Every transition emits `SessionEvent::StateChanged`.

```mermaid
stateDiagram-v2
  [*] --> Idle
  Idle --> Starting: Start
  Failed --> Starting: Start
  Starting --> Running: MarkRunning
  Running --> Paused: Pause
  Paused --> Running: Resume
  Starting --> Stopping: Stop
  Running --> Stopping: Stop
  Paused --> Stopping: Stop
  Stopping --> Idle: MarkStopped
  Starting --> Failed: Fail(reason)
  Running --> Failed: Fail(reason)
  Paused --> Failed: Fail(reason)
  Stopping --> Failed: Fail(reason)
```

- `is_capturing()` is true only in `Running`; `is_active()` is true in `Starting`, `Running`, `Paused`.
- `SessionController::start` fails the session (`Fail`) if a source or model cannot be opened.
- When every capture stream ends, the controller emits a warning notice and stops itself (generation-guarded so a
  stale signal cannot stop a newer session).
- While `Paused`, audio is still metered but the gate is flushed and nothing reaches ASR.

## Platform status

| Platform | Native capture | Status |
| --- | --- | --- |
| Windows 10/11 | WASAPI endpoint loopback, process loopback (build ≥ 20348), microphones | Verified end-to-end |
| Linux | `pactl`/`parec` over the PulseAudio protocol (PulseAudio or pipewire-pulse) | Compiles, never run |
| macOS | none (`platform_backend()` returns `Unsupported`) | Planned: Core Audio taps |
| Any Chromium browser | MV3 extension + native host + bridge | Tests pass; not yet loaded in a real browser |

See [`docs/platform/`](../platform/) and the ADRs in [`docs/adr/`](../adr/).
