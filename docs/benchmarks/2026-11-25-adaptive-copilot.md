# Adaptive window and contextual copilot — 2026-11-25

Local, uncommitted implementation on top of `6b9bf7a1ea9127fad668c4a1d3c5ef353764e13e`.
This record distinguishes automated contracts, native macOS interaction and model-backed replay measurements.
No commit, push, PR or remote CI run was created for this change.

## Environment

- Apple M5, 10 logical CPUs, 24 GiB unified memory; macOS 27.0.1.
- Whisper Base Q8 and Silero VAD 5.1.2, with the existing Metal build path.
- Qwen3.5 2B Q4_K_M and llama.cpp server b12686 with GPU offload, two slots and two CPU threads.
- Native QA used an ad-hoc signed debug bundle named `Parlance Local`, with a temporary build-time bundle identifier to distinguish it from the previously installed application. Product source and permissions were unchanged by that name override.
- Existing installed models were used; no model download was required.
- Final normal-name debug bundle: `target/debug/bundle/macos/Parlance.app`; `codesign --verify --deep --strict` passed. Executable SHA-256: `70e810f76728b0e4e18167a3627f6253db47f780e7b79f98fc99c0ebc9b460df`.

## Automated verification

| Check | Observed result |
| --- | --- |
| `cargo test --workspace` | 208 passed, 0 failed, 1 existing subprocess helper ignored |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed after formatting |
| `pnpm typecheck` and `pnpm lint` | Desktop and extension passed |
| `pnpm test` | Desktop 37 passed; extension 31 passed |
| Tauri debug app bundle | Built and ad-hoc signed on macOS |

Core tests use controlled model/translator ports and temporary profile storage.
They cover request priority, cancellation and late callbacks, session epochs, immutable profile attribution, capacity rejection, Unicode/delimiter isolation, profile save failures and concurrent mutations.
The desktop Rust crate retains `test = false`; profile persistence tests execute in storage, and native interaction checks the command/permission wiring.

Observed regression failures before their fixes included a token-limit stream being accepted, capture shortcuts firing in editable fields, and an ASR callback blocking while a copilot event sink was deliberately stalled.
The UI also verifies frozen replies, early events before IPC acknowledgement, preservation of rejected inputs, profile-editor focus return and transcript continuity across modes.
The later dialog-shortcut and idea-field submission tests were added with the fixes; no red-before-fix execution is claimed for those two tests.

## Native macOS observations

- Opened compact subtitles, expanded preparation and pinned reading in the same native window; the view resized and the narrow toolbar remained within its surface.
- Submitted a written question with Command+Enter while capture was off. The local model produced English text and Spanish meaning without starting audio.
- Created and selected a synthetic project profile, quit, reopened and confirmed the profile and active selection survived. Meeting history remained off.
- Turned automatic suggestions off and verified that written questions still worked. Restored the original automatic-suggestions preference afterward.
- Renamed the profile from `QA temporary API` to `QA updated API`; the prior response retained the old name and revision 1. The next response used revision 2.
- Typed a profile edit and pressed Command+J; the dialog and draft remained open. Submitted from the optional idea field with Command+Enter.
- Started real system-output capture and played the existing 11-second JFK fixture through `afplay`. The native transcript and Spanish translation advanced while a manual response was requested. Entering pinned reading kept the capture clock running; Stop returned to audio off.
- Observed the reply-copy control report `Copied`.
- Quit the test application and removed only the synthetic profile after matching its name, content and revision. No user profile or history was removed.

The audio check is a short functional check, not a long-session benchmark or an accuracy certification.
Native Windows, Linux, Intel Mac and older-macOS fallback acceptance were not performed for this change.
Arbitrary monitor layouts and every resize size were not exercised natively; geometry and error paths also have mocked API tests.
No standalone browser E2E run was performed: the product surface was exercised through the native Tauri app, with frontend component tests separately.

## Model quality observations

One general cache-aside question initially produced an imprecise technical explanation from the 2B model.
With a profile stating that TTL information had not been supplied, an initial answer incorrectly inferred that the team had no TTL configured.
The system prompt was tightened to distinguish unknown information from absent configuration.
A subsequent native answer stated that the provided context did not specify the TTL and requested configuration details, while retaining the known Redis caching fact.

