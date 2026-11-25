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
        let base = directories::BaseDirs::new().map(|dirs| dirs.data_local_dir().to_owned()).unwrap_or_default();
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

#[test]
fn auto_detects_spanish_from_complete_utterances() {
    let models = models_dir();
    let (asr_model, vad_model) = (models.join("ggml-base-q8_0.bin"), models.join("ggml-silero-v5.1.2.bin"));
    if !asr_model.exists() || !vad_model.exists() {
        eprintln!("skipping: models not found in {}", models.display());
        return;
    }
    let recognizer = WhisperRecognizer::load(&asr_model, WhisperConfig::default()).unwrap();
    let vad = SileroVad::load(&vad_model).unwrap();
    let mut gate = VadGate::new(Box::new(vad), GateConfig::default());
    let mut transcriber = StreamingTranscriber::new(recognizer, StreamingConfig::default());
    let mut updates = Vec::new();
    for chunk in read_fixture("meeting_question_es.wav").chunks(1_600) {
        for event in gate.process(chunk).unwrap() {
            transcriber.push(event);
        }
        updates.extend(transcriber.poll().unwrap());
    }
    for event in gate.flush() {
        transcriber.push(event);
    }
    updates.extend(transcriber.flush().unwrap());
    let finals: Vec<_> = updates.iter().filter(|s| s.is_final()).collect();
    for segment in &finals {
        eprintln!("{} {:?}: {}", segment.start_ms, segment.language, segment.text());
    }
    assert_eq!(finals.len(), 3);
    assert!(
        finals.iter().all(|s| s.language.as_ref().map(|l| l.as_str()) == Some("es")),
        "Spanish speech must not stay locked to the first short guess"
    );
}

#[test]
fn spanish_final_recovers_after_an_ambiguous_half_second() {
    let model = models_dir().join("ggml-base-q8_0.bin");
    if !model.exists() {
        eprintln!("skipping: Whisper model not installed");
        return;
    }
    let recognizer = WhisperRecognizer::load(&model, WhisperConfig::default()).unwrap();
    let mut transcriber = StreamingTranscriber::new(recognizer, StreamingConfig::default());
    let audio = read_fixture("meeting_question_es.wav");
    // The real VAD starts the fixture's second utterance at 1912 ms.
    let start = 1912 * 16;
    transcriber.push(parlance_audio::vad::GateEvent::SpeechStarted { start_sample: start as u64 });
    transcriber.push(parlance_audio::vad::GateEvent::Speech(audio[start..start + 8_000].to_vec()));
    let early = transcriber.poll().unwrap();
    eprintln!("short hypothesis: {:?}", early.iter().map(|s| (&s.language, s.text())).collect::<Vec<_>>());
    transcriber.push(parlance_audio::vad::GateEvent::Speech(audio[start + 8_000..start + 48_000].to_vec()));
    let final_update = transcriber.flush().unwrap();
    assert_eq!(final_update.len(), 1);
    assert_eq!(final_update[0].language.as_ref().unwrap().as_str(), "es");
    assert!(final_update[0].text().to_lowercase().contains("migración"), "{}", final_update[0].text());
}

#[test]
fn music_prefixes_do_not_expand_into_decoder_loops() {
    use parlance_asr::{AsrError, DecodeOptions, Hypothesis, SpeechRecognizer};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let Some(fixture) = std::env::var_os("PARLANCE_MUSIC_FIXTURE") else {
        eprintln!("skipping: set PARLANCE_MUSIC_FIXTURE to a licensed 16 kHz mono music WAV");
        return;
    };
    let models = models_dir();
    let model = models.join("ggml-base-q8_0.bin");
    let vad = models.join("ggml-silero-v5.1.2.bin");
    if !model.exists() || !vad.exists() {
        eprintln!("skipping: models not installed");
        return;
    }
    struct Bounded(WhisperRecognizer, Arc<AtomicUsize>);
    impl SpeechRecognizer for Bounded {
        fn transcribe(&mut self, audio: &[f32], options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
            let h = self.0.transcribe(audio, options)?;
            if options.fast {
                self.1.fetch_add(1, Ordering::Relaxed);
            }
            // This English test clip cannot contain twelve spoken words/second, plus sixteen words of slack.
            let maximum = audio.len() * 12 / 16_000 + 16;
            assert!(
                h.text.split_whitespace().count() <= maximum,
                "decoder expanded {} ms into {} words",
                audio.len() * 1000 / 16_000,
                h.text.split_whitespace().count()
            );
            Ok(h)
        }
    }
    let mut reader = hound::WavReader::open(fixture).unwrap();
    assert_eq!(reader.spec().sample_rate, 16_000);
    assert_eq!(reader.spec().channels, 1);
    let audio: Vec<f32> = reader.samples::<i16>().map(|s| f32::from(s.unwrap()) / 32_768.0).collect();
    let mut gate = VadGate::new(Box::new(SileroVad::load(&vad).unwrap()), GateConfig::default());
    let fast_calls = Arc::new(AtomicUsize::new(0));
    let mut stream = StreamingTranscriber::new(
        Bounded(WhisperRecognizer::load(&model, WhisperConfig::default()).unwrap(), fast_calls.clone()),
        StreamingConfig::default(),
    );
    let mut final_words = 0;
    for chunk in audio.chunks(1600) {
        for event in gate.process(chunk).unwrap() {
            stream.push(event);
        }
        for segment in stream.poll().unwrap() {
            if segment.is_final() {
                final_words += segment.text().split_whitespace().count();
            }
        }
    }
    for event in gate.flush() {
        stream.push(event);
    }
    for segment in stream.flush().unwrap() {
        if segment.is_final() {
            final_words += segment.text().split_whitespace().count();
        }
    }
    assert!(fast_calls.load(Ordering::Relaxed) > 0, "fixture must exercise fast decodes");
    assert!(final_words > 0, "suppressing all recognition is not a fix");
}
