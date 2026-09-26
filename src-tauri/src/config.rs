use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};

const CONFIG_FILE: &str = "config.json";
const DEFAULT_HOTKEY: &str = "Ctrl+Shift+Space";
const DEFAULT_LANGUAGE: &str = "en";
const DEFAULT_MODEL_FILE: &str = "ggml-base.en.bin";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub hotkey: String,
    pub microphone: String,
    pub language: String,
    pub model: String,
    pub history_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_owned(),
            microphone: "default".to_owned(),
            language: DEFAULT_LANGUAGE.to_owned(),
            model: default_model_path(),
            history_enabled: true,
        }
    }
}

pub fn load() -> AppConfig {
    let path = config_path();

    match fs::read_to_string(&path)
        .ok()
        .and_then(|contents| serde_json::from_str::<AppConfig>(&contents).ok())
    {
        Some(config) if valid(&config) => config,
        _ => AppConfig::default(),
    }
}

pub fn save(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    write_json(&path, config).map_err(|error| format!("Unable to save configuration: {error}"))
}

pub fn config_path() -> PathBuf {
    vaktum_data_dir().join(CONFIG_FILE)
}

pub fn valid(config: &AppConfig) -> bool {
    !config.hotkey.trim().is_empty()
        && !config.language.trim().is_empty()
        && !config.model.trim().is_empty()
        && (config.microphone == "default" || !config.microphone.trim().is_empty())
}

fn default_model_path() -> String {
    if let Some(path) = std::env::var_os("VAKTUM_WHISPER_MODEL") {
        if !path.is_empty() {
            return PathBuf::from(path).display().to_string();
        }
    }

    vaktum_data_dir()
        .join("models")
        .join(DEFAULT_MODEL_FILE)
        .display()
        .to_string()
}

fn vaktum_data_dir() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join("Vaktum");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".vaktum");
    }

    PathBuf::from(".vaktum")
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary_path = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;

    let mut file = fs::File::create(&temporary_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);

    fs::rename(temporary_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe_and_meaningful() {
        let config = AppConfig::default();
        assert_eq!(config.hotkey, "Ctrl+Shift+Space");
        assert_eq!(config.microphone, "default");
        assert_eq!(config.language, "en");
        assert!(config.history_enabled);
        assert!(!config.model.is_empty());
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        let mut config = AppConfig::default();
        config.hotkey.clear();
        assert!(!valid(&config));
    }

    #[test]
    fn configuration_round_trips() {
        let config = AppConfig {
            hotkey: "Alt+Space".to_owned(),
            microphone: "wasapi:test".to_owned(),
            language: "en".to_owned(),
            model: "C:\\models\\custom.bin".to_owned(),
            history_enabled: false,
        };

        let encoded = serde_json::to_string(&config).expect("config should serialize");
        let decoded: AppConfig =
            serde_json::from_str(&encoded).expect("config should deserialize");

        assert_eq!(decoded, config);
    }

    #[test]
    fn configuration_file_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "vaktum-config-{}.json",
            std::process::id()
        ));

        let config = AppConfig {
            hotkey: "Alt+Space".to_owned(),
            microphone: "default".to_owned(),
            language: "en".to_owned(),
            model: "custom.bin".to_owned(),
            history_enabled: false,
        };

        write_json(&path, &config).expect("config file should save");
        let contents = std::fs::read_to_string(&path).expect("config file should exist");
        let loaded: AppConfig =
            serde_json::from_str(&contents).expect("config file should load");

        assert_eq!(loaded, config);
        let _ = std::fs::remove_file(path);
    }
}
