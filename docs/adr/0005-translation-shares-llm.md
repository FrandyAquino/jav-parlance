# 0005 — Translation shares the copilot LLM

**Context.** Candidates were MADLAD-400 3B (Apache-2.0, 400+ languages, encoder-decoder, not servable by
`llama-server`), OPUS-MT (one model per language pair, mixed Apache-2.0/CC-BY-4.0), NLLB (CC-BY-NC, rejected as
non-permissive), and the Qwen instruct model already loaded for the copilot.

**Decision.** Default translation uses the same Qwen3.5 instance through `llama-server`, with a strict
translation prompt, transcript isolation and temperature 0. Partial hypotheses are translated while the person
speaks, latest-wins (a newer partial replaces any queued one, so the newest words are always translated next);
final segments remain queued in order, with the current live partial given a turn between finals. A final,
retraction, new segment or source-language correction cancels an obsolete in-flight partial. Same-segment word
additions do not restart generation on every ASR update. Only successful finals feed the
translation context. The `Translator` port allows a CTranslate2 adapter (MADLAD or OPUS-MT via `ct2rs`) later.

The visible preview remains while a replacement streams a shorter/different prefix; completed corrections
still replace it, including shorter text. `Translation.is_final` continues to mean a final source segment,
not merely that one provisional generation has completed. Retained previews are bounded by the final queue
capacity plus the active partial. Intentional preemption does not produce a failure notice.

**Consequences.** One model in memory instead of two, which matters on 8–16 GB machines. Translation and copilot
share the server through two parallel slots (`--parallel 2`), so a long copilot reply cannot block translation;
superseded copilot generations are cancelled. There is no explicit priority scheduling yet (roadmap M4).
