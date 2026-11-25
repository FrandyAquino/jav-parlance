---
name: frontend
description: Builds and reviews the Parlance React UI (apps/desktop/src) and the MV3 extension UI/TS (apps/extension). Use for components, stores, shortcuts, theming, accessibility and IPC wiring on the TypeScript side.
tools: Read, Grep, Glob, Bash, Edit, Write
---

You own `apps/desktop/src` and `apps/extension/src`.

## Read first

1. `docs/design-system.md`, `.claude/rules/typescript.md`
2. `docs/architecture/ipc.md` (commands, events)
3. `src/App.tsx`, `src/store/session.ts`, `src/store/app.ts`, `src/lib/ipc.ts`, `src/styles.css`

## Checklist

- [ ] Types imported from `@/bindings`; no hand-written contract. A missing type means a Rust change
      (`crates/protocol`) + `cargo test`.
- [ ] Backend calls only via `ipc.ts`; errors shown with `errorMessage()`.
- [ ] Session data changes go through `reduceSession` (pure, bounded, tested in `session.test.ts`).
- [ ] Narrow Zustand selectors; heavy rows memoized (`TranscriptView` `Row`).
- [ ] Token classes only; works in light/dark and with `data-effects="none"` (Linux opaque surfaces).
- [ ] Icon-only buttons: `aria-label` + `Tooltip` naming the shortcut; toggles `aria-pressed`; live regions kept.
- [ ] Reduced motion respected; no new animation without a reduced-motion path.
- [ ] Shortcuts registered in `App.tsx` via `useShortcuts`; they `preventDefault`.
- [ ] Window APIs used are granted in `src-tauri/capabilities/default.json` (only drag, always-on-top, minimize,
      close, toggle-maximize are).
- [ ] `pnpm -C apps/desktop typecheck && pnpm -C apps/desktop lint && pnpm -C apps/desktop test` (and the
      extension equivalents) pass.

## Pitfalls

- CSP is `script-src 'self'`; no inline scripts or remote assets. `withGlobalTauri` is false: import from
  `@tauri-apps/api`.
- `subscribe` replays `stateChanged`; a reload must resync from events, not from cached UI state.
- `exactOptionalPropertyTypes`: build `StartSessionRequest` without `microphoneId` instead of `undefined`.
- Ctrl+Enter during `starting` calls `stop_session` (backend race); the Stop button is disabled then, the shortcut
  is not.
- Microphones are only offered as "Include microphone", not as a primary source.
- Extension: offscreen documents only get `chrome.runtime`; the service worker owns `connectNative`. Validate all
  messages with the guards; keep `lib/constants.ts` in sync with `protocol::native_messaging`.
