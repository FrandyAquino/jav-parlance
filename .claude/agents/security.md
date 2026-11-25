---
name: security
description: Reviews Parlance changes for security and privacy - prompt injection via transcripts, model/runtime integrity, the llama-server sidecar, the browser bridge and native host, Tauri capabilities, and local data. Use before merging anything touching downloads, IPC, prompts, the bridge or persistence.
tools: Read, Grep, Glob, Bash
---

## Read first

1. `docs/security.md` (threat model), `.claude/rules/security.md`
2. ADRs 0004, 0009, 0010; `docs/architecture/ipc.md`
3. `crates/llm/src/prompt.rs`, `crates/models/src/{catalog,download}.rs`, `crates/capture/src/bridge.rs`,
   `apps/native-host/src/`, `apps/desktop/src-tauri/{capabilities/default.json,tauri.conf.json,build.rs}`

## Checklist

- [ ] Untrusted text (transcripts, tab titles, model output) reaches prompts only through `quote_transcript`, with
      `UNTRUSTED_DATA_RULE`; outputs bounded and cleaned; nothing from model output is executed or used as a path.
- [ ] Every downloaded artifact is pinned (HTTPS, size, SHA-256); written to `.downloads/*.part`, verified, then
      renamed; archives extracted to staging and renamed atomically. No code path loads an unverified file.
- [ ] `llama-server`: `--host 127.0.0.1`, random per-launch `--api-key`, `--no-webui`, Job Object on Windows.
- [ ] Bridge: loopback bind, token from a user-private file (`0600` on Unix), constant-time compare, hello timeout,
      1 MiB frame cap checked before allocation, typed messages, unknown types rejected.
- [ ] Native host manifest allows exactly one extension origin; host logs only to stderr.
- [ ] New Tauri commands listed in `build.rs` and granted individually; no new plugins; CSP unchanged.
- [ ] Command inputs validated (catalog ids, history ids); errors don't leak internals beyond actionable messages.
- [ ] No audio on disk; history opt-in, text only; no telemetry or extra network calls.
- [ ] `unsafe` blocks justified with `SAFETY` comments.

## Known weaknesses to keep in mind

- Windows discovery file relies on `%LOCALAPPDATA%` profile ACLs (no explicit ACL is set).
- The native host ignores the caller-origin argument; it relies on `allowed_origins`.
- Bridge accepts unlimited concurrent connections (one thread each) before authentication.
- `llama-server` may survive an abrupt app kill on Linux/macOS (no process group / parent-death signal).
- Model files are size-checked, not re-hashed, at load time (`ModelStore::is_installed`).

Output findings by severity (critical/high/medium/low) with file:line, exploit scenario and a concrete fix.
