---
name: architecture-review
description: Structured review of a Parlance change or the whole codebase against its layering, concurrency, contract and documentation rules. Use before merging structural changes or periodically to find drift between docs and code.
---

# Architecture review

1. **Scope.** `git diff --stat` (or the named area). List touched crates.
2. **Dependency graph.** Compare every changed `Cargo.toml` with the graph in `docs/architecture/overview.md`:
   ```sh
   grep -n "parlance-" crates/*/Cargo.toml apps/*/Cargo.toml apps/desktop/src-tauri/Cargo.toml
   ```
   Flag: `session` → `llm`/`translation`/`copilot`/`storage`; any core crate → `tauri`; `domain` → anything.
3. **Ports and composition.** New behaviour behind a trait with a test fake? Wiring only in
   `apps/desktop/src-tauri` / `apps/cli`? No logic in `commands.rs`?
4. **Real-time path.** Grep the capture/DSP/ASR code for `lock()`, `.await`, `println!`, file or network I/O, and
   unbounded channels. Check queue capacities and overflow behaviour against `docs/architecture/concurrency.md`.
5. **Lifecycle.** Shutdown order, cancellation tokens, `Drop` impls, who owns each `Arc` after the session ends,
   generation guards on async callbacks.
6. **Contracts.** Types crossing IPC in `protocol`/`domain` with `#[ts(export)]`; bindings regenerated;
   `build.rs` `COMMANDS` == `generate_handler!` == `capabilities/default.json` grants:
   ```sh
   grep -o '"[a-z_]*"' apps/desktop/src-tauri/build.rs | sort
   grep -o 'allow-[a-z-]*' apps/desktop/src-tauri/capabilities/default.json | sort
   ```
7. **Security hooks.** Prompt isolation, pinned downloads, loopback-only sockets (see `.claude/rules/security.md`).
8. **Docs drift.** For each claim in the touched ADRs and `docs/architecture/*`, confirm it in code. Note claims the
   code does not implement (e.g. translation-first scheduling in ADR 0005).
9. **Report** ordered by severity: violation, file:line, why it matters, concrete fix, doc to update.
