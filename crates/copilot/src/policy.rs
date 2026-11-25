use std::time::Duration;

/// Tuning knobs for when a detected question deserves a suggested reply.
#[derive(Debug, Clone)]
pub struct SuggestionPolicy {
    /// Minimum time between suggestions so rapid back-and-forth does not
    /// flood the panel (and the LLM) with replies nobody can read.
    pub cooldown: Duration,
    /// Questions shorter than this are usually fillers ("you know?").
    pub min_chars: usize,
}

impl Default for SuggestionPolicy {
    fn default() -> Self {
        Self { cooldown: Duration::from_secs(4), min_chars: 12 }
    }
}

/// Decides whether a detected question should trigger a new suggestion.
///
/// `since_last` is the time since the previous suggestion started, if any.
///
/// TODO(product): this is the main lever on how "chatty" the copilot feels.
/// Ideas worth trying: only answer questions that mention the user's name or
/// use second-person forms ("you", "tú", "vous"), skip rhetorical tags
/// ("right?", "¿verdad?"), or always allow a question that arrives after a
/// long silence even inside the cooldown.
pub fn should_suggest(question: &str, policy: &SuggestionPolicy, since_last: Option<Duration>) -> bool {
    let long_enough = question.trim().chars().count() >= policy.min_chars;
    let cooled_down = since_last.is_none_or(|elapsed| elapsed >= policy.cooldown);
    long_enough && cooled_down
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_cooldown_and_length() {
        let policy = SuggestionPolicy::default();
        assert!(should_suggest("Can you send the numbers?", &policy, None));
        assert!(!should_suggest("Can you send the numbers?", &policy, Some(Duration::from_secs(1))));
        assert!(should_suggest("Can you send the numbers?", &policy, Some(Duration::from_secs(5))));
        assert!(!should_suggest("you know?", &policy, None));
    }
}
