//! Live translation of finalized transcript segments.
//!
//! [`Translator`] is the port; [`LlmTranslator`] implements it with the shared
//! local LLM (ADR 0005). [`TranslationWorker`] plugs into a session as a
//! segment listener and never blocks the ASR thread.

mod translator;
mod worker;

pub use translator::{LlmTranslator, TranslatedLine, TranslationError, TranslationRequest, Translator};
pub use worker::TranslationWorker;
