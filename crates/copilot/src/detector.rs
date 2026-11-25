//! Lightweight, offline question detection.
//!
//! Whisper usually punctuates questions, but not reliably, so a trailing
//! question mark is combined with language-specific interrogative openers.

const MIN_WORDS: usize = 3;

/// Lowercase openers that start a question in common meeting languages.
/// Matched against the first word(s) of the sentence.
const OPENERS: &[(&str, &[&str])] = &[
    (
        "en",
        &[
            "what", "how", "why", "when", "where", "who", "which", "whose", "can", "could", "would", "will", "should",
            "do", "does", "did", "is", "are", "was", "were", "have", "has", "shall", "may",
        ],
    ),
    (
        "es",
        &[
            "qué", "que", "cómo", "como", "por qué", "cuándo", "dónde", "quién", "quiénes", "cuál", "cuáles", "cuánto",
            "cuántos", "puedes", "puede", "podrías", "podría", "tienes", "tiene", "hay", "sabes", "es",
        ],
    ),
    ("pt", &["o que", "como", "por que", "quando", "onde", "quem", "qual", "quais", "quanto", "pode", "você"]),
    (
        "fr",
        &[
            "est-ce",
            "qu'est-ce",
            "comment",
            "pourquoi",
            "quand",
            "où",
            "qui",
            "quel",
            "quelle",
            "quels",
            "combien",
            "peux-tu",
            "pouvez-vous",
            "pourriez-vous",
            "avez-vous",
            "as-tu",
        ],
    ),
    (
        "de",
        &[
            "was", "wie", "warum", "wann", "wo", "wer", "welche", "welcher", "wieviel", "können", "kannst", "könnten",
            "hast", "haben", "ist", "sind", "gibt",
        ],
    ),
    ("it", &["che", "cosa", "come", "perché", "quando", "dove", "chi", "quale", "quanto", "puoi", "può", "potresti"]),
];

/// Requests addressed to someone are questions even when ASR punctuates them
/// as statements (Whisper often drops the `?`, especially in Spanish).
const REQUESTS: &[&str] = &[
    "can you",
    "could you",
    "would you",
    "will you",
    "do you",
    "did you",
    "are you",
    "have you",
    "puedes",
    "podrías",
    "podría",
    "podrían",
    "pueden",
    "tienes",
    "sabes",
    "me puedes",
    "me podrías",
    "pode",
    "poderia",
    "você pode",
    "peux-tu",
    "pouvez-vous",
    "pourriez-vous",
    "kannst du",
    "können sie",
    "könnten sie",
    "puoi",
    "potresti",
];

/// Whether `text` starts with `phrase` as whole words ("what" must not match "whatever").
fn starts_with_phrase(text: &str, phrase: &str) -> bool {
    let lowered = text.trim_start_matches(['¿', '¡', '"', '\'']).to_lowercase();
    lowered.strip_prefix(phrase).is_some_and(|rest| rest.starts_with(|c: char| !c.is_alphanumeric()))
}

fn is_request(text: &str) -> bool {
    REQUESTS.iter().any(|phrase| starts_with_phrase(text, phrase))
}

fn opens_with_interrogative(text: &str, language: Option<&str>) -> bool {
    OPENERS
        .iter()
        .filter(|(code, _)| language.is_none_or(|lang| lang == *code))
        .flat_map(|(_, openers)| openers.iter())
        .any(|opener| starts_with_phrase(text, opener))
}

/// Whether `text` reads as a question. `language` (ISO 639-1) narrows the
/// opener list; with `None` all known languages are considered.
pub fn is_question(text: &str, language: Option<&str>) -> bool {
    let text = text.trim();
    if text.split_whitespace().count() < MIN_WORDS {
        return false;
    }
    let punctuated = text.ends_with('?') || text.ends_with('？') || text.starts_with('¿');
    // Openers alone are ambiguous ("What a day."), so they only count when
    // the sentence is not explicitly terminated as a statement.
    let unterminated_statement = !text.ends_with('.') && !text.ends_with('!');
    punctuated || is_request(text) || (unterminated_statement && opens_with_interrogative(text, language))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_punctuated_questions_in_any_language() {
        assert!(is_question("Can you share the roadmap?", Some("en")));
        assert!(is_question("¿Cuándo terminamos el proyecto", Some("es")));
        assert!(is_question("この件はいつ終わりますか？ 来週 ですか？", None));
    }

    #[test]
    fn detects_unpunctuated_questions_by_opener() {
        assert!(is_question("how does the billing migration work", Some("en")));
        assert!(is_question("qu'est-ce que tu en penses", Some("fr")));
        assert!(is_question("wie lange dauert das noch", Some("de")));
    }

    #[test]
    fn detects_requests_punctuated_as_statements() {
        assert!(is_question("Podría enviarme los números del trimestre antes del viernes.", Some("es")));
        assert!(is_question("Could you share your screen.", Some("en")));
    }

    #[test]
    fn ignores_statements_and_short_fragments() {
        assert!(!is_question("We shipped the release yesterday.", Some("en")));
        assert!(!is_question("What a great quarter.", Some("en")));
        assert!(!is_question("Right?", Some("en")));
        assert!(!is_question("whatever happens we ship", Some("en")));
    }
}
