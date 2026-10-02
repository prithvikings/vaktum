use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};

const DICTIONARY_FILE: &str = "dictionary.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictionaryEntry {
    pub source: String,
    pub replacement: String,
}

pub fn load() -> Vec<DictionaryEntry> {
    load_from_path(&dictionary_path()).unwrap_or_default()
}

pub fn save(entries: &[DictionaryEntry]) -> Result<(), String> {
    validate_entries(entries)?;
    save_to_path(&dictionary_path(), entries)
        .map_err(|error| format!("Unable to save dictionary: {error}"))
}

pub fn add(source: &str, replacement: &str) -> Result<Vec<DictionaryEntry>, String> {
    let entry = DictionaryEntry {
        source: normalize_source(source)
            .ok_or_else(|| "Dictionary source cannot be empty.".to_owned())?,
        replacement: normalize_replacement(replacement)
            .ok_or_else(|| "Dictionary replacement cannot be empty.".to_owned())?,
    };

    let mut entries = load();
    if entries
        .iter()
        .any(|existing| existing.source.eq_ignore_ascii_case(&entry.source))
    {
        return Err("A dictionary entry with that source already exists.".to_owned());
    }

    entries.push(entry);
    save(&entries)?;
    Ok(entries)
}

pub fn remove(source: &str) -> Result<Vec<DictionaryEntry>, String> {
    let source = normalize_source(source)
        .ok_or_else(|| "Dictionary source cannot be empty.".to_owned())?;
    let mut entries = load();
    let original_len = entries.len();
    entries.retain(|entry| !entry.source.eq_ignore_ascii_case(&source));

    if entries.len() == original_len {
        return Err("Dictionary entry was not found.".to_owned());
    }

    save(&entries)?;
    Ok(entries)
}

pub fn dictionary_path() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data)
            .join("Vaktum")
            .join(DICTIONARY_FILE);
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".vaktum")
            .join(DICTIONARY_FILE);
    }

    PathBuf::from(".vaktum").join(DICTIONARY_FILE)
}

pub fn apply(entries: &[DictionaryEntry], input: &str) -> String {
    if input.is_empty() || entries.is_empty() {
        return input.to_owned();
    }

    let prepared = prepare_entries(entries);
    if prepared.is_empty() {
        return input.to_owned();
    }

    let spans = token_spans(input);
    if spans.is_empty() {
        return input.to_owned();
    }

    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut index = 0;

    while index < spans.len() {
        let best = prepared
            .iter()
            .filter(|entry| matches_at(entry, input, &spans, index))
            .max_by(|left, right| {
                left.tokens
                    .len()
                    .cmp(&right.tokens.len())
                    .then_with(|| right.source_key.cmp(&left.source_key))
            });

        if let Some(entry) = best {
            let last_index = index + entry.tokens.len() - 1;
            let first = &input[spans[index].0..spans[index].1];
            let last = &input[spans[last_index].0..spans[last_index].1];
            let (first_core_start, _) = token_core_bounds(first);
            let (_, last_core_end) = token_core_bounds(last);

            output.push_str(&input[cursor..spans[index].0]);
            output.push_str(&first[..first_core_start]);
            output.push_str(&entry.replacement);
            output.push_str(&last[last_core_end..]);

            cursor = spans[last_index].1;
            index = last_index + 1;
        } else {
            index += 1;
        }
    }

    output.push_str(&input[cursor..]);
    output
}

/// Returns the portion that is safe to commit during streaming.
///
/// A trailing sequence that is a proper prefix of a multi-word dictionary
/// entry is held back so a phrase such as `whisper` is not inserted before a
/// later hypothesis expands it to `whisper rs`.
pub fn safe_prefix(entries: &[DictionaryEntry], input: &str) -> String {
    let spans = token_spans(input);
    if spans.is_empty() || entries.is_empty() {
        return input.to_owned();
    }

    let prepared = prepare_entries(entries);
    let max_hold = prepared
        .iter()
        .filter(|entry| entry.tokens.len() > 1)
        .map(|entry| entry.tokens.len() - 1)
        .max()
        .unwrap_or(0)
        .min(spans.len());

    for hold in (1..=max_hold).rev() {
        let start = spans.len() - hold;
        let suffix = &spans[start..];

        if prepared.iter().any(|entry| {
            entry.tokens.len() > hold
                && suffix.iter().enumerate().all(|(offset, span)| {
                    comparable_token(&input[span.0..span.1]) == entry.tokens[offset]
                })
        }) {
            return input[..spans[start].0].trim_end().to_owned();
        }
    }

    input.to_owned()
}

