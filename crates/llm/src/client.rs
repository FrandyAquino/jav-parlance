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
/// CPU inference can be slow; a complete local generation still has a three-minute ceiling.
const GENERATION_TIMEOUT: Duration = Duration::from_secs(180);
/// Allow slow prompt evaluation and token generation, but end a minute of HTTP/SSE silence.
const GENERATION_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

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
        cancellable(cancel.clone(), async {
            let endpoint = self.server.endpoint(READY_WAIT).await?;
            request_completion(&self.http, &endpoint, request, &cancel, on_token).await
        })
        .await
    }
}

async fn cancellable<T>(
    cancel: CancellationToken,
    operation: impl std::future::Future<Output = Result<T, LlmError>>,
) -> Result<T, LlmError> {
    tokio::select! { biased; _ = cancel.cancelled() => Err(LlmError::Cancelled), result = operation => result }
}

async fn request_completion(
    http: &reqwest::Client,
    endpoint: &crate::server::Endpoint,
    request: ChatRequest,
    cancel: &CancellationToken,
    on_token: &OnToken<'_>,
) -> Result<String, LlmError> {
    let completion = async {
        let response = tokio::time::timeout(
            GENERATION_IDLE_TIMEOUT,
            http.post(format!("{}/v1/chat/completions", endpoint.base_url))
                .bearer_auth(&endpoint.api_key)
                .json(&body(&request))
                .send(),
        )
        .await
        .map_err(|_| LlmError::Request("The model did not respond in time. Try again.".into()))?
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| LlmError::Request(e.to_string()))?;
        let mut stream = response.bytes_stream();
        let mut parser = SseParser::default();
        let mut output = String::new();
        loop {
            let chunk = tokio::time::timeout(GENERATION_IDLE_TIMEOUT, stream.next())
                .await
                .map_err(|_| LlmError::Request("The model stream stalled. Try again.".into()))?;
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(|e| LlmError::Request(e.to_string()))?;
            for item in parser.push(&chunk) {
                if cancel.is_cancelled() {
                    return Err(LlmError::Cancelled);
                }
                match item {
                    SseItem::Token(token) => {
                        on_token(&token);
                        output.push_str(&token);
                    }
                    SseItem::Done => return Ok(output),
                    SseItem::Failed(error) => return Err(LlmError::Request(error)),
                }
            }
        }
        Err(LlmError::Request("The model stream ended before completion. Try again.".into()))
    };
    tokio::time::timeout(GENERATION_TIMEOUT, completion)
        .await
        .map_err(|_| LlmError::Request("The model generation exceeded its time limit. Try again.".into()))?
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

