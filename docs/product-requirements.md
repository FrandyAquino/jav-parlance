# Product requirements

## Problem

People in multilingual meetings lose the thread when they cannot follow a language in real time, and freeze
when asked a question in a language they read better than they speak. Existing tools send audio to the cloud,
cost money per minute, or do not work offline.

## Users

- Non-native participants in international meetings (primary).
- Interpreters and note-takers who want a live, bilingual transcript.
- Privacy-sensitive teams that cannot send meeting audio to third parties.

## Goals

1. Live transcript of what others say, in their language, with automatic language detection.
2. Live translation into the user's language, aligned line by line with the original.
3. When someone asks a question, a short suggested reply in the speaker's language plus its meaning in the
   user's language.
4. Works with any meeting app: system audio, one application, a microphone, or one browser tab.
5. Fully local after a one-time model download; no account; no paid APIs.
6. Answer written technical questions without audio, using reusable local project context and an optional user idea.

## Non-goals (for now)

Meeting summaries and action items, speaker diarization by name, cloud sync, mobile apps, recording audio files.

## Experience requirements

- Floating, resizable, always-on-top window that stays out of the way; translucent where the OS supports it.
- One action to start listening; recording state and elapsed time always visible.
- Text appears while people speak (partial), then settles (final) without jumping.
- Keyboard-first: start/stop, pause, copilot, settings and pin all have shortcuts.
- Clear, actionable errors (missing model, permission denied, device lost) instead of silent failure.
- Compact subtitles, expanded preparation and a pinned reading view share one window and preserve the active session.
- Automatic suggestions never replace a manual draft/result or a pinned answer. Replies identify their project context and revision.
- Named project profiles can be created, edited, selected or deleted locally. Context is captured per request; later changes do not rewrite previous answers.
- The written-question composer chooses an explicit reply language and keeps inputs when generation fails or context is too large.

## Quality targets (CPU-only laptop, default models)

| Metric | Target | Baseline (see docs/benchmarks) |
| --- | --- | --- |
| Final transcript after speaker stops | < 1.0 s p95 | 0.76 s |
| First partial after speech starts | < 2.0 s | 1.7 s |
| Translation after final line (GPU / CPU) | < 1.0 s / < 2.0 s | 0.3 s / 0.5 s |
| WER on clear English speech | < 10 % | 0 % (JFK) |
| Steady-state RAM without LLM | < 500 MB | ~295 MB |

## Privacy requirements

Audio never written to disk; transcripts stored only with explicit opt-in; no telemetry; capture never starts
without a user action. See `docs/security.md`.
Saving a project profile is separate from opting into meeting history. Written questions, ideas and replies remain in memory for the app run.