fn normalize_source(source: &str) -> Option<String> {
    let normalized = source.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn normalize_replacement(replacement: &str) -> Option<String> {
    let normalized = replacement.trim().to_owned();
    (!normalized.is_empty()).then_some(normalized)
}

fn validate_entries(entries: &[DictionaryEntry]) -> Result<(), String> {
    let mut sources = Vec::with_capacity(entries.len());

    for entry in entries {
        let source = normalize_source(&entry.source)
            .ok_or_else(|| "Dictionary source cannot be empty.".to_owned())?;
        if normalize_replacement(&entry.replacement).is_none() {
            return Err("Dictionary replacement cannot be empty.".to_owned());
        }

        if sources
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&source))
        {
            return Err("Dictionary contains duplicate source entries.".to_owned());
        }
        sources.push(source);
    }

    Ok(())
}

#[derive(Debug)]
struct PreparedEntry {
    tokens: Vec<String>,
    replacement: String,
    source_key: String,
}

fn prepare_entries(entries: &[DictionaryEntry]) -> Vec<PreparedEntry> {
    let mut prepared = entries
        .iter()
        .filter_map(|entry| {
            let source = normalize_source(&entry.source)?;
            let replacement = normalize_replacement(&entry.replacement)?;
            let tokens = source
                .split_whitespace()
                .map(comparable_token)
                .collect::<Vec<_>>();

            if tokens.iter().any(String::is_empty) {
                return None;
            }

            Some(PreparedEntry {
                tokens,
                replacement,
                source_key: source.to_ascii_lowercase(),
            })
        })
        .collect::<Vec<_>>();

    prepared.sort_by(|left, right| {
        right
            .tokens
            .len()
            .cmp(&left.tokens.len())
            .then_with(|| left.source_key.cmp(&right.source_key))
    });
    prepared
}

fn matches_at(
    entry: &PreparedEntry,
    input: &str,
    spans: &[(usize, usize)],
    start: usize,
) -> bool {
    let end = start + entry.tokens.len();
    if end > spans.len() {
        return false;
    }

    spans[start..end].iter().enumerate().all(|(offset, span)| {
        comparable_token(&input[span.0..span.1]) == entry.tokens[offset]
    })
}

