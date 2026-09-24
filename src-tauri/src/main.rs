#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;

use std::sync::{Arc, Mutex};
use tauri::{Emitter, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState};

pub struct AppState { recorder: Arc<Mutex<audio::Recorder>> }

#[tauri::command]
fn recording_start(state: State<'_, AppState>) -> Result<(), String> { audio::start(&state.recorder).map_err(|e| e.to_string()) }

#[tauri::command]
fn recording_stop(state: State<'_, AppState>) -> Result<String, String> { audio::stop(&state.recorder).map(|p| p.display().to_string()).map_err(|e| e.to_string()) }

fn main() {
    let recorder = Arc::new(Mutex::new(audio::Recorder::default()));
    let state = AppState { recorder: recorder.clone() };
    tauri::Builder::default()
      .manage(state)
      .plugin(tauri_plugin_global_shortcut::Builder::new().build())
      .setup(move |app| {
        eprintln!("[INFO] Vaktum started");
        let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
        let handle = app.handle().clone();
        let rec = recorder.clone();
        app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event: ShortcutEvent| {
          match event.state() {
            ShortcutState::Pressed => match audio::start(&rec) {
              Ok(()) => { eprintln!("[INFO] Recording started"); let _ = handle.emit("vaktum://recording-started", ()); }
              Err(e) => { eprintln!("[ERROR] {e}"); let _ = handle.emit("vaktum://recording-error", e.to_string()); }
            },
            ShortcutState::Released => match audio::stop(&rec) {
              Ok(path) => { eprintln!("[INFO] Recording stopped"); let _ = handle.emit("vaktum://recording-stopped", path.display().to_string()); }
              Err(e) => { eprintln!("[ERROR] {e}"); let _ = handle.emit("vaktum://recording-error", e.to_string()); }
            },
          }
        })?;
        eprintln!("[INFO] Global hotkey registered: Ctrl+Shift+Space");
        Ok(())
      })
      .invoke_handler(tauri::generate_handler![recording_start, recording_stop])
      .run(tauri::generate_context!())
      .expect("error while running Vaktum");
}
