# Music decoding and live translation — 2026-11-25

## Reproduction

The user reported repeated English source text and Spanish translations during Drake's “God's Plan”,
plus translation retyping/delay until a pause. The source-side repetition showed the earlier translation-only
guard was insufficient.

A temporary 30-second official Apple Music preview was obtained through Apple's public preview API:
https://music.apple.com/us/album/gods-plan/1406109769
The audio and lyrics are not committed. The preview was converted locally to 16 kHz mono PCM for repeatable
pipeline checks. An exploratory YouTube playback was also attempted; the deterministic evidence below uses
the official preview, not a claim that the entire track was validated.

The real Whisper fast pass expanded 6,984 ms into 380 words. Fast decoding explicitly disabled temperature
fallback. Upstream whisper.cpp marks low-entropy/repeating decodes failed and retries at another temperature,
but the disabled fallback let that failed result escape. Restoring the native fallback made the same
`music_prefixes_do_not_expand_into_decoder_loops` regression pass. An end-to-end real-time replay completed
with 47 recognized words; the genuine short repeated refrain remained instead of the runaway expansion.

Run the opt-in music regression with legally available input:

```sh
PARLANCE_MUSIC_FIXTURE=/path/to/16k-mono-preview.wav cargo test -p parlance-asr --test whisper_fixture music_prefixes -- --nocapture
```

The test skips when the external fixture or local models are absent. Ordinary CI does not certify music
recognition. English and Spanish speech fixtures were also rerun to check for regressions.

## Translation scheduling and display

A probe using the actual compiled TranslationWorker and a controlled 906 ms translator established:

- Tokens were already delivered before a pause; the UI did not gate rendering on `is_final`.
- A new revision replaced a 13-character visible preview with a three-character prefix, causing retyping.
- An obsolete partial and two pending finals held the current live segment behind other work.

The corrected worker retains the visible preview while a replacement prefix is incomplete, but publishes
completed corrections even when shorter. It cancels obsolete partial futures on final/retraction/new-segment
or language correction, and gives current live work a turn between ordered finals. Same-segment word additions
do not repeatedly restart generation. Internal preemption produces no failure notice. Preview storage is bounded.

Regression tests cover preview stability, completed shorter corrections, continuous-input progress, ordered
final/live interleaving, final/new-segment/empty-final preemption, intentional cancellation and genuine failures.
The first three regressions failed on the old implementation and passed after the change.

A real preview replay with Whisper Base Q8 and local Qwen 2B produced partial translations while audio was
still arriving. The confirmation run completed all five final translations. Another run rejected one repetitive
model completion; the guard discarded it rather than presenting it as reliable text. This is not a guarantee
of lyric accuracy, and model-dependent output can vary.

## Typography

The live Gemini page's computed UI font was Google Sans Flex. The unmodified official variable font is
bundled locally, with a pinned source and SHA-256 in `apps/desktop/src/assets/fonts/SOURCE.md`. The OFL and
trademark notices ship as installer resources. Font loading requires no connection to Google.

## Validation

Full workspace: 172 passed, 0 failed, 1 intentionally ignored subprocess helper. The music fixture was supplied for this run.
Desktop typecheck/build/lint and 21 frontend tests passed. Workspace clippy with `-D warnings` passed.
Universal packaging and native font/launch verification are recorded in the delivery update below.

## Delivery update

The signed universal app was rebuilt, copied to `~/Applications/Parlance.app`, signature-verified and reopened.
Native inspection confirmed Google Sans Flex in the toolbar and empty-state text with the source picker ready.
Both license notices are present under the app's `Contents/Resources/licenses/google-sans-flex/` directory.
The executable path/hash was checked to avoid accidentally testing an older debug bundle with the same bundle ID.

The optional music test was strengthened to require actual fast decodes and nonempty final recognition; a
silent fixture now fails explicitly instead of passing vacuously. The final translation checks also verify that
failure retains a useful prior preview without falsely marking it as a completed final translation.

CI on the preceding head passed Rust checks on Linux/macOS and frontend checks. Windows MSVC exposed an
older MinGW-specific enum conversion applied too broadly; it is now limited to Windows GNU. A host-side
probe verified GNU unsigned/MSVC signed type handling; actual MSVC confirmation awaits CI.

The final focused translation suite passed 21 tests, including queue closure with pending ordered finals
and final-failure preview semantics. Recovery can take longer than a normal fast decode: one music partial
required about 1.4 seconds on this Mac. Sustained headroom on slower CPU-only machines remains unmeasured.