Another response suggested a 30-second TTL as a recommendation without enough workload context to justify it.
The application does not certify technical correctness, current library behavior or team configuration beyond the supplied context.
These observations support manual review of suggestions and do not establish a general factual-accuracy rate.

## Review corrections

- Moved copilot ingress off the ASR path after reproducing blocking event delivery; kept bounded queues and visible context omission.
- Reused the sidecar's HTTP client and removed duplicate suggestion state from the transcript store.
- Serialized settings reconciliation and manual admission to prevent an older settings snapshot from rolling back the current model/preferences.
- Protected open dialog drafts from application shortcuts and made submit work from both composer fields.
- Drained a complete HTTP request in the cancellation test before asserting connection EOF, avoiding assumptions about TCP packet boundaries.

Ten independent review lenses completed with no retained code findings or unaddressed implementation requirements; the four reported candidates were checked against their corrections. Review run: `20261125-151901-600ee98d`, held in local review scratch under `/tmp/compound-engineering-501/ce-code-review/`.
This is source-review coverage, not factual-accuracy certification or acceptance on every supported platform.

After that review, a final parent lifecycle trace removed an early copilot-end call from the normal Stop command. The session pipeline already flushes final ASR text before invoking listener end; the early call could reject that last segment from recent context. Startup-failure cleanup is retained. Copilot/session tests, workspace clippy and the final bundle rebuild passed afterward; native stop-mid-utterance was not rerun for this one-line wiring correction.

## Replay measurement method

The optional [probe](../../apps/cli/examples/adaptive_copilot_probe.rs) uses the real session pipeline, model, translator and copilot service with `ReplayBackend` at 1× speed.
It compares the same 132-second clip without manual requests and with three requests at 20%, 45% and 70% of playback.
The clip is the existing 11-second JFK fixture repeated twelve times; it is synthetic repetition of speech, not a real meeting or music corpus.
Fixture SHA-256: `2ad9ba1696e3704683485a4345f01694ac688b4cedc6bc9de663916efb7eacf6`.

Transcript and translation lag are publication time minus the session-start return time minus the final segment's audio end time.
Translation samples require nonempty final text; missing/empty results, notices and drain timeouts are reported separately.
Copilot time to first token and total time include queueing from manual submission.
The sidecar is shut down and awaited between conditions.
One pair is preliminary evidence, not a distribution across repeated runs or other hardware.

```sh
cargo build --release -p parlance-cli --example adaptive_copilot_probe
target/release/examples/adaptive_copilot_probe /tmp/parlance-u6-repeated-jfk.wav --repetitions 1
```

The fixture is intentionally not duplicated in the repository. Recreate it by concatenating twelve copies of `fixtures/audio/jfk.wav` as PCM16 WAV, preserving its sample format.

## Replay results

[Raw observations](2026-11-25-adaptive-copilot-results.jsonl) retain individual samples, request text/results, notices and state transitions.

| Metric | Baseline | Three manual questions | Difference |
| --- | --- | --- | --- |
| Final transcript publication p95 | 130.8 ms | 137.8 ms | +7.0 ms |
| Nonempty final translation publication p95 | 381.8 ms | 398.5 ms | +16.7 ms |
| Final transcript / translation samples | 39 / 39 | 39 / 39 | Same counts |
| Missing or empty final translations | 0 | 0 | None |
| Drain timeout | No | No | None |

All three manual requests completed. Time to first token was 202.4, 120.3 and 171.9 ms; complete bilingual results took 3.71, 2.66 and 4.95 seconds respectively.
Each condition had one normal Starting → Running → Stopping → Idle sequence and only the expected source-ended notice.
The observed differences fit the plan's +250 ms transcript / +1000 ms translation regression budgets on this machine for this pair.
The first condition loaded the speech model in about 5.1 seconds; the second reused the process cache and started in about 47 ms. Publication lag excludes that startup interval, so these figures do not compare cold-start performance.

The replay answers acknowledged unknown TTL and rejected the intentionally incorrect no-invalidation idea, but the cache-aside explanation remained imprecise and the model volunteered arbitrary TTL recommendations. The translation also rendered “write-through” imprecisely. This is a documented quality limitation of the tested 2B configuration, not a passing factual-accuracy claim.
