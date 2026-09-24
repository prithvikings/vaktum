import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { VaktumState } from "./lib/types";
import { commands } from "./lib/tauri";

export default function App() {
  const [state, setState] = useState<VaktumState>("idle");
  const [recordingPath, setRecordingPath] = useState("");
  const [transcript, setTranscript] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let unlisten: Array<() => void> = [];

    void commands.latestRecordingPath()
      .then(setRecordingPath)
      .catch(() => undefined);
    void Promise.all([
      listen("vaktum://recording-started", () => {
        setError("");
        setTranscript("");
        setRecordingPath("");
        setState("recording");
      }),
      listen<string>("vaktum://recording-stopped", (event) => {
        setRecordingPath(event.payload);
        setState("processing");
        window.setTimeout(() => setState("idle"), 250);
      }),
      listen<string>("vaktum://recording-error", (event) => {
        setError(event.payload);
        setState("error");
      }),
    ]).then((listeners) => {
      unlisten = listeners;
    });

    return () => unlisten.forEach((stop) => stop());
  }, []);

  const transcribe = async () => {
    if (!recordingPath || state === "transcribing") return;

    setError("");
    setTranscript("");
    setState("transcribing");

    try {
      const result = await commands.transcribeRecording(recordingPath);
      setTranscript(result);
      setState("idle");
    } catch (cause) {
      setError(String(cause));
      setState("error");
    }
  };

  return (
    <main className="app">
      <header>
        <div>
          <h1>Vaktum</h1>
          <p>Local-first voice dictation</p>
        </div>
        <span className={state === "recording" ? "dot recording" : "dot"} />
      </header>

      <section className="card">
        <small>Status</small>
        <strong>
          {state === "recording"
            ? "Listening..."
            : state === "processing"
              ? "Recording stopped"
              : state === "transcribing"
                ? "Transcribing..."
                : state === "error"
                  ? "Error"
                  : "Ready"}
        </strong>
        {error && <p className="error">{error}</p>}
      </section>

      <section className="card">
        <small>Hotkey</small>
        <strong>Ctrl + Shift + Space</strong>
        <p>Hold the global shortcut while speaking. Release it to save the recording.</p>
      </section>

      {recordingPath && (
        <section className="card">
          <small>Latest recording</small>
          <code>{recordingPath}</code>
          <button type="button" onClick={() => void transcribe()} disabled={state === "transcribing"}>
            {state === "transcribing" ? "Transcribing..." : "Transcribe"}
          </button>
        </section>
      )}

      {transcript && (
        <section className="card">
          <small>Transcript</small>
          <p>{transcript}</p>
        </section>
      )}
    </main>
  );
}
