export type VaktumState =
  | "idle"
  | "recording"
  | "processing"
  | "transcribing"
  | "inserting"
  | "error";

export type AppConfig = {
  hotkey: string;
  microphone: string;
  language: string;
  model: string;
  history_enabled: boolean;
};

export type HistoryEntry = {
  timestamp: string;
  raw_transcript: string;
  final_transcript: string;
};

export type TranscriptionResult = {
  raw_transcript: string;
  final_transcript: string;
  history_error?: string | null;
};

export type AudioDevice = {
  name: string;
};

export type ApplicationKind =
  | "vscode"
  | "notepad"
  | "browser"
  | "terminal"
  | "unknown";

export type DictationContext = {
  application: ApplicationKind;
  process_name: string;
  captured_at: number;
};
