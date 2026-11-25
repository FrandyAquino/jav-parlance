---
paths:
  - "crates/**"
  - "apps/**/*.rs"
---

# Rust conventions

- Edition 2024, `rustfmt.toml` (max width 120). Run `cargo fmt --all` and
  `cargo clippy --workspace --all-targets -- -D warnings` before finishing.
- No `unwrap()`/`expect()` outside tests (workspace lint `clippy::unwrap_used`). `expect` only on invariants with a
  message ("valid literal"). `dbg!` is denied.
- Every `unsafe` block gets a `// SAFETY:` comment; `unsafe_op_in_unsafe_fn` is denied.
- Errors: `thiserror` enums per crate; `anyhow` only in `apps/cli` and `apps/native-host`. Map to `IpcError` only in
  `apps/desktop/src-tauri/src/error.rs`.
- Public types implement `Debug` (lint `missing_debug_implementations`); hand-write it for types holding trait objects
  or native handles.
- Recover poisoned mutexes with `lock().unwrap_or_else(|p| p.into_inner())`, as the existing code does.
- Never block a capture thread: no allocation growth, locks, I/O, logging floods or inference in the device loop. Only
  `AudioSink::push`, which drops instead of blocking.
- `SegmentListener::on_segment` and `EventSink` run on the ASR/DSP threads: hand work off with `try_send` or an
  unbounded send; never `.await`, never do I/O inline.
- Blocking work called from async code goes through `tokio::task::spawn_blocking` (model load, device enumeration,
  pipeline shutdown, archive extraction).
- Every async AI or network operation takes a `CancellationToken` and selects on it.
- New tunables are named `const`s with a doc comment explaining the number.
- Platform code lives behind `#[cfg(...)]` modules inside the adapter crate (`capture/src/windows`, `linux`); shared
  code stays platform-free.
- Keep comments minimal and explain *why*.
