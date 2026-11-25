# Security and privacy

## Principles

1. **Local by default.** Audio, transcripts, translations and replies never leave the device. The only network
   traffic is downloading catalog artifacts the user requests (Hugging Face, GitHub releases).
2. **Explicit capture.** Capture starts only from a user action (Listen button / shortcut, extension click). A
   recording indicator and timer are always visible while capturing.
3. **No audio at rest.** Audio lives only in bounded in-memory buffers. History (opt-in, off by default) stores
   text only, in `<local data dir>/parlance/history.db`, deletable per meeting.
4. **Least privilege IPC.** The webview can call only the commands listed in `src-tauri/build.rs` and granted in
   `capabilities/default.json`, plus a few window-chrome permissions. CSP disallows remote scripts and
   connections. Raw audio never crosses into the renderer.

## Threats and mitigations

| Threat | Mitigation | Where |
| --- | --- | --- |
| Prompt injection spoken in a meeting ("ignore previous instructions…") | Transcript text is only embedded via `quote_transcript`: angle brackets neutralized so the block cannot be closed, control characters stripped, length bounded; every system prompt states the block is untrusted data; output token budgets are bounded; replies are suggestions the user must read and speak, never executed | `crates/llm/src/prompt.rs`, translation and copilot prompts |
| Tampered or truncated model / runtime download | Pinned size + SHA-256 per artifact; staged `.part` file renamed only after verification; atomic archive extraction | `crates/models` (ADR 0009) |
| Another local process driving the LLM | `llama-server` binds `127.0.0.1` on an ephemeral port with a random 192-bit API key | `crates/llm/src/server.rs` |
| Orphaned LLM process holding memory after a crash | Windows job object with kill-on-close; kill-on-drop elsewhere | `crates/llm/src/job.rs` |
| Malicious local client injecting audio into the bridge | Bridge listens on loopback only and requires a random token from a user-private discovery file; constant-time comparison; 1 MiB frame cap; strictly typed frames | `crates/capture/src/bridge.rs` |
| Web pages reaching the native host | Host manifest allow-lists only the extension id; the extension requests no host permissions or content scripts | `apps/native-host`, `apps/extension/public/manifest.json` |
| Crash or partial write corrupting settings/history | Settings written to a temp file then renamed; SQLite in WAL mode with schema migrations | `src-tauri/src/settings.rs`, `crates/storage` |
| Capture glitches affecting other apps | Capture threads never block; overflow drops and counts samples | `crates/audio/src/ring.rs` |

## Rules for contributors

- Never add a network call outside `crates/models` downloads without an ADR.
- Never log transcript text at `info` or above; never log audio.
- New artifacts must include a verified SHA-256 and license in the catalog.
- New Tauri commands must be added to `COMMANDS` and to the capability file explicitly.
- Any LLM prompt that includes speech must use `parlance_llm::prompt::quote_transcript` and `UNTRUSTED_DATA_RULE`.

## Open items

- Discovery file permissions on Windows rely on the per-user `%LOCALAPPDATA%` ACL (Unix uses mode 0600).
- Code signing and update integrity are not implemented yet.
- Microphone/screen permission UX on macOS awaits the macOS adapter.
