//! Incremental parser for OpenAI-style server-sent events.

use serde::Deserialize;

#[derive(Debug, PartialEq, Eq)]
pub enum SseItem {
    Token(String),
    Done,
    Failed(String),
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct Delta {
    content: Option<String>,
}

/// Accepts arbitrary byte chunks (events may be split anywhere) and yields
/// completed items.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseItem> {
        self.buffer.extend_from_slice(bytes);
        let mut items = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=newline).collect();
            let line = String::from_utf8_lossy(&line);
            items.extend(parse_line(line.trim_end()));
        }
        items
    }
}

fn parse_line(line: &str) -> Vec<SseItem> {
    let Some(data) = line.strip_prefix("data:") else {
        return Vec::new();
    };
    let data = data.trim_start();
    if data == "[DONE]" {
        return vec![SseItem::Done];
    }
    let Ok(chunk) = serde_json::from_str::<Chunk>(data) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for choice in chunk.choices {
        if let Some(content) = choice.delta.content.filter(|c| !c.is_empty()) {
            items.push(SseItem::Token(content));
        }
        if let Some(reason) = choice.finish_reason {
            if reason == "stop" {
                items.push(SseItem::Done);
            } else {
                items.push(SseItem::Failed(format!(
                    "Generation ended before a complete reply ({reason}). Reduce the context or retry."
                )));
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(content: &str) -> String {
        format!("data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n", serde_json::to_string(content).unwrap())
    }

    #[test]
    fn parses_tokens_and_done() {
        let mut parser = SseParser::default();
        let stream = format!("{}{}data: [DONE]\n\n", event("Hola"), event(" mundo"));
        assert_eq!(
            parser.push(stream.as_bytes()),
            [SseItem::Token("Hola".into()), SseItem::Token(" mundo".into()), SseItem::Done]
        );
    }

    #[test]
    fn handles_events_split_across_chunks_including_utf8() {
        let mut parser = SseParser::default();
        let bytes = event("café").into_bytes();
        let (head, tail) = bytes.split_at(bytes.len() - 6);
        assert!(parser.push(head).is_empty());
        assert_eq!(parser.push(tail), [SseItem::Token("café".into())]);
    }

    #[test]
    fn ignores_comments_role_chunks_and_garbage() {
        let mut parser = SseParser::default();
        let stream = ": keep-alive\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\ndata: nonsense\n";
        assert!(parser.push(stream.as_bytes()).is_empty());
    }

    #[test]
    fn reports_truncated_completion_even_without_text() {
        let mut parser = SseParser::default();
        let items = parser.push(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n");
        assert!(!items.is_empty(), "token-limit termination must be reported, not silently accepted");
    }
}
