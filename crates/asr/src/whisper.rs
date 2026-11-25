use std::collections::VecDeque;
use std::path::Path;

use parlance_audio::TARGET_SAMPLE_RATE;
use parlance_audio::vad::{VadError, VoiceActivityDetector};
use parlance_domain::LanguageCode;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState, WhisperVadContext,
    WhisperVadContextParams,
};

use crate::recognizer::{AsrError, DecodeOptions, Hypothesis, SpeechRecognizer};

/// whisper.cpp refuses or hallucinates on inputs shorter than ~1 s.
const MIN_INPUT_SAMPLES: usize = TARGET_SAMPLE_RATE as usize + TARGET_SAMPLE_RATE as usize / 10;
/// Encoder positions per second of audio (1500 per 30 s window).
const AUDIO_CTX_PER_SECOND: f32 = 50.0;
const FULL_AUDIO_CTX: i32 = 1500;
/// Segments the model itself flags as probably silent are dropped.
const NO_SPEECH_THRESHOLD: f32 = 0.6;

pub fn system_info() -> &'static str {
    whisper_rs::print_system_info()
}

/// Routes whisper.cpp/ggml logging (very chatty, and the VAD logs on every
/// call) into `tracing`, where the subscriber's filter applies.
fn route_native_logs() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(whisper_rs::install_logging_hooks);
}

#[derive(Debug, Clone)]
pub struct WhisperConfig {
    pub threads: usize,
    pub use_gpu: bool,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(4, usize::from);
        // Leave headroom for capture, UI and the LLM worker.
        Self { threads: (cores / 2).clamp(1, 8), use_gpu: true }
    }
}

pub struct WhisperRecognizer {
    state: WhisperState,
    config: WhisperConfig,
    // Keeps the model alive for as long as the state that references it.
    _context: WhisperContext,
}

impl std::fmt::Debug for WhisperRecognizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhisperRecognizer").field("config", &self.config).finish()
    }
}

impl WhisperRecognizer {
    pub fn load(model_path: &Path, config: WhisperConfig) -> Result<Self, AsrError> {
        route_native_logs();
        let load_error = |reason: String| AsrError::ModelLoad { path: model_path.display().to_string(), reason };
        let path = model_path.to_str().ok_or_else(|| load_error("path is not valid UTF-8".into()))?;
        let mut params = WhisperContextParameters::default();
        params.use_gpu(config.use_gpu);
        let context = WhisperContext::new_with_params(path, params).map_err(|e| load_error(e.to_string()))?;
        let state = context.create_state().map_err(|e| load_error(e.to_string()))?;
        Ok(Self { state, config, _context: context })
    }

    fn params<'a>(&self, options: &'a DecodeOptions, samples: usize) -> FullParams<'a, 'a> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.config.threads as i32);
        params.set_translate(false);
        params.set_no_context(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(NO_SPEECH_THRESHOLD);
        params.set_language(Some(options.language.as_ref().map_or("auto", LanguageCode::as_str)));
        if options.fast {
            // Shrinking the encoder context to the actual audio length cuts
            // partial-pass latency several-fold; final passes use the full
            // window for best accuracy.
            let seconds = samples as f32 / TARGET_SAMPLE_RATE as f32;
            let ctx = ((seconds * AUDIO_CTX_PER_SECOND) as i32 + 128).min(FULL_AUDIO_CTX);
            params.set_audio_ctx(ctx);
            params.set_temperature_inc(0.0);
        }
        params
    }

    fn collect_text(&self) -> String {
        self.state
            .as_iter()
            .filter(|segment| segment.no_speech_probability() < NO_SPEECH_THRESHOLD)
            .filter_map(|segment| segment.to_str_lossy().ok().map(|s| s.trim().to_owned()))
            .filter(|text| !text.is_empty() && !is_annotation(text))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Whisper emits bracketed sound annotations such as `[Music]` or `(laughs)`.
fn is_annotation(text: &str) -> bool {
    let wrapped = |open, close| text.starts_with(open) && text.ends_with(close);
    wrapped('[', ']') || wrapped('(', ')') || wrapped('*', '*')
}

impl SpeechRecognizer for WhisperRecognizer {
    fn transcribe(&mut self, audio: &[f32], options: &DecodeOptions) -> Result<Hypothesis, AsrError> {
        let mut padded;
        let input = if audio.len() < MIN_INPUT_SAMPLES {
            padded = audio.to_vec();
            padded.resize(MIN_INPUT_SAMPLES, 0.0);
            &padded[..]
        } else {
            audio
        };
        let params = self.params(options, input.len());
        self.state.full(params, input).map_err(|e| AsrError::Inference(e.to_string()))?;

        let language = whisper_rs::get_lang_str(self.state.full_lang_id_from_state())
            .and_then(|code| LanguageCode::new(code).ok());
        Ok(Hypothesis { text: self.collect_text(), language })
    }
}

/// Silero VAD through whisper.cpp's ggml port, so no ONNX runtime is needed.
///
/// whisper.cpp resets the LSTM state on every call, so each frame is scored
/// together with a short window of preceding audio to give the model context.
pub struct SileroVad {
    context: WhisperVadContext,
    history: VecDeque<f32>,
    window: Vec<f32>,
}

const SILERO_FRAME: usize = 512;
const SILERO_CONTEXT_FRAMES: usize = 8;

impl std::fmt::Debug for SileroVad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SileroVad").finish_non_exhaustive()
    }
}

impl SileroVad {
    pub fn load(model_path: &Path) -> Result<Self, AsrError> {
        route_native_logs();
        let path = model_path.to_str().ok_or_else(|| AsrError::ModelLoad {
            path: model_path.display().to_string(),
            reason: "path is not valid UTF-8".into(),
        })?;
        let mut params = WhisperVadContextParams::new();
        params.set_n_threads(1);
        params.set_use_gpu(false);
        let context = WhisperVadContext::new(path, params)
            .map_err(|e| AsrError::ModelLoad { path: path.to_owned(), reason: e.to_string() })?;
        Ok(Self { context, history: VecDeque::new(), window: Vec::new() })
    }
}

impl VoiceActivityDetector for SileroVad {
    fn frame_len(&self) -> usize {
        SILERO_FRAME
    }

    fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, VadError> {
        self.window.clear();
        self.window.extend(self.history.iter());
        self.window.extend_from_slice(frame);
        self.context.detect_speech(&self.window).map_err(|e| VadError(e.to_string()))?;
        let probability = self.context.probabilities().last().copied().unwrap_or(0.0);

        self.history.extend(frame);
        let excess = self.history.len().saturating_sub(SILERO_FRAME * SILERO_CONTEXT_FRAMES);
        self.history.drain(..excess);
        Ok(probability)
    }

    fn reset(&mut self) {
        self.history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_annotations() {
        assert!(is_annotation("[Music]"));
        assert!(is_annotation("(laughs)"));
        assert!(!is_annotation("Hello (world)!"));
    }
}
