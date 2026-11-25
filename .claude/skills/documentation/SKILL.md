---
name: documentation
description: Writes or updates Parlance documentation (architecture, platform, testing, design system, ADRs, roadmap) so it matches the code exactly. Use after behaviour changes or when docs and code disagree.
---

# Documentation

1. **Map the change to docs.**
   | Change | Doc |
   | --- | --- |
   | Crate, dependency, thread, data flow, state machine | `docs/architecture/overview.md` |
   | Queue, capacity, cancellation, supervision, shutdown | `docs/architecture/concurrency.md` |
   | Tauri command, `SessionEvent`, capability, bridge protocol | `docs/architecture/ipc.md` |
   | OS capture/runtime/setup | `docs/platform/{windows,linux,macos,browser-extension}.md` |
   | Tests, fixtures, benchmark commands | `docs/testing.md` |
   | Tokens, components, shortcuts, a11y | `docs/design-system.md` |
   | Decision with alternatives | new `docs/adr/NNNN-*.md` + row in `docs/adr/README.md` |
   | Progress | `docs/roadmap.md` |
   | Measurements | `docs/benchmarks/` |
   | Threat model | `docs/security.md` |
2. **Read the code, not the old doc.** Take constants, names and commands from source (`grep -rn "const "` in the
   crate); copy command names from `build.rs`, events from `crates/protocol/src/events.rs`, states from
   `crates/domain/src/session.rs`.
3. **Status words.** "verified" only for what ran end-to-end on real hardware; otherwise "compiles", "tests pass",
   "never run", or "planned". Never describe planned features in the present tense.
4. **Style.** English, concise, skimmable: tables for inventories, Mermaid for graphs/sequences/state machines,
   exact shell commands in fenced blocks. No filler, no marketing.
5. **ADRs.** Context / Decision / Consequences, short. Never rewrite an accepted ADR's decision; supersede it
   (`Superseded by NNNN`). Fix factual drift (numbers, file names) in place.
6. **Agent tooling.** Keep `CLAUDE.md`, `.claude/rules/*`, `.claude/agents/*`, `.claude/skills/*` consistent with the
   docs when commands, paths or rules change.
7. **Check.** Every file path and command in the edited doc exists/runs; Mermaid blocks render (no unbalanced
   brackets); links are relative and resolve.
