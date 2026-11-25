//! Local language-model access.
//!
//! - [`LanguageModel`] is the port used by translation and the copilot.
//! - [`LlamaServer`] supervises a `llama-server` child process (ADR 0004).
//! - [`prompt`] isolates untrusted transcript text inside prompts.

mod client;
#[cfg(windows)]
mod job;
pub mod prompt;
mod server;
mod sse;

use async_trait::async_trait;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

pub use client::ServerModel;
pub use server::{LlamaServer, ServerConfig, ServerStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("the language model is not ready: {0}")]
    Unavailable(String),
    #[error("language model request failed: {0}")]
    Request(String),
    #[error("generation cancelled")]
    Cancelled,
}

/// Token callback. A type alias keeps `async_trait` from tying the `&str`
/// lifetime to the future, which would forbid passing temporaries.
pub type OnToken<'a> = dyn Fn(&str) + Send + Sync + 'a;

/// Streams tokens through `on_token` and returns the full completion.
#[async_trait]
pub trait LanguageModel: Send + Sync {
    async fn generate(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
        on_token: &OnToken<'_>,
    ) -> Result<String, LlmError>;
}
