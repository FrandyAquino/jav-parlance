//! Accuracy metrics used by fixture tests and benchmarks.

fn normalize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word error rate: word-level Levenshtein distance divided by reference length.
pub fn word_error_rate(reference: &str, hypothesis: &str) -> f64 {
    let reference = normalize(reference);
    let hypothesis = normalize(hypothesis);
    if reference.is_empty() {
        return if hypothesis.is_empty() { 0.0 } else { 1.0 };
    }
    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, ref_word) in reference.iter().enumerate() {
        let mut current = vec![i + 1; hypothesis.len() + 1];
        for (j, hyp_word) in hypothesis.iter().enumerate() {
            let substitution = previous[j] + usize::from(ref_word != hyp_word);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[hypothesis.len()] as f64 / reference.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_zero_error() {
        assert_eq!(word_error_rate("Hello, World!", "hello world"), 0.0);
    }

    #[test]
    fn counts_substitutions_insertions_and_deletions() {
        assert_eq!(word_error_rate("a b c d", "a x c"), 0.5);
        assert_eq!(word_error_rate("a b", "a b c d"), 1.0);
    }
}
