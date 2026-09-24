import { invoke } from "@tauri-apps/api/core";

export const commands = {
  recordingStart: () => invoke("recording_start"),
  recordingStop: () => invoke<string>("recording_stop"),
  transcribeRecording: (path: string) => invoke<string>("transcribe_recording", { path }),
};
