---
name: code-review
description: Reviews a Parlance diff for correctness, real-time safety, security, contracts, tests and docs using the project's own rules. Use on any PR or local change before commit.
---

# Code review

1. `git diff` (or the PR). Read the touched files in full, not just hunks, plus the matching `.claude/rules/*.md`.
2. **Correctness.** State machine transitions via `SessionState::apply`; error mapping to the right `ErrorCode`;
   revision/ordering logic; off-by-one in sample/ms conversions (16 kHz in DSP/ASR, 48 kHz stereo from capture).
3. **Real-time safety.** Nothing new in capture loops beyond `AudioSink::push`; listeners and `EventSink` don't
   block; blocking work in async code wrapped in `spawn_blocking`; locks never held across `.await`.
4. **Concurrency and lifecycle.** Bounded queues with explicit overflow; cancellation tokens passed through; `Drop`
   joins threads / kills children; races between start/stop or between sessions (generation checks).
5. **Security.** `quote_transcript` + `UNTRUSTED_DATA_RULE` on every prompt; pinned SHA-256 for new downloads;
   loopback-only sockets; new commands in `build.rs` + capabilities; no new plugins or CSP changes.
6. **Contracts.** Rust-side types with `#[ts(export)]`; bindings regenerated and committed; UI uses `@/bindings`.
7. **Style.** English, minimal comments explaining why, no `unwrap` outside tests, `SAFETY` on `unsafe`, named
   constants for tunables, TS strictness respected, design tokens in UI.
8. **Tests.** A test that fails without the change; model/hardware tests skip cleanly.
9. **Run.**
   ```sh
   cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
   git status --porcelain apps/desktop/src/bindings
   pnpm -r typecheck && pnpm -r lint && pnpm -r test
   ```
10. **Docs.** `docs/roadmap.md`, ADR, `docs/architecture/*`, platform docs updated where behaviour changed.
11. **Report** findings as blocking / should-fix / nit, each with file:line and a suggested fix. Say explicitly what
    was not verified (hardware, browser, other OS).
