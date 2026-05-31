#![cfg_attr(test, allow(dead_code))]

mod app_state;
mod engine;
mod hotkeys;
mod macro_file;
#[cfg(not(test))]
mod ui_commands;

#[cfg(not(test))]
use app_state::{AppState, GLOBAL_STATE};
#[cfg(not(test))]
use std::sync::Arc;
#[cfg(not(test))]
use tauri::{Manager, WindowEvent};

#[cfg(not(test))]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let app_data_dir = app.path().app_data_dir()?;
            let macro_dir = app_data_dir.join("macros");
            std::fs::create_dir_all(&macro_dir)?;

            let hotkey_config = hotkeys::load_or_default(&app_data_dir);
            let state = Arc::new(AppState::new(hotkey_config));
            state.set_app_data_dir(app_data_dir);
            state.set_macro_dir(macro_dir);
            state.set_app_handle(app_handle.clone());
            state
                .refresh_macros_from_disk()
                .map_err(std::io::Error::other)?;

            let hotkey_runtime = hotkeys::start_hotkey_thread(state.clone(), app_handle.clone())
                .map_err(std::io::Error::other)?;
            *state
                .hotkey_runtime
                .lock()
                .expect("hotkey_runtime mutex poisoned") = Some(hotkey_runtime);

            let _ = GLOBAL_STATE.set(state.clone());
            app.manage(state);
            Ok(())
        })
        .on_window_event(|_window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                shutdown_workers();
            }
        })
        .invoke_handler(tauri::generate_handler![
            ui_commands::start_recording,
            ui_commands::stop_recording,
            ui_commands::play_macro,
            ui_commands::stop_playback,
            ui_commands::list_macros,
            ui_commands::delete_macro,
            ui_commands::rename_macro,
            ui_commands::update_macro,
            ui_commands::get_hotkeys,
            ui_commands::set_hotkeys,
            ui_commands::get_playback_options,
            ui_commands::set_playback_options
        ])
        .run(tauri::generate_context!())
        .expect("error while running macro recorder");
}

#[cfg(not(test))]
fn shutdown_workers() {
    let Some(state) = app_state::global_state() else {
        return;
    };

    let _ = engine::player::stop_playback(state.clone());
    let _ = engine::recorder::stop_recording_without_save(state.clone());

    let hotkey_runtime = state
        .hotkey_runtime
        .lock()
        .ok()
        .and_then(|mut runtime| runtime.take());

    if let Some(runtime) = hotkey_runtime {
        runtime.stop();
    }
}
