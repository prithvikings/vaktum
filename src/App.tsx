import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AppConfig, AudioDevice, HistoryEntry, VaktumState } from "./lib/types";
import { canTransition, transition } from "./lib/state-machine";
import { commands } from "./lib/tauri";

type View = "main" | "history" | "settings";

const DEFAULT_CONFIG: AppConfig = {
  hotkey: "Ctrl+Shift+Space",
  microphone: "default",
  language: "en",
  model: "",
  history_enabled: true,
};

function formatHistoryTimestamp(timestamp: string): string {
  const date = new Date(Number(timestamp) * 1000);
  if (Number.isNaN(date.getTime())) return timestamp;
  return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(date);
}

function historyDay(timestamp: string): string {
  const date = new Date(Number(timestamp) * 1000);
  if (Number.isNaN(date.getTime())) return "History";

  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const entryDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const days = Math.round((today.getTime() - entryDay.getTime()) / 86_400_000);

  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";

  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date);
}

export default function App() {
  const [state, setState] = useState<VaktumState>("idle");
  const [view, setView] = useState<View>("main");
  const [recordingPath, setRecordingPath] = useState("");
  const [transcript, setTranscript] = useState("");
  const [rawTranscript, setRawTranscript] = useState("");
  const [error, setError] = useState("");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [config, setConfig] = useState<AppConfig>(DEFAULT_CONFIG);
  const [draftConfig, setDraftConfig] = useState<AppConfig>(DEFAULT_CONFIG);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [settingsSaved, setSettingsSaved] = useState(false);

  const move = (to: VaktumState) => {
    setState((current) => {
      if (canTransition(current, to)) return transition(current, to);
      setError("Unable to continue from " + current + " state.");
      return "error";
    });
  };

  const resetError = () => {
    setError("");
    setState((current) => (current === "error" ? transition(current, "idle") : current));
  };

  const loadHistory = async () => {
    try {
      setHistory(await commands.getHistory());
    } catch (cause) {
      setError(String(cause));
    }
  };

  useEffect(() => {
    void Promise.all([
      commands.latestRecordingPath().then(setRecordingPath).catch(() => undefined),
      commands.getConfig().then((loaded) => {
        setConfig(loaded);
        setDraftConfig(loaded);
      }).catch(() => undefined),
      commands.getHistory().then(setHistory).catch(() => undefined),
      commands.inputDevices().then(setDevices).catch(() => undefined),
    ]);

    let unlisten: Array<() => void> = [];

    void Promise.all([
      listen("vaktum://recording-started", () => {
        setError("");
        setTranscript("");
        setRawTranscript("");
        setRecordingPath("");
        move("recording");
      }),
      listen<string>("vaktum://recording-stopped", (event) => {
        setRecordingPath(event.payload);
        move("processing");
      }),
      listen<string>("vaktum://recording-error", (event) => {
        setError(event.payload);
        setState((current) => canTransition(current, "error") ? transition(current, "error") : "error");
      }),
      listen<string>("vaktum://navigate", (event) => {
        setView(event.payload === "settings" ? "settings" : "history");
        void loadHistory();
      }),
    ]).then((listeners) => {
      unlisten = listeners;
    });

    return () => unlisten.forEach((stop) => stop());
  }, []);

  const transcribe = async () => {
    if (!recordingPath || state === "transcribing" || state === "inserting") return;

    setError("");
    setTranscript("");
    setRawTranscript("");

    if (state === "error") {
      setState(transition("error", "idle"));
    }

    setState((current) => {
      if (canTransition(current, "transcribing")) return transition(current, "transcribing");
      setError("Unable to start transcription from " + current + " state.");
      return "error";
    });

    try {
      const result = await commands.transcribeRecording(recordingPath);
      setRawTranscript(result.raw_transcript);
      setTranscript(result.final_transcript);
      if (result.history_error) setError(result.history_error);
      await loadHistory();
      setState((current) => canTransition(current, "idle") ? transition(current, "idle") : "error");
    } catch (cause) {
      setError(String(cause));
      setState((current) => canTransition(current, "error") ? transition(current, "error") : "error");
    }
  };

  const insertTranscript = async () => {
    if (!transcript.trim() || state === "inserting" || state === "transcribing") return;

    setError("");

    if (state === "error") {
      setState(transition("error", "idle"));
    }

    setState((current) => {
      if (canTransition(current, "inserting")) return transition(current, "inserting");
      setError("Unable to start insertion from " + current + " state.");
      return "error";
    });

    try {
      await commands.insertText(transcript);
      setState((current) => canTransition(current, "idle") ? transition(current, "idle") : "error");
    } catch (cause) {
      setError(String(cause));
      setState((current) => canTransition(current, "error") ? transition(current, "error") : "error");
    }
  };

  const saveSettings = async () => {
    setError("");
    setSettingsSaved(false);

    try {
      await commands.saveConfig(draftConfig);
      setConfig(draftConfig);
      setSettingsSaved(true);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const groupedHistory = useMemo(() => {
    const groups = new Map<string, HistoryEntry[]>();
    for (const entry of history) {
      const day = historyDay(entry.timestamp);
      const existing = groups.get(day) ?? [];
      existing.push(entry);
      groups.set(day, existing);
    }
    return Array.from(groups.entries());
  }, [history]);

  const statusLabel =
    state === "recording" ? "Listening..." :
    state === "processing" ? "Recording stopped" :
    state === "transcribing" ? "Transcribing..." :
    state === "inserting" ? "Inserting..." :
    state === "error" ? "Error" : "Ready";

  return (
    <main className="app">
      <header>
        <div>
          <h1>Vaktum</h1>
          <p>Local-first voice dictation</p>
        </div>
        <span className={state === "recording" ? "dot recording" : "dot"} />
      </header>

      <nav className="nav">
        <button type="button" className={view === "main" ? "active" : ""} onClick={() => setView("main")}>Dictation</button>
        <button type="button" className={view === "history" ? "active" : ""} onClick={() => { setView("history"); void loadHistory(); }}>History</button>
        <button type="button" className={view === "settings" ? "active" : ""} onClick={() => setView("settings")}>Settings</button>
      </nav>

      {view === "main" && (
        <>
          <section className="card">
            <small>Status</small>
            <strong>{statusLabel}</strong>
            {error && <p className="error">{error}</p>}
            {state === "error" && <button type="button" onClick={resetError}>Dismiss error</button>}
          </section>

          <section className="card">
            <small>Hotkey</small>
            <strong>{config.hotkey}</strong>
            <p>Hold the global shortcut while speaking. Release it to save the recording.</p>
          </section>

          {recordingPath && (
            <section className="card">
              <small>Latest recording</small>
              <code>{recordingPath}</code>
              <button type="button" onClick={() => void transcribe()} disabled={state === "transcribing" || state === "inserting" || state === "recording"}>
                {state === "transcribing" ? "Transcribing..." : "Transcribe"}
              </button>
            </section>
          )}

          {transcript && (
            <section className="card">
              <small>Transcript</small>
              <p className="transcript">{transcript}</p>
              {rawTranscript && rawTranscript !== transcript && (
                <details>
                  <summary>Raw Whisper transcript</summary>
                  <p>{rawTranscript}</p>
                </details>
              )}
              <button type="button" onClick={() => void insertTranscript()} disabled={state === "inserting" || state === "transcribing"}>
                {state === "inserting" ? "Inserting..." : "Insert into focused app"}
              </button>
            </section>
          )}

          <section className="card">
            <small>Recent History</small>
            {!config.history_enabled ? <p>History is disabled in Settings.</p> : history.length === 0 ? <p>No transcription history yet.</p> : (
              <div className="history-preview">
                {history.slice(0, 3).map((entry) => (
                  <div className="history-item" key={entry.timestamp + entry.final_transcript}>
                    <time>{formatHistoryTimestamp(entry.timestamp)}</time>
                    <p>{entry.final_transcript}</p>
                  </div>
                ))}
              </div>
            )}
            <button type="button" onClick={() => { setView("history"); void loadHistory(); }}>View History</button>
          </section>
        </>
      )}

      {view === "history" && (
        <section className="card">
          <div className="section-heading">
            <div><small>History</small><strong>Recent transcriptions</strong></div>
            <button type="button" onClick={() => setView("main")}>Back</button>
          </div>

          {!config.history_enabled ? <p>History is disabled in Settings.</p> :
            groupedHistory.length === 0 ? <p>No transcription history yet.</p> :
            <div className="history-list">
              {groupedHistory.map(([day, entries]) => (
                <div key={day}>
                  <h2>{day}</h2>
                  {entries.map((entry) => (
                    <article className="history-item" key={entry.timestamp + entry.raw_transcript}>
                      <time>{formatHistoryTimestamp(entry.timestamp)}</time>
                      <p>{entry.final_transcript}</p>
                      {entry.raw_transcript !== entry.final_transcript && (
                        <details>
                          <summary>Raw Whisper transcript</summary>
                          <p>{entry.raw_transcript}</p>
                        </details>
                      )}
                    </article>
                  ))}
                </div>
              ))}
            </div>}
        </section>
      )}

      {view === "settings" && (
        <section className="card settings">
          <div className="section-heading">
            <div><small>Settings</small><strong>Phase 1 configuration</strong></div>
            <button type="button" onClick={() => setView("main")}>Back</button>
          </div>

          <label>
            Global hotkey
            <input value={draftConfig.hotkey} onChange={(event) => setDraftConfig({ ...draftConfig, hotkey: event.target.value })} placeholder="Ctrl+Shift+Space" />
          </label>

          <label>
            Microphone
            <select value={draftConfig.microphone} onChange={(event) => setDraftConfig({ ...draftConfig, microphone: event.target.value })}>
              <option value="default">System default microphone</option>
              {devices.map((device) => <option value={device.name} key={device.name}>{device.name}</option>)}
            </select>
          </label>

          <label>
            Language
            <input value={draftConfig.language} onChange={(event) => setDraftConfig({ ...draftConfig, language: event.target.value })} placeholder="en" />
          </label>

          <label>
            Whisper model path
            <input value={draftConfig.model} onChange={(event) => setDraftConfig({ ...draftConfig, model: event.target.value })} placeholder="%LOCALAPPDATA%\\\\Vaktum\\\\models\\\\ggml-base.en.bin" />
          </label>

          <label className="checkbox">
            <input type="checkbox" checked={draftConfig.history_enabled} onChange={(event) => setDraftConfig({ ...draftConfig, history_enabled: event.target.checked })} />
            Enable local transcription history
          </label>

          <button type="button" onClick={() => void saveSettings()}>Save Settings</button>
          {settingsSaved && <p className="success">Settings saved.</p>}
        </section>
      )}
    </main>
  );
}
