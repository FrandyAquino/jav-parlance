# 0002 — whisper.cpp ASR with LocalAgreement-2 streaming

**Context.** We need low-latency, revisable transcripts in ~100 languages, offline, on CPU-only laptops as
well as GPUs. Whisper is batch-oriented (30 s windows).

**Decision.** Use whisper.cpp through `whisper-rs`. Utterances are bounded by VAD (at most 20 s), so no
window trimming is needed. While an utterance is open it is re-decoded every ~500 ms of new audio with a reduced
`audio_ctx` (fast partial pass). The LocalAgreement-2 policy from WhisperStreaming commits the longest prefix
on which two consecutive hypotheses agree. When the utterance ends, a full-context pass produces the final text
and keeps the committed prefix frozen while the language remains consistent. In Auto mode every pass can
re-detect the language; a corrected language resets agreement so an early wrong-language prefix cannot survive.
Only a source language explicitly chosen by the user is forced.
Fast passes retain whisper.cpp's temperature fallback. Disabling it can return failed low-entropy greedy
decodes as long repetition loops, which was reproduced with a real music preview.

**Alternatives.** SimulStreaming's AlignAtt policy needs decoder cross-attention access that `whisper-rs`
does not expose; revisit if latency targets are missed. Cloud ASR is out of scope.

**Consequences.** Segment ids are stable, revisions increase monotonically, and only the unstable tail changes unless Auto corrects the detected language.
Measured with Whisper base Q8 on the dev laptop CPU (see `docs/benchmarks/`): WER 0.0 on the JFK fixture, final
text about 650 ms after the speaker stops, processing RTF 0.41. Re-decoding costs CPU proportional to
utterance length; slow machines decode bigger steps because the ASR worker drains its queue before each pass.

**Correction (2026-11-25).** A 500 ms prefix of the Spanish fixture was detected as Arabic, while 600 ms
and the complete utterance were Spanish. Locking the first guess forced all later decodes into the wrong
language. Regression coverage includes that real prefix and a scripted recognizer that changes its estimate.
Music/vocals can still confuse Whisper; automatic detection is not a guarantee of lyric accuracy.
