/// Languages written without spaces are compared character by character.
fn is_unspaced_script(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c as u32,
            0x3040..=0x30FF   // Hiragana, Katakana
            | 0x3400..=0x9FFF // CJK ideographs
            | 0x0E00..=0x0E7F // Thai
            | 0xAC00..=0xD7AF // Hangul syllables (spaced, but short words agree poorly)
        )
    })
}

/// Splits a hypothesis into comparison units (words, or characters for CJK/Thai).
pub fn words_of(text: &str) -> Vec<String> {
    if is_unspaced_script(text) {
        text.chars().filter(|c| !c.is_whitespace()).map(String::from).collect()
    } else {
        text.split_whitespace().map(str::to_owned).collect()
    }
}

fn normalized(word: &str) -> String {
    word.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn join(words: &[String]) -> String {
    let unspaced = words.iter().any(|w| is_unspaced_script(w));
    words.join(if unspaced { "" } else { " " })
}

/// LocalAgreement-2: commits the longest common prefix of the last two
/// hypotheses. Committed words are frozen and never revised.
#[derive(Debug, Default)]
pub struct LocalAgreement {
    committed: Vec<String>,
    previous: Vec<String>,
}

impl LocalAgreement {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a full-utterance hypothesis; returns `(stable, unstable)` text.
    pub fn update(&mut self, hypothesis: &str) -> (String, String) {
        let current = words_of(hypothesis);
        let start = self.committed.len();

        let agreed = self
            .previous
            .iter()
            .skip(start)
            .zip(current.iter().skip(start))
            .take_while(|(a, b)| normalized(a) == normalized(b))
            .count();
        self.committed.extend(current.iter().skip(start).take(agreed).cloned());

        let unstable = current.get(self.committed.len()..).unwrap_or_default();
        let view = (join(&self.committed), join(unstable));
        self.previous = current;
        view
    }

    /// Final text: frozen prefix plus whatever the final pass adds after it.
    pub fn finalize(&self, final_hypothesis: &str) -> String {
        let current = words_of(final_hypothesis);
        let mut words = self.committed.clone();
        words.extend(current.into_iter().skip(self.committed.len()));
        join(&words)
    }

    pub fn committed_text(&self) -> String {
        join(&self.committed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_hypothesis_is_entirely_unstable() {
        let mut agreement = LocalAgreement::new();
        assert_eq!(agreement.update("hello there"), (String::new(), "hello there".into()));
    }

    #[test]
    fn commits_common_prefix_of_consecutive_hypotheses() {
        let mut agreement = LocalAgreement::new();
        agreement.update("how are");
        assert_eq!(agreement.update("how are you doing"), ("how are".into(), "you doing".into()));
        assert_eq!(agreement.update("how are you doing today"), ("how are you doing".into(), "today".into()));
    }

    #[test]
    fn ignores_case_and_punctuation_when_comparing() {
        let mut agreement = LocalAgreement::new();
        agreement.update("Hello, world");
        let (stable, _) = agreement.update("hello world again");
        assert_eq!(stable, "hello world");
    }

    #[test]
    fn committed_words_survive_later_revisions() {
        let mut agreement = LocalAgreement::new();
        agreement.update("we meet at noon");
        agreement.update("we meet at noon tomorrow");
        let (stable, unstable) = agreement.update("we met at new tomorrow please");
        assert_eq!(stable, "we meet at noon tomorrow");
        assert_eq!(unstable, "please");
    }

    #[test]
    fn finalize_keeps_frozen_prefix() {
        let mut agreement = LocalAgreement::new();
        agreement.update("the budget is");
        agreement.update("the budget is final");
        assert_eq!(agreement.finalize("the budgets is final now"), "the budget is final now");
    }

    #[test]
    fn compares_cjk_by_character() {
        let mut agreement = LocalAgreement::new();
        agreement.update("今日は");
        let (stable, unstable) = agreement.update("今日はいい天気");
        assert_eq!(stable, "今日は");
        assert_eq!(unstable, "いい天気");
    }
}
