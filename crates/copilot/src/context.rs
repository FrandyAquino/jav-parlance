use std::collections::VecDeque;

/// Rolling window of recent final utterances, bounded by characters so the
/// prompt size (and therefore latency) stays predictable.
#[derive(Debug, Clone)]
pub struct ConversationContext {
    lines: VecDeque<String>,
    max_chars: usize,
    chars: usize,
}

impl ConversationContext {
    pub fn new(max_chars: usize) -> Self {
        Self { lines: VecDeque::new(), max_chars, chars: 0 }
    }

    pub fn push(&mut self, line: impl Into<String>) {
        let line = line.into();
        self.chars += line.chars().count();
        self.lines.push_back(line);
        while self.chars > self.max_chars && self.lines.len() > 1 {
            if let Some(evicted) = self.lines.pop_front() {
                self.chars -= evicted.chars().count();
            }
        }
    }

    pub fn transcript(&self) -> String {
        self.lines.iter().map(String::as_str).collect::<Vec<_>>().join("\n")
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_lines_beyond_budget_but_keeps_latest() {
        let mut context = ConversationContext::new(10);
        context.push("aaaa");
        context.push("bbbb");
        context.push("cccc");
        assert_eq!(context.transcript(), "bbbb\ncccc");
        context.push("a very long line that exceeds the budget");
        assert_eq!(context.len(), 1);
    }
}
