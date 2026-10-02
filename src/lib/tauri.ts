import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  AudioDevice,
  DictionaryEntry,
  HistoryEntry,
  TranscriptionResult,
} from "./types";

export const commands = {
  recordingStart: () => invoke<void>("recording_start"),
  recordingStop: () => invoke<string>("recording_stop"),
  latestRecordingPath: () => invoke<string>("latest_recording_path"),
  transcribeRecording: (path: string) =>
    invoke<TranscriptionResult>("transcribe_recording", { path }),
  insertText: (text: string) => invoke<void>("insert_text", { text }),
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => invoke<void>("save_config", { newConfig: config }),
  getHistory: () => invoke<HistoryEntry[]>("get_history"),
  getDictionary: () => invoke<DictionaryEntry[]>("get_dictionary"),
  addDictionaryEntry: (source: string, replacement: string) =>
    invoke<DictionaryEntry[]>("add_dictionary_entry", { source, replacement }),
  removeDictionaryEntry: (source: string) =>
    invoke<DictionaryEntry[]>("remove_dictionary_entry", { source }),
  inputDevices: () => invoke<AudioDevice[]>("input_devices"),
};
