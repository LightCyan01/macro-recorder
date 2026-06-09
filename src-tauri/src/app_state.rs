#[cfg(not(test))]
use crate::hotkeys::HotkeyRuntime;
use crate::{
    hotkeys::HotkeyConfig,
    macro_file::{self, MacroEvent, MacroFile},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Instant,
};
#[cfg(not(test))]
use tauri::AppHandle;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlaybackOptions {
    pub loop_count: u32,
    pub speed_multiplier: f64,
    #[serde(default)]
    pub infinite_loop: bool,
}

impl Default for PlaybackOptions {
    fn default() -> Self {
        Self {
            loop_count: 1,
            speed_multiplier: 1.0,
            infinite_loop: false,
        }
    }
}

impl PlaybackOptions {
    pub fn sanitized(&self) -> Self {
        Self {
            loop_count: self.loop_count.max(1),
            speed_multiplier: self.speed_multiplier.clamp(0.1, 4.0),
            infinite_loop: self.infinite_loop,
        }
    }
}

pub struct AppState {
    pub recording: AtomicBool,
    pub playing: AtomicBool,
    pub run_gen: AtomicU64,
    pub recorded_event_count: AtomicU64,
    pub played_event_count: AtomicU64,
    pub events_buf: Mutex<Vec<MacroEvent>>,
    pub recording_keys_down: Mutex<HashSet<u16>>,
    pub macros: Mutex<Vec<MacroFile>>,
    pub playing_macro: Mutex<Option<String>>,
    pub hotkeys: Mutex<HotkeyConfig>,
    pub playback_options: Mutex<PlaybackOptions>,
    #[cfg(not(test))]
    pub recorder_runtime: Mutex<Option<crate::engine::recorder::RecorderRuntime>>,
    #[cfg(not(test))]
    pub player_runtime: Mutex<Option<crate::engine::player::PlayerRuntime>>,
    #[cfg(not(test))]
    pub hotkey_runtime: Mutex<Option<HotkeyRuntime>>,
    pub recording_started: Mutex<Option<Instant>>,
    pub hotkey_vk_codes: [AtomicU16; 3],
    app_data_dir: Mutex<Option<PathBuf>>,
    macro_dir: Mutex<Option<PathBuf>>,
    #[cfg(not(test))]
    app_handle: Mutex<Option<AppHandle>>,
}

impl AppState {
    pub fn new(hotkeys: HotkeyConfig) -> Self {
        let hotkey_vk_codes = [
            AtomicU16::new(hotkeys.record_toggle.vk_code),
            AtomicU16::new(hotkeys.play_toggle.vk_code),
            AtomicU16::new(hotkeys.emergency_stop.vk_code),
        ];
        Self {
            recording: AtomicBool::new(false),
            playing: AtomicBool::new(false),
            run_gen: AtomicU64::new(1),
            recorded_event_count: AtomicU64::new(0),
            played_event_count: AtomicU64::new(0),
            events_buf: Mutex::new(Vec::new()),
            recording_keys_down: Mutex::new(HashSet::new()),
            macros: Mutex::new(Vec::new()),
            playing_macro: Mutex::new(None),
            hotkeys: Mutex::new(hotkeys),
            playback_options: Mutex::new(PlaybackOptions::default()),
            #[cfg(not(test))]
            recorder_runtime: Mutex::new(None),
            #[cfg(not(test))]
            player_runtime: Mutex::new(None),
            #[cfg(not(test))]
            hotkey_runtime: Mutex::new(None),
            recording_started: Mutex::new(None),
            hotkey_vk_codes,
            app_data_dir: Mutex::new(None),
            macro_dir: Mutex::new(None),
            #[cfg(not(test))]
            app_handle: Mutex::new(None),
        }
    }

    pub fn set_app_data_dir(&self, path: PathBuf) {
        *self
            .app_data_dir
            .lock()
            .expect("app_data_dir mutex poisoned") = Some(path);
    }

    pub fn app_data_dir(&self) -> Result<PathBuf, String> {
        self.app_data_dir
            .lock()
            .map_err(|_| "App data directory lock was poisoned".to_string())?
            .clone()
            .ok_or_else(|| "App data directory is not initialized".to_string())
    }

    pub fn set_macro_dir(&self, path: PathBuf) {
        *self.macro_dir.lock().expect("macro_dir mutex poisoned") = Some(path);
    }

    pub fn macro_dir(&self) -> Result<PathBuf, String> {
        self.macro_dir
            .lock()
            .map_err(|_| "Macro directory lock was poisoned".to_string())?
            .clone()
            .ok_or_else(|| "Macro directory is not initialized".to_string())
    }

    #[cfg(not(test))]
    pub fn set_app_handle(&self, app_handle: AppHandle) {
        *self.app_handle.lock().expect("app_handle mutex poisoned") = Some(app_handle);
    }

    #[cfg(not(test))]
    pub fn app_handle(&self) -> Option<AppHandle> {
        self.app_handle
            .lock()
            .ok()
            .and_then(|handle| handle.as_ref().cloned())
    }

    pub fn refresh_macros_from_disk(&self) -> Result<Vec<MacroFile>, String> {
        let dir = self.macro_dir()?;
        let macros = macro_file::load_macros(&dir)?;
        *self
            .macros
            .lock()
            .map_err(|_| "Macro list lock was poisoned".to_string())? = macros.clone();
        Ok(macros)
    }

    pub fn replace_macro_cache(&self, macros: Vec<MacroFile>) -> Result<(), String> {
        *self
            .macros
            .lock()
            .map_err(|_| "Macro list lock was poisoned".to_string())? = macros;
        Ok(())
    }

    pub fn next_generation(&self) -> u64 {
        self.run_gen.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current_playing_macro(&self) -> Option<String> {
        self.playing_macro
            .lock()
            .ok()
            .and_then(|name| name.as_ref().cloned())
    }

    pub fn set_playing_macro(&self, name: Option<String>) -> Result<(), String> {
        *self
            .playing_macro
            .lock()
            .map_err(|_| "Playing macro lock was poisoned".to_string())? = name;
        Ok(())
    }

    pub fn update_hotkey_vk_codes(&self, config: &HotkeyConfig) {
        self.hotkey_vk_codes[0].store(config.record_toggle.vk_code, Ordering::Release);
        self.hotkey_vk_codes[1].store(config.play_toggle.vk_code, Ordering::Release);
        self.hotkey_vk_codes[2].store(config.emergency_stop.vk_code, Ordering::Release);
    }

    pub fn is_hotkey_vk(&self, vk_code: u16) -> bool {
        self.hotkey_vk_codes[0].load(Ordering::Relaxed) == vk_code
            || self.hotkey_vk_codes[1].load(Ordering::Relaxed) == vk_code
            || self.hotkey_vk_codes[2].load(Ordering::Relaxed) == vk_code
    }
}

pub static GLOBAL_STATE: OnceLock<Arc<AppState>> = OnceLock::new();

pub fn global_state() -> Option<Arc<AppState>> {
    GLOBAL_STATE.get().cloned()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub macro_directory: Option<String>,
}

fn settings_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("settings.json")
}

pub fn load_settings(app_data_dir: &Path) -> AppSettings {
    let path = settings_path(app_data_dir);
    fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AppSettings>(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(app_data_dir: &Path, settings: &AppSettings) -> Result<(), String> {
    fs::create_dir_all(app_data_dir)
        .map_err(|err| format!("Failed to create app data directory: {err}"))?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|err| format!("Failed to serialize settings: {err}"))?;
    fs::write(settings_path(app_data_dir), json)
        .map_err(|err| format!("Failed to save settings: {err}"))
}
