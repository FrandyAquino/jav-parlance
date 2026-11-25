# 0001 — Tauri v2 shell, Rust core, React UI

**Context.** The app needs native audio capture on three OSes, CPU/GPU inference, a small footprint and a
polished translucent floating window.

**Decision.** Tauri v2 hosts a React + TypeScript UI (Vite, Tailwind v4, Radix/shadcn-style primitives,
Zustand). Audio, inference and persistence live in Rust crates with no Tauri dependency;
`apps/desktop/src-tauri` is only the composition root and an IPC adapter.

**Consequences.** Core crates are testable headlessly (`apps/cli` reuses them unchanged). Raw audio never
crosses into the renderer; only typed events do (levels at 20 Hz, segments, translations, suggestions). We
depend on each OS webview (WebView2, WKWebView, WebKitGTK), so visual effects need per-platform fallbacks.
