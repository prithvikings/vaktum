#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod cleanup;
mod config;
mod history;
mod insertion;
mod streaming;
mod transcription;

use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::{
    menu::{Menu, MenuItem},
    Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{
    GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState,
};

pub struct AppState {
    recorder: Arc<Mutex<audio::Recorder>>,
    target_window: Arc<Mutex<Option<i64>>>,
    config: Arc<Mutex<config::AppConfig>>,
    streaming_session: Arc<Mutex<Option<streaming::StreamingSession>>>,
}

#[derive(serde::Serialize)]
struct TranscriptionResponse {
    raw_transcript: String,
    final_transcript: String,
    history_error: Option<String>,
}

#[tauri::command]
fn recording_start(state: State<'_, AppState>) -> Result<(), String> {
    let microphone = state
        .config
        .lock()
        .map_err(|_| "Configuration is unavailable".to_owned())?
        .microphone
        .clone();

    audio::start(&state.recorder, &microphone)
        .map_err(|error| {
            eprintln!("[ERROR] recording start: {error}");
            "Unable to start recording. Please check your microphone.".to_owned()
        })
}

#[tauri::command]
fn recording_stop(state: State<'_, AppState>) -> Result<String, String> {
    audio::stop(&state.recorder)
        .map(|path| path.display().to_string())
        .map_err(|error| {
            eprintln!("[ERROR] recording stop: {error}");
            "Unable to stop recording. Please try again.".to_owned()
        })
}

#[tauri::command]
fn latest_recording_path() -> Result<String, String> {
    transcription::latest_recording_path()
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn transcribe_recording(
    state: State<'_, AppState>,
    path: String,
) -> Result<TranscriptionResponse, String> {
    let config = state
        .config
        .lock()
        .map_err(|_| "Configuration is unavailable".to_owned())?
        .clone();

    let transcription_config = config.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut transcriber = transcription::WhisperTranscriber::from_config(&transcription_config)
            .map_err(|error| error.to_string())?;

        transcriber
            .transcribe(path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| {
        eprintln!("[ERROR] transcription task failed: {error}");
        "Transcription failed. Please try again.".to_owned()
    })?
    .map_err(|error| {
        eprintln!("[ERROR] transcription: {error}");

        if error.contains("Whisper model not found") {
            "Whisper model not found. Please check the model path in Settings.".to_owned()
        } else {
            "Transcription failed. Please try again.".to_owned()
        }
    })?;

    let history_error = if config.history_enabled {
        let entry = history::HistoryEntry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_owned()),
            raw_transcript: result.raw_transcript.clone(),
            final_transcript: result.final_transcript.clone(),
        };

        match history::append(entry) {
            Ok(()) => None,
            Err(error) => {
                eprintln!("[ERROR] history save: {error}");
                Some("Unable to save history.".to_owned())
            }
        }
    } else {
        None
    };

    Ok(TranscriptionResponse {
        raw_transcript: result.raw_transcript,
        final_transcript: result.final_transcript,
        history_error,
    })
}

#[tauri::command]
fn insert_text(state: State<'_, AppState>, text: String) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("No transcript available for insertion".to_owned());
    }

    let target_window = state
        .target_window
        .lock()
        .map_err(|_| "No target application was captured.".to_owned())?
        .ok_or_else(|| "No target application was captured.".to_owned())?;

    insertion::insert_text(target_window, &text).map_err(|error| {
        eprintln!("[ERROR] insertion: {error}");
        "Unable to insert text into the target application.".to_owned()
    })
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Result<config::AppConfig, String> {
    state
        .config
        .lock()
        .map(|config| config.clone())
        .map_err(|_| "Configuration is unavailable".to_owned())
}

#[tauri::command]
fn save_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    new_config: config::AppConfig,
) -> Result<(), String> {
    if !config::valid(&new_config) {
        return Err("Invalid configuration. Please check the settings.".to_owned());
    }

    let parsed_shortcut: Shortcut = new_config
        .hotkey
        .parse()
        .map_err(|error| format!("Invalid global hotkey: {error}"))?;

    let mut current = state
        .config
        .lock()
        .map_err(|_| "Configuration is unavailable".to_owned())?;

    let old_hotkey = current.hotkey.clone();

    if old_hotkey != new_config.hotkey {
        app.global_shortcut()
            .unregister(old_hotkey.as_str())
            .map_err(|error| format!("Unable to update global hotkey: {error}"))?;

        let recorder = state.recorder.clone();
        let target = state.target_window.clone();
        if let Err(error) = register_hotkey(&app, parsed_shortcut, recorder, target, state.config.clone()) {
            let _ = register_hotkey(
                &app,
                old_hotkey
                    .parse::<Shortcut>()
                    .map_err(|parse_error| parse_error.to_string())?,
                state.recorder.clone(),
                state.target_window.clone(),
                state.config.clone(),
                state.streaming_session.clone(),
            );

            return Err(format!("Unable to update global hotkey: {error}"));
        }
    }

    if let Err(error) = config::save(&new_config) {
        if old_hotkey != new_config.hotkey {
            let _ = app.global_shortcut().unregister(new_config.hotkey.as_str());
            let _ = register_hotkey(
                &app,
                old_hotkey
                    .parse::<Shortcut>()
                    .map_err(|parse_error| parse_error.to_string())?,
                state.recorder.clone(),
                state.target_window.clone(),
                state.config.clone(),
                state.streaming_session.clone(),
            );
        }

        return Err(error);
    }

    *current = new_config;
    Ok(())
}

#[tauri::command]
fn get_history() -> Result<Vec<history::HistoryEntry>, String> {
    Ok(history::load())
}

