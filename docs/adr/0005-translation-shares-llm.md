# 0005 — Translation shares the copilot LLM

**Context.** Candidates were MADLAD-400 3B (Apache-2.0, 400+ languages, encoder-decoder, not servable by
`llama-server`), OPUS-MT (one model per language pair, mixed Apache-2.0/CC-BY-4.0), NLLB (CC-BY-NC, rejected as
non-permissive), and the Qwen instruct model already loaded for the copilot.

**Decision.** Default translation uses the same Qwen3.5 instance through `llama-server`, with a strict
translation prompt, transcript isolation and temperature 0. Final segments are translated; partials are not,
to save compute. The `Translator` port allows a CTranslate2 adapter (MADLAD or OPUS-MT via `ct2rs`) later.

**Consequences.** One model in memory instead of two, which matters on 8–16 GB machines. Translation and copilot
share the server through two parallel slots (`--parallel 2`), so a long copilot reply cannot block translation;
superseded copilot generations are cancelled. There is no explicit priority scheduling yet (roadmap M4).
