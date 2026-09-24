pub fn clean_transcript(input: &str) -> String {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.is_empty() {
        return String::new();
    }

    let mut tokens = normalized.split_whitespace();
    let mut output = Vec::with_capacity(normalized.len());

    while let Some(token) = tokens.next() {
        let mut cleaned = token.to_owned();

        while let Some(next) = tokens.next() {
            if is_punctuation_token(next) {
                cleaned.push_str(next);
                continue;
            }

            if should_remove_repeated_word(&cleaned, next) {
                continue;
            }

            output.push(cleaned);
            cleaned = next.to_owned();
            break;
        }

        output.push(cleaned);
    }

    normalize_punctuation_spacing(&output)
}

fn should_remove_repeated_word(previous: &str, current: &str) -> bool {
    let previous_word = normalize_word(previous);
    let current_word = normalize_word(current);

    !previous_word.is_empty()
        && previous_word == current_word
        && !intentional_repetition(&current_word)
}

fn intentional_repetition(word: &str) -> bool {
    matches!(word, "very" | "bye")
}

fn normalize_word(token: &str) -> String {
    token
        .trim_matches(|character: char| character.is_ascii_punctuation())
        .to_ascii_lowercase()
}

fn is_punctuation_token(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|character| character.is_ascii_punctuation())
}

fn normalize_punctuation_spacing(tokens: &[String]) -> String {
    let mut result = String::new();

    for token in tokens {
        let is_punctuation = is_punctuation_token(token);

        if is_punctuation {
            result = result.trim_end().to_owned();
            result.push_str(token);
        } else {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(token);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::clean_transcript;

    #[test]
    fn trims_leading_and_trailing_whitespace() {
        assert_eq!(clean_transcript("   hello world   "), "hello world");
    }

    #[test]
    fn collapses_repeated_whitespace() {
        assert_eq!(clean_transcript("hello    world"), "hello world");
    }

    #[test]
    fn normalizes_punctuation_spacing() {
        assert_eq!(
            clean_transcript("hello , world ."),
            "hello, world."
        );
    }

    #[test]
    fn removes_obvious_repeated_word() {
        assert_eq!(clean_transcript("hello hello world"), "hello world");
    }

    #[test]
    fn preserves_intentional_repeated_words() {
        assert_eq!(
            clean_transcript("very very important"),
            "very very important"
        );
        assert_eq!(clean_transcript("bye bye"), "bye bye");
    }

    #[test]
    fn empty_or_whitespace_only_input_returns_empty() {
        assert_eq!(clean_transcript("   \n\t  "), "");
    }

    #[test]
    fn preserves_technical_terminology() {
        assert_eq!(
            clean_transcript("React   TypeScript   Tauri"),
            "React TypeScript Tauri"
        );
    }

    #[test]
    fn preserves_mixed_punctuation_and_text() {
        assert_eq!(
            clean_transcript("Use React , TypeScript and Tauri ."),
            "Use React, TypeScript and Tauri."
        );
    }

    #[test]
    fn preserves_non_repeated_words_and_case() {
        assert_eq!(
            clean_transcript("HTTP API React TypeScript"),
            "HTTP API React TypeScript"
        );
    }
}
