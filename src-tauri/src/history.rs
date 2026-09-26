use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};

const HISTORY_FILE: &str = "history.json";
pub const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub timestamp: String,
    pub raw_transcript: String,
    pub final_transcript: String,
}

pub fn load() -> Vec<HistoryEntry> {
    load_from_path(&history_path()).unwrap_or_default()
}

pub fn append(entry: HistoryEntry) -> Result<(), String> {
    let path = history_path();
    let mut entries = load_from_path(&path).unwrap_or_default();
    entries.insert(0, entry);
    entries.truncate(HISTORY_LIMIT);
    save_to_path(&path, &entries)
        .map_err(|error| format!("Unable to save history: {error}"))
}

pub fn history_path() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data)
            .join("Vaktum")
            .join(HISTORY_FILE);
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".vaktum")
            .join(HISTORY_FILE);
    }

    PathBuf::from(".vaktum").join(HISTORY_FILE)
}

fn load_from_path(path: &PathBuf) -> Result<Vec<HistoryEntry>, String> {
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let entries = serde_json::from_str::<Vec<HistoryEntry>>(&contents)
        .map_err(|error| error.to_string())?;

    Ok(entries.into_iter().take(HISTORY_LIMIT).collect())
}

fn save_to_path(path: &PathBuf, entries: &[HistoryEntry]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let temporary_path = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(entries).map_err(|error| error.to_string())?;

    let mut file = fs::File::create(&temporary_path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);

    fs::rename(temporary_path, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_history_path() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();

        std::env::temp_dir().join(format!("vaktum-history-{suffix}.json"))
    }

    fn entry(index: usize) -> HistoryEntry {
        HistoryEntry {
            timestamp: format!("2026-09-26T12:{index:02}:00Z"),
            raw_transcript: format!("raw {index}"),
            final_transcript: format!("final {index}"),
        }
    }

    #[test]
    fn entry_preserves_raw_and_final_transcript() {
        let value = entry(1);
        assert_eq!(value.raw_transcript, "raw 1");
        assert_eq!(value.final_transcript, "final 1");
        assert!(!value.timestamp.is_empty());
    }

    #[test]
    fn history_is_limited_to_newest_entries() {
        let path = temp_history_path();
        let mut entries = (0..(HISTORY_LIMIT + 5)).map(entry).collect::<Vec<_>>();
        entries.reverse();
        save_to_path(&path, &entries).expect("history should save");

        let loaded = load_from_path(&path).expect("history should load");
        assert_eq!(loaded.len(), HISTORY_LIMIT);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupted_history_is_rejected() {
        let path = temp_history_path();
        fs::write(&path, b"{not valid json").expect("fixture should write");

        assert!(load_from_path(&path).is_err());

        let _ = fs::remove_file(path);
    }
}
