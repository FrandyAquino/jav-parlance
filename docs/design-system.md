# Design system

A compact, translucent floating window that stays out of the way of the meeting. Source of truth:
`apps/desktop/src/styles.css` (Tailwind v4 `@theme`) and `apps/desktop/src/components/`.

## Tokens

CSS variables on `:root`, switched by `data-theme` (`light` | `dark`) and `data-effects` (`native` | `none`) on
`<html>` (set by `hooks/useTheme.ts`). Tailwind exposes them as `bg-surface`, `text-ink-muted`, `border-line`, etc.

| Token | Role | Dark | Light |
| --- | --- | --- | --- |
| `--surface` | Window and floating surfaces | `rgb(16 18 26 / .62)` | `rgb(250 250 253 / .7)` |
| `--surface-raised` | Hover, tracks, subtle fills | `rgb(255 255 255 / .06)` | `rgb(0 0 0 / .04)` |
| `--line` | Borders, dividers, scrollbar thumb | `rgb(255 255 255 / .09)` | `rgb(0 0 0 / .08)` |
| `--ink` | Primary text, final text | `#eceef5` | `#14161d` |
| `--ink-muted` | Secondary text, unstable/streaming text | `#a1a7ba` | `#545a6d` |
| `--ink-faint` | Labels, timestamps, placeholders | `#6c7286` | `#8a90a3` |
| `--accent` | Focus ring, meter, selected, copilot icon | `#86a8ff` | `#2f5bea` |
| `--accent-ink` | Text on accent | `#0b1020` | `#ffffff` |
| `--record` | Listen button, recording dot, errors, close hover | `#ff5c5c` | `#e5383b` |
| `--warn` | Paused dot, warnings | `#f5b759` | `#b26b00` |
| `--radius-window` | Window corner | `14px` | `14px` |

Dark is the default when no theme attribute is present.

### Surfaces and the `data-effects` fallback

- `.window` = `--surface` background + 1 px `--line` border + `--radius-window`. Used by the main window and the
  settings dialog.
- `html`, `body`, `#root` are transparent; the OS supplies the backdrop (Mica/Acrylic on Windows, `hudWindow`
  vibrancy on macOS, configured in `tauri.conf.json`).
- On Linux `useTheme` sets `data-effects="none"`, which swaps `--surface` for a near-opaque value
  (`.97` dark / `.98` light), because a webview cannot blur what is behind the window.
- Floating layers (select menus, tooltips, toasts, "Live" pill) use `bg-[var(--surface)]` + `backdrop-blur`;
  the settings dialog uses `backdrop-blur-xl` over a `bg-black/30` overlay.

## Typography

- Sans: bundled Google Sans Flex → Segoe UI Variable Text → system (SF Pro) → Noto Sans. Mono: Cascadia Mono → SF Mono →
  JetBrains Mono.
- Scale (px, via arbitrary Tailwind values): 10 (group labels, hover timestamps), 11 (column headings, metadata),
  12 (controls, hints, toasts), 13 (buttons, dialog body), 14 with `leading-relaxed` (transcript, replies).
- Weights: regular and `font-medium` only. `tabular-nums` for clocks and timestamps.
- Text is `user-select: none` by default; add `.selectable` to content users copy (transcript, suggestions).

## Spacing and shape

- Title bar `h-11`, controls `h-7` (`sm`/`icon`) or `h-8` (`md`), icons `size-3.5` (inline `size-3`).
- Content gutters `px-5`; column gap `gap-6`; row padding `py-1.5`.
- Radii: `rounded-lg` controls/lists, `rounded-xl` menus, `rounded-md` items/tooltips, `rounded-full` pills/dots.

## Components

| Component | File | Notes |
| --- | --- | --- |
| `Button` | `ui/button.tsx` | cva variants `primary`, `record`, `ghost` (default), `subtle`; sizes `sm`, `md`, `icon`; defaults `type="button"` |
| `Select` | `ui/select.tsx` | Radix Select, grouped options, required `label` → `aria-label` on trigger |
| `SwitchRow` | `ui/switch.tsx` | Radix Switch inside a `<label>` with description |
| `Tooltip`, `TooltipProvider` | `ui/tooltip.tsx` | Radix Tooltip, 400 ms delay |
| `TitleBar` | `TitleBar.tsx` | Drag region, session controls, source picker, meter, language pair, toggles, window buttons |
| `SessionControls` | `SessionControls.tsx` | Listen / elapsed clock / pause / stop |
| `SourcePicker` | `SourcePicker.tsx` | Primary source (grouped) + optional microphone; refreshes on pointer down |
| `LevelMeter` | `LevelMeter.tsx` | `role="meter"`, −60…0 dB, RMS bar + peak tick |
| `TranscriptView` | `TranscriptView.tsx` | One scroll container, one grid row per segment (original + translation stay aligned); stick-to-bottom with "Live" pill |
| `CopilotPanel` | `CopilotPanel.tsx` | Live/manual question selection, editable idea and output language, response status, context attribution, copy and pin |
| `ProjectProfiles` | `ProjectProfiles.tsx` | Local project selection and accessible create/edit/delete dialog |
| `PinnedReply` | `PinnedReply.tsx` | Frozen chosen answer with larger reading text and explicit return controls |
| `Notices` | `Notices.tsx` | Bottom-right toasts; info/warning auto-dismiss after 6 s, errors stay |
| `EmptyState` | `EmptyState.tsx` | First-run model download, then "pick a source" |
| `SettingsDialog` | `SettingsDialog.tsx` | Radix Dialog: Languages, Assistant, Models, Privacy (history), Appearance |
| `ModelList`, `HistoryList` | | Lists inside settings |

