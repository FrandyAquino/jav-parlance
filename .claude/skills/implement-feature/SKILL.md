---
name: implement-feature
description: End-to-end workflow for adding a feature to Parlance across Rust crates, Tauri IPC, generated TS contracts and the React UI, with tests and docs. Use for any user-visible feature or new command/event.
---

# Implement a feature

1. **Locate.** Read `docs/architecture/overview.md` and decide which crate owns the behaviour. Logic never goes in
   `apps/desktop/src-tauri` beyond wiring. If it crosses crate boundaries or changes threading, consult the
   `architect` agent first.
2. **Domain/contract.** Add or change types in `crates/domain` (pure) or `crates/protocol` (wire), deriving
   `Serialize, Deserialize, TS` with `#[serde(rename_all = "camelCase")]`, `#[ts(export)]`, and
   `#[ts(type = "number")]` on 64-bit integers. New UI event → new `SessionEvent` variant.
3. **Core.** Implement behind a port in the owning crate. Add unit tests next to the code and an integration test
   with fakes (`ReplayBackend`, scripted recognizer, fake `LanguageModel`) if it spans stages.
4. **Wire.** In `apps/desktop/src-tauri`:
   - compose it in `state.rs` / `intelligence.rs` / `commands.rs`;
   - new command: add to `generate_handler!` (`lib.rs`), `COMMANDS` (`build.rs`) and `allow-<name>` in
     `capabilities/default.json`; map errors in `error.rs` to an `ErrorCode`.
   - mirror in `apps/cli` if it helps headless testing.
5. **Regenerate contracts.** `cargo test --workspace` (writes `apps/desktop/src/bindings`).
6. **UI.** Add the wrapper in `src/lib/ipc.ts`; handle events in `reduceSession` (`store/session.ts`) with a vitest
   case; build components from `components/ui` primitives following `docs/design-system.md`; add shortcuts in
   `App.tsx` if relevant.
7. **Verify.**
   ```sh
   cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
   pnpm -C apps/desktop typecheck && pnpm -C apps/desktop lint && pnpm -C apps/desktop test
   pnpm dev   # manual check
   ```
8. **Docs (same change).** `docs/roadmap.md`; `docs/architecture/ipc.md` for commands/events;
   `concurrency.md` for threads/queues; a new or superseding ADR for decisions; platform docs if OS-specific.
   Don't mark anything verified that was only compiled.
