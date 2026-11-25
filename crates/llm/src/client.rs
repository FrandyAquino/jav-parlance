use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::server::LlamaServer;
use crate::sse::{SseItem, SseParser};
use crate::{ChatRequest, LanguageModel, LlmError, OnToken};

const READY_WAIT: Duration = Duration::from_secs(2);

/// [`LanguageModel`] backed by the supervised `llama-server`.
#[derive(Debug, Clone)]
pub struct ServerModel {
    server: Arc<LlamaServer>,
    http: reqwest::Client,
}

impl ServerModel {
    pub fn new(server: Arc<LlamaServer>) -> Self {
        Self { server, http: reqwest::Client::new() }
    }
}

fn body(request: &ChatRequest) -> serde_json::Value {
    json!({
        "messages": request.messages,
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
        "stream": true,
        "cache_prompt": true,
        // Qwen3.x reasons by default; replies must stream immediately.
        "chat_template_kwargs": { "enable_thinking": false },
    })
}

#[async_trait]
impl LanguageModel for ServerModel {
    async fn generate(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
        on_token: &OnToken<'_>,
    ) -> Result<String, LlmError> {
        let endpoint = self.server.endpoint(READY_WAIT).await?;
        let response = self
            .http
            .post(format!("{}/v1/chat/completions", endpoint.base_url))
            .bearer_auth(&endpoint.api_key)
            .json(&body(&request))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| LlmError::Request(e.to_string()))?;

        let mut stream = response.bytes_stream();
        let mut parser = SseParser::default();
        let mut output = String::new();
        loop {
            let chunk = tokio::select! {
                // Dropping the response closes the connection, which makes
                // llama-server abort the generation and free the slot.
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                next = stream.next() => match next {
                    Some(chunk) => chunk.map_err(|e| LlmError::Request(e.to_string()))?,
                    None => return Ok(output),
                },
            };
            for item in parser.push(&chunk) {
                match item {
                    SseItem::Token(token) => {
                        on_token(&token);
                        output.push_str(&token);
                    }
                    SseItem::Done => return Ok(output),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChatMessage;

    #[test]
    fn disables_reasoning_and_streams() {
        let request = ChatRequest { messages: vec![ChatMessage::user("hi")], max_tokens: 8, temperature: 0.0 };
        let body = body(&request);
        assert_eq!(body["stream"], true);
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(body["messages"][0]["role"], "user");
    }
}
