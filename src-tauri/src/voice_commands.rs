use crate::insertion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceCommand {
    NewLine,
    NewParagraph,
    DeleteLastWord,
}

impl VoiceCommand {
    fn phrase(self) -> &'static str {
        match self {
            Self::NewLine => "new line",
            Self::NewParagraph => "new paragraph",
            Self::DeleteLastWord => "delete last word",
        }
    }
}

/// Parses only the explicit command phrases supported by P2-M3-M1.
pub fn parse_command(transcript: &str) -> Option<VoiceCommand> {
    let normalized = transcript.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }

    [
        VoiceCommand::NewLine,
        VoiceCommand::NewParagraph,
        VoiceCommand::DeleteLastWord,
    ]
    .into_iter()
    .find(|command| normalized.eq_ignore_ascii_case(command.phrase()))
}

/// Returns true when the transcript is a proper prefix of a supported command.
/// Streaming callers use this to hold back partial command text instead of
/// inserting it before Whisper has enough context to recognize the command.
pub fn is_command_prefix(transcript: &str) -> bool {
    let normalized = transcript
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    if normalized.is_empty() || parse_command(&normalized).is_some() {
        return false;
    }

    [
        VoiceCommand::NewLine,
        VoiceCommand::NewParagraph,
        VoiceCommand::DeleteLastWord,
    ]
    .into_iter()
    .any(|command| command.phrase().starts_with(&format!("{normalized} ")))
}

pub fn execute(command: VoiceCommand, target_window: i64) -> Result<(), String> {
    eprintln!("[INFO] Executing voice command: {command:?}");

    match command {
        VoiceCommand::NewLine => insertion::press_enter(target_window, 1),
        VoiceCommand::NewParagraph => insertion::press_enter(target_window, 2),
        VoiceCommand::DeleteLastWord => insertion::delete_last_word(target_window),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_command_prefix, parse_command, VoiceCommand};

    #[test]
    fn parses_supported_commands() {
        assert_eq!(parse_command("new line"), Some(VoiceCommand::NewLine));
        assert_eq!(parse_command("NEW LINE"), Some(VoiceCommand::NewLine));
        assert_eq!(
            parse_command("new paragraph"),
            Some(VoiceCommand::NewParagraph)
        );
        assert_eq!(
            parse_command("delete last word"),
            Some(VoiceCommand::DeleteLastWord)
        );
    }

    #[test]
    fn ignores_surrounding_whitespace() {
        assert_eq!(parse_command("  new line  "), Some(VoiceCommand::NewLine));
    }

    #[test]
    fn rejects_non_commands() {
        for transcript in [
            "",
            "hello world",
            "please create a new line",
            "delete the last word from this sentence",
        ] {
            assert_eq!(
                parse_command(transcript),
                None,
                "unexpected command: {transcript:?}"
            );
        }
    }

    #[test]
    fn recognizes_only_proper_command_prefixes() {
        assert!(is_command_prefix("new"));
        assert!(is_command_prefix("delete last"));
        assert!(!is_command_prefix("new line"));
        assert!(!is_command_prefix("new line please"));
        assert!(!is_command_prefix("hello new"));
    }
}