fn token_spans(input: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;

    for (index, character) in input.char_indices() {
        if character.is_whitespace() {
            if let Some(begin) = start.take() {
                spans.push((begin, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }

    if let Some(begin) = start {
        spans.push((begin, input.len()));
    }

    spans
}

fn token_core_bounds(token: &str) -> (usize, usize) {
    let core = token_core(token);
    if core.is_empty() {
        return (token.len(), token.len());
    }

    let start = token.find(core).unwrap_or(0);
    (start, start + core.len())
}

fn token_core(token: &str) -> &str {
    token.trim_matches(|character: char| ".,!?;:()[]{}\"'".contains(character))
}

fn comparable_token(token: &str) -> String {
    token_core(token).to_lowercase()
}

fn load_from_path(path: &PathBuf) -> Result<Vec<DictionaryEntry>, String> {
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let entries = serde_json::from_str::<Vec<DictionaryEntry>>(&contents)
        .map_err(|error| error.to_string())?;
    validate_entries(&entries)?;
    Ok(entries)
}

fn save_to_path(path: &PathBuf, entries: &[DictionaryEntry]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary_path = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(entries).map_err(std::io::Error::other)?;

    let mut file = fs::File::create(&temporary_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);

    fs::rename(temporary_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: &str, replacement: &str) -> DictionaryEntry {
        DictionaryEntry {
            source: source.to_owned(),
            replacement: replacement.to_owned(),
        }
    }

    #[test]
    fn basic_replacement() {
        assert_eq!(
            apply(&[entry("vaktum", "Vaktum")], "hello vaktum"),
            "hello Vaktum"
        );
    }

    #[test]
    fn no_match_leaves_text_unchanged() {
        assert_eq!(apply(&[entry("vaktum", "Vaktum")], "hello world"), "hello world");
    }

    #[test]
    fn respects_word_boundaries() {
        assert_eq!(apply(&[entry("cat", "dog")], "cat concatenate"), "dog concatenate");
    }

    #[test]
    fn supports_multi_word_phrases() {
        assert_eq!(
            apply(&[entry("type script", "TypeScript")], "type script project"),
            "TypeScript project"
        );
    }

    #[test]
    fn longest_phrase_wins() {
        let entries = [
            entry("react", "React"),
            entry("react native", "React Native"),
        ];
        assert_eq!(apply(&entries, "react native app"), "React Native app");
    }

    #[test]
    fn multiple_replacements_are_applied() {
        let entries = [
            entry("vaktum", "Vaktum"),
            entry("whisper rs", "whisper-rs"),
        ];
        assert_eq!(
            apply(&entries, "using vaktum with whisper rs"),
            "using Vaktum with whisper-rs"
        );
    }

    #[test]
    fn replacement_handles_punctuation() {
        let entries = [entry("react", "React"), entry("vaktum", "Vaktum")];
        assert_eq!(apply(&entries, "react, vaktum!"), "React, Vaktum!");
        assert_eq!(apply(&[entry("react", "React")], "(react)"), "(React)");
    }

    #[test]
    fn technical_punctuation_is_not_treated_as_a_boundary() {
        let entries = [entry("c++", "C++")];
        assert_eq!(apply(&entries, "c++ and c"), "C++ and c");
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(apply(&[entry("vaktum", "Vaktum")], "VAKTUM vaktum"), "Vaktum Vaktum");
    }

    #[test]
    fn application_is_idempotent_for_normal_entries() {
        let entries = [entry("vaktum", "Vaktum"), entry("whisper rs", "whisper-rs")];
        let once = apply(&entries, "vaktum whisper rs");
        assert_eq!(apply(&entries, &once), once);
    }

    #[test]
    fn empty_input_is_safe() {
        assert_eq!(apply(&[entry("vaktum", "Vaktum")], ""), "");
    }

    #[test]
    fn rejects_empty_entries_and_duplicates() {
        assert!(validate_entries(&[entry("", "value")]).is_err());
        assert!(validate_entries(&[entry("source", "")]).is_err());
        assert!(validate_entries(&[
            entry("Vaktum", "Vaktum"),
            entry("vaktum", "VAKTUM"),
        ])
        .is_err());
    }

    #[test]
    fn persistence_round_trips_entries() {
        let path = std::env::temp_dir().join(format!("vaktum-dictionary-{}.json", std::process::id()));
        let entries = vec![entry("vaktum", "Vaktum"), entry("whisper rs", "whisper-rs")];

        save_to_path(&path, &entries).expect("dictionary should save");
        let loaded = load_from_path(&path).expect("dictionary should load");
        assert_eq!(loaded, entries);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_persistence_is_rejected() {
        let path = std::env::temp_dir().join(format!("vaktum-dictionary-invalid-{}.json", std::process::id()));
        fs::write(&path, b"{not valid json").expect("fixture should write");
        assert!(load_from_path(&path).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn safe_prefix_holds_partial_multi_word_entries() {
        let entries = [entry("whisper rs", "whisper-rs")];
        assert_eq!(safe_prefix(&entries, "using whisper"), "using");
        assert_eq!(safe_prefix(&entries, "using whisper rs"), "using whisper rs");
    }

    #[test]
    fn safe_prefix_does_not_hold_complete_or_single_word_entries() {
        let entries = [entry("vaktum", "Vaktum"), entry("react native", "React Native")];
        assert_eq!(safe_prefix(&entries, "hello vaktum"), "hello vaktum");
        assert_eq!(safe_prefix(&entries, "hello react native"), "hello react native");
    }
}
