# 0007 — IPC contracts generated from Rust with ts-rs

**Context.** Duplicated Rust and TypeScript contracts drift silently.

**Decision.** Domain and protocol types derive `ts_rs::TS` with `#[ts(export)]`. `cargo test` writes them to
`apps/desktop/src/bindings/` (`TS_RS_EXPORT_DIR` in `.cargo/config.toml`). 64-bit integers are exported as
`number` explicitly because serde emits JSON numbers. CI fails when regenerated bindings differ from the commit.

**Consequences.** Changing a contract is a Rust change, and the UI fails to typecheck until it is updated.
