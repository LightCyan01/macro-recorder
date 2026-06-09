use crate::{
    app_state::{self, AppState, PlaybackOptions},
    engine::{player, recorder},
    hotkeys::{self, HotkeyConfig},
    macro_file::{self, MacroFile},
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn start_recording(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    recorder::start_recording(state.inner().clone())
}

#[tauri::command]
pub fn stop_recording(
    name: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<MacroFile, String> {
    recorder::stop_recording(state.inner().clone(), app, name)
}

#[tauri::command]
pub fn play_macro(
    name: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let macro_file = macro_file::load_macro(&state.macro_dir()?, &name)?;
    player::start_playback(state.inner().clone(), macro_file, app)
}

#[tauri::command]
pub fn stop_playback(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    player::stop_playback(state.inner().clone())
}

#[tauri::command]
pub fn list_macros(state: State<'_, Arc<AppState>>) -> Result<Vec<MacroFile>, String> {
    hotkeys::reload_macro_cache(&state)
}

#[tauri::command]
pub fn delete_macro(
    name: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<MacroFile>, String> {
    if player::is_playing_macro(state.inner().as_ref(), &name) {
        player::stop_playback(state.inner().clone())?;
    }
    macro_file::delete_macro(&state.macro_dir()?, &name)?;
    state.refresh_macros_from_disk()
}

#[tauri::command]
pub fn rename_macro(
    old_name: String,
    new_name: String,
    state: State<'_, Arc<AppState>>,
) -> Result<MacroFile, String> {
    if player::is_playing_macro(state.inner().as_ref(), &old_name) {
        player::stop_playback(state.inner().clone())?;
    }
    let renamed = macro_file::rename_macro(&state.macro_dir()?, &old_name, &new_name)?;
    state.refresh_macros_from_disk()?;
    Ok(renamed)
}

#[tauri::command]
pub fn update_macro(
    macro_file: MacroFile,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<MacroFile, String> {
    if player::is_playing_macro(state.inner().as_ref(), &macro_file.name) {
        player::stop_playback(state.inner().clone())?;
    }

    let saved = macro_file::update_macro(&state.macro_dir()?, macro_file)?;
    let macros = state.refresh_macros_from_disk()?;
    app.emit("macros-updated", macros).ok();
    Ok(saved)
}

#[tauri::command]
pub fn get_hotkeys(state: State<'_, Arc<AppState>>) -> Result<HotkeyConfig, String> {
    state
        .hotkeys
        .lock()
        .map(|config| config.clone())
        .map_err(|_| "Hotkey config lock was poisoned".to_string())
}

#[tauri::command]
pub fn set_hotkeys(config: HotkeyConfig, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    hotkeys::validate_config(&config)?;

    {
        let runtime_guard = state
            .hotkey_runtime
            .lock()
            .map_err(|_| "Hotkey runtime lock was poisoned".to_string())?;
        let runtime = runtime_guard
            .as_ref()
            .ok_or_else(|| "Hotkey runtime is not initialized".to_string())?;
        runtime.apply(config.clone())?;
    }

    hotkeys::save_hotkeys(&state.app_data_dir()?, &config)?;
    state.update_hotkey_vk_codes(&config);
    *state
        .hotkeys
        .lock()
        .map_err(|_| "Hotkey config lock was poisoned".to_string())? = config;
    Ok(())
}

#[tauri::command]
pub fn get_playback_options(state: State<'_, Arc<AppState>>) -> Result<PlaybackOptions, String> {
    state
        .playback_options
        .lock()
        .map(|options| options.clone())
        .map_err(|_| "Playback options lock was poisoned".to_string())
}

#[tauri::command]
pub fn set_playback_options(
    options: PlaybackOptions,
    state: State<'_, Arc<AppState>>,
) -> Result<PlaybackOptions, String> {
    let sanitized = options.sanitized();
    *state
        .playback_options
        .lock()
        .map_err(|_| "Playback options lock was poisoned".to_string())? = sanitized.clone();
    Ok(sanitized)
}

#[tauri::command]
pub fn get_save_directory(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let dir = state.macro_dir()?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn set_save_directory(
    path: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let dir = std::path::PathBuf::from(&path);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("Failed to create directory: {err}"))?;
    }
    if !dir.is_dir() {
        return Err("The selected path is not a directory".to_string());
    }

    state.set_macro_dir(dir);

    let app_data_dir = state.app_data_dir()?;
    let settings = app_state::AppSettings {
        macro_directory: Some(path.clone()),
    };
    app_state::save_settings(&app_data_dir, &settings)?;

    let macros = state.refresh_macros_from_disk()?;
    app.emit("macros-updated", macros).ok();

    Ok(path)
}
