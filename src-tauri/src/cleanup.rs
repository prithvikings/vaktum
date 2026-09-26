pub fn clean_transcript(input: &str) -> String {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.is_empty() {
        return String::new();
    }

    let tokens = normalized
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    normalize_punctuation_spacing(&tokens)
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
    fn preserves_repeated_words() {
        assert_eq!(
            clean_transcript("hello hello world"),
            "hello hello world"
        );
    }

    #[test]
    fn preserves_intentional_repeated_words() {
        assert_eq!(
            clean_transcript("very very important"),
            "very very important"
        );
        assert_eq!(clean_transcript("bye bye"), "bye bye");
        assert_eq!(
            clean_transcript("no no don't do that"),
            "no no don't do that"
        );
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