#[cfg(test)]
mod streaming_tests {
    use super::*;
    use crate::server::Endpoint;
    use crate::{ChatMessage, ServerConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    // A controlled loopback peer can hold headers or keep an SSE response open.
    async fn controlled_peer()
    -> (Endpoint, tokio::sync::mpsc::UnboundedSender<&'static str>, oneshot::Receiver<()>, tokio::task::JoinHandle<()>)
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint =
            Endpoint { base_url: format!("http://{}", listener.local_addr().unwrap()), api_key: "test".into() };
        let (send, mut receive) = tokio::sync::mpsc::unbounded_channel::<&'static str>();
        let (entered, wait) = oneshot::channel();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            let mut received = Vec::new();
            loop {
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                received.extend_from_slice(&buffer[..count]);
                assert!(received.len() < 65_536);
                if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&received[..end]).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if received.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            entered.send(()).unwrap();
            while let Some(bytes) = receive.recv().await {
                if socket.write_all(bytes.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        (endpoint, send, wait, peer)
    }

    const OPEN_STREAM: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n";
    const MORE_TOKEN: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"more\"}}]}\n\n";

    async fn assert_stall_is_bounded(streaming: bool) {
        let (endpoint, send, entered, peer) = controlled_peer().await;
        let (tokens, mut received) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            request_completion(&reqwest::Client::new(), &endpoint, request(), &CancellationToken::new(), &|_| {
                tokens.send(()).unwrap();
            })
            .await
        });
        entered.await.unwrap();
        if streaming {
            send.send(OPEN_STREAM).unwrap();
            received.recv().await.unwrap();
        }
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(65)).await;
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("stalled generation must end without waiting for the server")
            .unwrap();
        assert!(matches!(result, Err(LlmError::Request(_))));
        drop(send);
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn generation_deadline_bounds_pending_headers() {
        assert_stall_is_bounded(false).await;
    }

    #[tokio::test]
    async fn generation_inactivity_bounds_stalled_sse() {
        assert_stall_is_bounded(true).await;
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_stalled_sse_stream() {
        let (endpoint, send, entered, peer) = controlled_peer().await;
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let (tokens, mut received) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            cancellable(
                token.clone(),
                request_completion(&reqwest::Client::new(), &endpoint, request(), &token, &|_| {
                    tokens.send(()).unwrap();
                }),
            )
            .await
        });
        entered.await.unwrap();
        send.send(OPEN_STREAM).unwrap();
        received.recv().await.unwrap();
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap();
        assert!(matches!(result, Err(LlmError::Cancelled)));
        assert!(received.recv().await.is_none());
        drop(send);
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn generation_total_deadline_bounds_an_active_stream() {
        let (endpoint, send, entered, peer) = controlled_peer().await;
        let (tokens, mut received) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            request_completion(&reqwest::Client::new(), &endpoint, request(), &CancellationToken::new(), &|_| {
                tokens.send(()).unwrap();
            })
            .await
        });
        entered.await.unwrap();
        send.send(OPEN_STREAM).unwrap();
        received.recv().await.unwrap();
        // Resume time for loopback I/O, then advance only after each token was consumed.
        for _ in 0..5 {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(30)).await;
            tokio::time::resume();
            send.send(MORE_TOKEN).unwrap();
            tokio::time::timeout(Duration::from_secs(2), received.recv()).await.unwrap().unwrap();
        }
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(35)).await;
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("continuous tokens must not extend the total generation budget")
            .unwrap();
        assert!(matches!(result, Err(LlmError::Request(_))));
        drop(send);
        peer.await.unwrap();
    }

    fn request() -> ChatRequest {
        ChatRequest { messages: vec![ChatMessage::user("hi")], max_tokens: 160, temperature: 0.0 }
    }
    async fn serve(body: &'static str) -> (Endpoint, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint =
            Endpoint { base_url: format!("http://{}", listener.local_addr().unwrap()), api_key: "test".into() };
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 8192];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        (endpoint, task)
    }
    async fn completion(body: &'static str) -> Result<String, LlmError> {
        let (endpoint, server) = serve(body).await;
        let result =
            request_completion(&reqwest::Client::new(), &endpoint, request(), &CancellationToken::new(), &|_| {}).await;
        server.await.unwrap();
        result
    }
    #[tokio::test]
    async fn incomplete_eof_and_token_limit_fail_while_stop_and_done_succeed() {
        assert!(completion("data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n").await.is_err());
        assert!(completion("data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n").await.is_err());
        assert_eq!(
            completion("data: {\"choices\":[{\"delta\":{\"content\":\"complete\"},\"finish_reason\":\"stop\"}]}\n\n")
                .await
                .unwrap(),
            "complete"
        );
        assert_eq!(
            completion("data: {\"choices\":[{\"delta\":{\"content\":\"complete\"}}]}\n\ndata: [DONE]\n\n")
                .await
                .unwrap(),
            "complete"
        );
    }
    #[tokio::test]
    async fn cancellation_before_readiness_returns_without_waiting_for_server() {
        let server = LlamaServer::spawn(ServerConfig::new(
            "/nonexistent/copilot-test-server".into(),
            "/nonexistent/model".into(),
        ));
        let model = ServerModel::new(server.clone());
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            model.generate(request(), cancel, &|_| panic!("cancelled request emitted a token")),
        )
        .await
        .unwrap();
        assert!(matches!(result, Err(LlmError::Cancelled)));
        server.shutdown_and_wait().await;
    }
    #[tokio::test]
    async fn cancellation_during_http_headers_drops_request() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint =
            Endpoint { base_url: format!("http://{}", listener.local_addr().unwrap()), api_key: "test".into() };
        let (entered, wait) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            let mut received = Vec::new();
            loop {
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0, "request ended before its complete body");
                received.extend_from_slice(&buffer[..count]);
                assert!(received.len() < 65_536, "test request is bounded");
                if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&received[..end]).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if received.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            entered.send(()).unwrap();
            assert_eq!(socket.read(&mut buffer).await.unwrap(), 0, "cancel closes the HTTP connection");
        });
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            cancellable(
                token.clone(),
                request_completion(&reqwest::Client::new(), &endpoint, request(), &token, &|_| {}),
            )
            .await
        });
        wait.await.unwrap();
        cancel.cancel();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap(),
            Err(LlmError::Cancelled)
        ));
        tokio::time::timeout(Duration::from_secs(1), server).await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn cancellation_during_streaming_blocks_following_tokens() {
        let (endpoint,server)=serve("data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"late\"}}]}\n\ndata: [DONE]\n\n").await;
        let cancel = CancellationToken::new();
        let seen = std::sync::Mutex::new(String::new());
        let callback = |token: &str| {
            seen.lock().unwrap().push_str(token);
            cancel.cancel();
        };
        let result = cancellable(
            cancel.clone(),
            request_completion(&reqwest::Client::new(), &endpoint, request(), &cancel, &callback),
        )
        .await;
        assert!(matches!(result, Err(LlmError::Cancelled)));
        assert_eq!(*seen.lock().unwrap(), "first");
        server.await.unwrap();
    }
}
