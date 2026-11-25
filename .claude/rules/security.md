---
paths:
  - "crates/**"
  - "apps/**"
---

# Security rules

- **Prompt isolation**: transcript text (and anything derived from other people's speech, tab titles included) is
  untrusted. Embed it only via `parlance_llm::prompt::quote_transcript` and append `UNTRUSTED_DATA_RULE` to the
  system prompt. Clean outputs with `clean_completion`. Bound output with `max_tokens`.
- **Downloads**: every artifact is a `CatalogEntry` in `crates/models/src/catalog.rs` with HTTPS URL, exact byte
  size and pinned SHA-256 (from HF LFS metadata or GitHub release digests). Never load or execute a file that did
  not pass `parlance_models::download`. Upgrading a model is a reviewed code change.
- **Sidecar**: `llama-server` binds `127.0.0.1` only, on a free port, with a fresh random `--api-key` per launch;
  `--no-webui`. Never `0.0.0.0`.
- **Bridge**: loopback TCP only, random token in a user-private discovery file, constant-time comparison, 5 s hello
  timeout, frames capped at `MAX_MESSAGE_BYTES` (1 MiB) before allocating, strictly typed messages.
- **Native host manifest**: `allowed_origins` is exactly one `chrome-extension://<id>/`; validate the id format.
- **Tauri**: new commands go in `build.rs` `COMMANDS` and get an explicit `allow-<command>` in
  `capabilities/default.json`. Do not add filesystem/shell/http plugins or loosen the CSP without an ADR.
- **Privacy**: audio is never written to disk. History is opt-in (`history_enabled`, default false) and stores text
  only. No telemetry, no network calls except user-initiated downloads.
- Validate all command inputs in the Rust command (ids exist in the catalog, enums parse).
- Log no transcript text above `debug`.
- See `docs/security.md` for the threat model.