#[tauri::command]
fn input_devices() -> Result<Vec<audio::AudioDevice>, String> {
    audio::input_devices()
        .map_err(|error| {
            eprintln!("[ERROR] input device enumeration: {error}");
            "Unable to enumerate microphones.".to_owned()
        })
        .map(|devices| {
            devices
                .into_iter()
                .map(|name| audio::AudioDevice { name })
                .collect()
        })
}

fn register_hotkey(
    app: &tauri::AppHandle,
    shortcut: Shortcut,
    recorder: Arc<Mutex<audio::Recorder>>,
    target_window: Arc<Mutex<Option<i64>>>,
    config: Arc<Mutex<config::AppConfig>>,
    streaming_session: Arc<Mutex<Option<streaming::StreamingSession>>>,
) -> Result<(), String> {
    let handle = app.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event: ShortcutEvent| {
            match event.state() {
                ShortcutState::Pressed => {
                    if let Ok(window) = insertion::capture_target_window() {
                        if let Ok(mut target) = target_window.lock() {
                            *target = Some(window);
                        }
                    }

                    let microphone = config
                        .lock()
                        .map(|value| value.microphone.clone())
                        .unwrap_or_else(|_| "default".to_owned());

                    let target_window_id = match insertion::capture_target_window() {
                        Ok(window) => {
                            if let Ok(mut target) = target_window.lock() {
                                *target = Some(window);
                            }
                            window
                        }
                        Err(error) => {
                            eprintln!("[ERROR] target capture: {error}");
                            let _ = handle.emit(
                                "vaktum://recording-error",
                                "No target application was captured.",
                            );
                            return;
                        }
                    };

                    let microphone = config
                        .lock()
                        .map(|value| value.microphone.clone())
                        .unwrap_or_else(|_| "default".to_owned());

                    let configured = config
                        .lock()
                        .map(|value| value.clone())
                        .unwrap_or_else(|_| config::AppConfig::default());

                    match audio::start(&recorder, &microphone) {
                        Ok(()) => match streaming::StreamingSession::start(
                            handle.clone(),
                            recorder.clone(),
                            target_window_id,
                            configured,
                        ) {
                            Ok(session) => {
                                if let Ok(mut active) = streaming_session.lock() {
                                    *active = Some(session);
                                }
                                eprintln!("[INFO] Streaming dictation started");
                                let _ = handle.emit("vaktum://recording-started", ());
                            }
                            Err(error) => {
                                eprintln!("[ERROR] streaming start: {error}");
                                let _ = audio::stop(&recorder);
                                let _ = handle.emit(
                                    "vaktum://recording-error",
                                    "Unable to start streaming transcription.",
                                );
                            }
                        },
                        Err(error) => {
                            eprintln!("[ERROR] recording start: {error}");
                            let _ = handle.emit(
                                "vaktum://recording-error",
                                "Unable to start recording. Please check your microphone.",
                            );
                        }
                    }
                }
                ShortcutState::Released => {
                    let session = streaming_session
                        .lock()
                        .ok()
                        .and_then(|mut active| active.take());

                    if let Some(session) = session {
                        eprintln!("[INFO] Streaming dictation stopping");
                        thread::spawn(move || session.stop());
                    } else {
                        match audio::stop(&recorder) {
                            Ok(path) => {
                                eprintln!("[INFO] Recording stopped");
                                let _ = handle.emit(
                                    "vaktum://recording-stopped",
                                    path.display().to_string(),
                                );
                            }
                            Err(error) => {
                                eprintln!("[ERROR] recording stop: {error}");
                                let _ = handle.emit(
                                    "vaktum://recording-error",
                                    "Unable to stop recording. Please try again.",
                                );
                            }
                        }
                    }
                },
            }
        })
        .map_err(|error| error.to_string())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    let recorder = Arc::new(Mutex::new(audio::Recorder::default()));
    let target_window = Arc::new(Mutex::new(None));
    let config = Arc::new(Mutex::new(config::load()));
    let streaming_session = Arc::new(Mutex::new(None));

    tauri::Builder::default()
        .manage(AppState {
            recorder: recorder.clone(),
            target_window: target_window.clone(),
            config: config.clone(),
            streaming_session: streaming_session.clone(),
        })
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            eprintln!("[INFO] Vaktum started");

            let configured = config
                .lock()
                .map_err(|_| "Configuration lock is unavailable")?
                .clone();

            let shortcut: Shortcut = configured
                .hotkey
                .parse()
                .map_err(|error| format!("Invalid configured hotkey: {error}"))?;

            register_hotkey(
                app.handle(),
                shortcut,
                recorder.clone(),
                target_window.clone(),
                config.clone(),
                streaming_session.clone(),
            )?;

            let show = MenuItem::with_id(
                app,
                "show",
                "Show / Open",
                true,
                None::<&str>,
            )?;
            let history_item = MenuItem::with_id(
                app,
                "history",
                "History",
                true,
                None::<&str>,
            )?;
            let settings = MenuItem::with_id(
                app,
                "settings",
                "Settings",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(
                app,
                "quit",
                "Quit",
                true,
                None::<&str>,
            )?;

            let menu = Menu::with_items(app, &[&show, &history_item, &settings, &quit])?;
            let tray_icon = app.default_window_icon().cloned().ok_or("Tray icon is unavailable")?;

            tauri::tray::TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                .tooltip("Vaktum")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "history" => {
                        show_main_window(app);
                        let _ = app.emit("vaktum://navigate", "history");
                    }
                    "settings" => {
                        show_main_window(app);
                        let _ = app.emit("vaktum://navigate", "settings");
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            eprintln!("[INFO] Global hotkey registered: {}", configured.hotkey);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            recording_start,
            recording_stop,
            latest_recording_path,
            transcribe_recording,
            insert_text,
            get_config,
            save_config,
            get_history,
            input_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vaktum");
}