## Text states

- Transcript: stable text `text-ink`, unstable tail `text-ink-muted`.
- Translation: final `text-ink`, streaming `text-ink-muted`, missing `…` in `text-ink-faint`.
- Recording: pulsing `--record` dot (`.recording-dot`); paused: static `--warn` dot.

## Keyboard shortcuts (`App.tsx`, `hooks/useShortcuts.ts`)

`mod` = Ctrl or Cmd. Matching shortcuts call `preventDefault` (Ctrl+P would otherwise print).

| Keys | Action |
| --- | --- |
| `mod+Enter` | Start / stop listening outside editable fields; submit from either composer field |
| `mod+P` | Pause / resume |
| `mod+,` | Open settings |
| `mod+J` | Toggle compact subtitles / expanded preparation |
| `mod+T` | Toggle always on top |

Capture and pause shortcuts ignore editable fields. Modes retain transcript scroll/follow state and manual work. A completed answer can be pinned for reading; later live suggestions cannot change it. Native sizes are remembered per mode during the run, without persistent geometry storage.

## Accessibility

- Every icon-only button has `aria-label`; toggles use `aria-pressed` (copilot, pin).
- Decorative icons and dots are `aria-hidden`; state is also given as `sr-only` text ("Recording" / "Paused").
- Transcript is `role="log"` + `aria-live="polite"`; the clock is `aria-live="polite"`; toasts are in `role="status"`.
- Suggestion card sets `aria-busy` while streaming; download progress uses `role="progressbar"`.
- Dialog has a title and an `sr-only` description; `Select` requires a `label`.
- Keyboard focus: global `:focus-visible` outline 2 px `--accent`, offset 2 px. Hover-revealed controls (copy)
  also appear on `focus-visible`.
- `lang` is set on translated paragraphs.
- Reduced motion: `prefers-reduced-motion: reduce` disables the recording pulse and sets all transition durations
  to 0.

## Do / don't

- Do use the token utilities (`text-ink-muted`, `border-line`, `bg-surface-raised`); don't hard-code colors (the
  only exception in code is `text-white` on `record`).
- Do keep surfaces translucent only through `--surface`; don't add blur/translucency that ignores `data-effects`.
- Do render original and translation in the same row; don't build two independently scrolling columns.
- Do use `Button`, `Select`, `SwitchRow`, `Tooltip` primitives; don't introduce new component libraries.
- Do give icon-only controls an `aria-label` and a tooltip with the shortcut.
- Do bound what the renderer keeps (rows, suggestions, notices); don't keep unbounded meeting state in the UI.
- Do mark copyable content `.selectable`; don't make chrome selectable.
- Don't animate beyond short color/transform transitions, and always respect reduced motion.
- Don't hand-write IPC types; import from `@/bindings`.

## Glass layer (2026-11-25)

- Surfaces float as frosted panels over an ambient light field (`.ambient`) inside the window, plus a faint
  `NoiseTexture` grain. Utilities in `styles.css`: `glass` (panels), `glass-strong` (popovers, dialog, toasts),
  `glass-accent` (copilot). Tokens: `--glass`, `--glass-strong`, `--glass-border`, `--glass-highlight`,
  `--glass-accent(-border)`, `--ambient-a/b/c`, defined for dark and light themes.
- Shadcn-compatible registry components from Magic UI (`components.json` registries `@magicui`, `@reui`):
  `ui/noise-texture`, `ui/progressive-blur` (fades transcript under the panel edge), `ui/border-beam` wrapped by
  `LiveBeam` (recording / copilot generating). `LiveBeam` renders nothing under reduced motion.
- Add more registry items with `npx shadcn@latest add @magicui/<item>` from `apps/desktop`.
