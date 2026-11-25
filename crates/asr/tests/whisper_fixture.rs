//! Real-model streaming test over a recorded fixture.
//!
//! Requires `ggml-base-q8_0.bin` and `ggml-silero-v5.1.2.bin` in
//! `PARLANCE_MODELS_DIR` (defaults to the app's model directory). The test is
//! skipped, not failed, when models are absent so CI without downloads stays green.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers may panic
#![cfg(feature = "whisper")]

use std::path::PathBuf;
use std::time::Instant;

use parlance_asr::metrics::word_error_rate;
use parlance_asr::{SileroVad, StreamingConfig, StreamingTranscriber, WhisperConfig, WhisperRecognizer};
use parlance_audio::vad::{GateConfig, VadGate};
use parlance_domain::Stability;

const JFK_REFERENCE: &str = "And so my fellow Americans, ask not what your country can do for you, \
                             ask what you can do for your country.";

fn models_dir() -> PathBuf {
    std::env::var_os("PARLANCE_MODELS_DIR").map(PathBuf::from).unwrap_or_else(|| {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .unwrap_or_default();
        base.join("parlance").join("models")
    })
}

fn read_fixture(name: &str) -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio").join(name);
    let mut reader = hound::WavReader::open(path).expect("fixture wav");
    assert_eq!(reader.spec().sample_rate, 16_000);
    reader.samples::<i16>().map(|s| f32::from(s.expect("sample")) / 32_768.0).collect()
}

#[test]
fn streams_jfk_with_low_word_error_rate() {
    let models = models_dir();
    let (asr_model, vad_model) = (models.join("ggml-base-q8_0.bin"), models.join("ggml-silero-v5.1.2.bin"));
    if !asr_model.exists() || !vad_model.exists() {
        eprintln!("skipping: models not found in {}", models.display());
        return;
    }

    let recognizer = WhisperRecognizer::load(&asr_model, WhisperConfig::default()).expect("load whisper");
    let vad = SileroVad::load(&vad_model).expect("load vad");
    let mut gate = VadGate::new(Box::new(vad), GateConfig::default());
    let mut transcriber = StreamingTranscriber::new(recognizer, StreamingConfig::default());

    let audio = read_fixture("jfk.wav");
    let started = Instant::now();
    let mut updates = Vec::new();
    for chunk in audio.chunks(1_600) {
        for event in gate.process(chunk).expect("vad") {
            transcriber.push(event);
        }
        updates.extend(transcriber.poll().expect("poll"));
    }
    for event in gate.flush() {
        transcriber.push(event);
    }
    updates.extend(transcriber.flush().expect("flush"));
    let elapsed = started.elapsed().as_secs_f64();

    let partials = updates.iter().filter(|s| s.stability == Stability::Partial).count();
    let transcript: Vec<String> = updates.iter().filter(|s| s.is_final()).map(|s| s.text()).collect();
    let transcript = transcript.join(" ");
    let wer = word_error_rate(JFK_REFERENCE, &transcript);
    let rtf = elapsed / (audio.len() as f64 / 16_000.0);
    eprintln!("transcript: {transcript}\nWER {wer:.3}  RTF {rtf:.2}  partials {partials}");

    assert!(partials > 0, "expected partial hypotheses while speaking");
    assert!(wer < 0.15, "WER too high: {wer}");
    assert!(updates.iter().filter(|s| s.is_final()).all(|s| s.language.as_ref().map(|l| l.as_str()) == Some("en")));
}
