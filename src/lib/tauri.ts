import { invoke } from "@tauri-apps/api/core";

export const commands = {
  recordingStart: () => invoke("recording_start"),
  recordingStop: () => invoke<string>("recording_stop"),
};
