# 0002 — whisper.cpp ASR with LocalAgreement-2 streaming

**Context.** We need low-latency, revisable transcripts in ~100 languages, offline, on CPU-only laptops as
well as GPUs. Whisper is batch-oriented (30 s windows).

**Decision.** Use whisper.cpp through `whisper-rs`. Utterances are bounded by VAD (at most 20 s), so no
window trimming is needed. While an utterance is open it is re-decoded every ~1 s of new audio with a reduced
`audio_ctx` (fast partial pass). The LocalAgreement-2 policy from WhisperStreaming commits the longest prefix
on which two consecutive hypotheses agree. When the utterance ends, a full-context pass produces the final text
and keeps the committed prefix frozen. The detected language is locked per utterance after the first pass.

**Alternatives.** SimulStreaming's AlignAtt policy needs decoder cross-attention access that `whisper-rs`
does not expose; revisit if latency targets are missed. Cloud ASR is out of scope.

**Consequences.** Segment ids are stable, revisions increase monotonically, and only the unstable tail changes.
Measured with Whisper base Q8 on the dev laptop CPU (see `docs/benchmarks/`): WER 0.0 on the JFK fixture, final
text about 650 ms after the speaker stops, processing RTF 0.41. Re-decoding costs CPU proportional to
utterance length; slow machines decode bigger steps because the ASR worker drains its queue before each pass.
