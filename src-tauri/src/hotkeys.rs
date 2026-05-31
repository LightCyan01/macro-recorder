#[cfg(not(test))]
use crate::{
    app_state::AppState,
    engine::{player, recorder},
    macro_file,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
#[cfg(not(test))]
use std::{
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
    time::Duration,
};
#[cfg(not(test))]
use tauri::{AppHandle, Emitter};

#[cfg(all(windows, not(test)))]
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
        MOD_SHIFT,
    },
    UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW, TranslateMessage, MSG,
        PM_NOREMOVE, WM_APP, WM_HOTKEY, WM_USER,
    },
};

#[cfg(all(windows, not(test)))]
use windows::Win32::System::Threading::GetCurrentThreadId;

pub const HOTKEY_ID_RECORD: i32 = 1;
pub const HOTKEY_ID_PLAY: i32 = 2;
pub const HOTKEY_ID_STOP: i32 = 3;

#[cfg(all(windows, not(test)))]
const WM_APP_HOTKEY_COMMAND: u32 = WM_APP + 41;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HotkeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub key: String,
    pub vk_code: u16,
    pub modifiers: HotkeyModifiers,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub record_toggle: HotkeyBinding,
    pub play_toggle: HotkeyBinding,
    pub emergency_stop: HotkeyBinding,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            record_toggle: HotkeyBinding {
                key: "F9".to_string(),
                vk_code: 0x78,
                modifiers: HotkeyModifiers::none(),
            },
            play_toggle: HotkeyBinding {
                key: "F10".to_string(),
                vk_code: 0x79,
                modifiers: HotkeyModifiers::none(),
            },
            emergency_stop: HotkeyBinding {
                key: "F11".to_string(),
                vk_code: 0x7A,
                modifiers: HotkeyModifiers::none(),
            },
        }
    }
}

impl HotkeyModifiers {
    pub fn none() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
        }
    }
}

#[cfg(not(test))]
enum HotkeyCommand {
    Apply(HotkeyConfig, mpsc::Sender<Result<(), String>>),
    Stop,
}

#[cfg(not(test))]
pub struct HotkeyRuntime {
    sender: mpsc::Sender<HotkeyCommand>,
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

#[cfg(not(test))]
impl HotkeyRuntime {
    pub fn apply(&self, config: HotkeyConfig) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(HotkeyCommand::Apply(config, reply_tx))
            .map_err(|_| "Hotkey thread is not available".to_string())?;
        self.wake();
        reply_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "Timed out while applying hotkeys".to_string())?
    }

    pub fn stop(mut self) {
        let _ = self.sender.send(HotkeyCommand::Stop);
        self.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    fn wake(&self) {
        #[cfg(all(windows, not(test)))]
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_APP_HOTKEY_COMMAND, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn config_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("hotkeys.json")
}

pub fn load_or_default(app_data_dir: &Path) -> HotkeyConfig {
    let path = config_path(app_data_dir);
    fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<HotkeyConfig>(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_hotkeys(app_data_dir: &Path, config: &HotkeyConfig) -> Result<(), String> {
    fs::create_dir_all(app_data_dir)
        .map_err(|err| format!("Failed to create app data directory: {err}"))?;
    let json = serde_json::to_string_pretty(config)
        .map_err(|err| format!("Failed to serialize hotkeys: {err}"))?;
    fs::write(config_path(app_data_dir), json)
        .map_err(|err| format!("Failed to save hotkeys: {err}"))
}

pub fn validate_config(config: &HotkeyConfig) -> Result<(), String> {
    let bindings = [
        ("Record Toggle", &config.record_toggle),
        ("Play Toggle", &config.play_toggle),
        ("Emergency Stop", &config.emergency_stop),
    ];

    for (label, binding) in bindings {
        if binding.vk_code == 0 {
            return Err(format!("{label} must include a non-modifier key"));
        }
    }

    for left_index in 0..bindings.len() {
        for right_index in left_index + 1..bindings.len() {
            let (left_label, left) = bindings[left_index];
            let (right_label, right) = bindings[right_index];
            if left.vk_code == right.vk_code && left.modifiers == right.modifiers {
                return Err(format!("{left_label} conflicts with {right_label}"));
            }
        }
    }

    Ok(())
}

#[cfg(not(test))]
pub fn start_hotkey_thread(state: Arc<AppState>, app: AppHandle) -> Result<HotkeyRuntime, String> {
    #[cfg(not(windows))]
    {
        let _ = (state, app);
        return Err("Global hotkeys are only available on Windows".to_string());
    }

    #[cfg(windows)]
    {
        let (cmd_tx, cmd_rx) = mpsc::channel::<HotkeyCommand>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();

        let join = thread::spawn(move || {
            let mut seed = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut seed, None, WM_USER, WM_USER, PM_NOREMOVE);
            }
            let thread_id = unsafe { GetCurrentThreadId() };
            let current_config = state
                .hotkeys
                .lock()
                .map(|config| config.clone())
                .unwrap_or_default();

            if let Err(err) = register_all(&current_config) {
                let _ = ready_tx.send(Err(err));
                return;
            }

            let _ = ready_tx.send(Ok(thread_id));
            hotkey_message_loop(state, app, cmd_rx, current_config);
            unregister_all();
        });

        let thread_id = ready_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "Timed out while starting hotkey thread".to_string())??;

        Ok(HotkeyRuntime {
            sender: cmd_tx,
            thread_id,
            join: Some(join),
        })
    }
}

#[cfg(all(windows, not(test)))]
fn hotkey_message_loop(
    state: Arc<AppState>,
    app: AppHandle,
    cmd_rx: mpsc::Receiver<HotkeyCommand>,
    mut registered_config: HotkeyConfig,
) {
    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }

        match message.message {
            WM_HOTKEY => handle_hotkey(message.wParam.0 as i32, &state, &app),
            WM_APP_HOTKEY_COMMAND => {
                if handle_commands(&cmd_rx, &mut registered_config) {
                    break;
                }
            }
            _ => unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            },
        }
    }
}

#[cfg(all(windows, not(test)))]
fn handle_commands(
    cmd_rx: &mpsc::Receiver<HotkeyCommand>,
    registered_config: &mut HotkeyConfig,
) -> bool {
    let mut should_stop = false;

    while let Ok(command) = cmd_rx.try_recv() {
        match command {
            HotkeyCommand::Apply(config, reply_tx) => {
                unregister_all();
                match register_all(&config) {
                    Ok(()) => {
                        *registered_config = config;
                        let _ = reply_tx.send(Ok(()));
                    }
                    Err(err) => {
                        let restore_result = register_all(registered_config);
                        let message = match restore_result {
                            Ok(()) => err,
                            Err(restore_err) => {
                                format!("{err}; failed to restore previous hotkeys: {restore_err}")
                            }
                        };
                        let _ = reply_tx.send(Err(message));
                    }
                }
            }
            HotkeyCommand::Stop => should_stop = true,
        }
    }

    should_stop
}

#[cfg(all(windows, not(test)))]
fn handle_hotkey(id: i32, state: &Arc<AppState>, app: &AppHandle) {
    let result = match id {
        HOTKEY_ID_RECORD => {
            if state.recording.load(std::sync::atomic::Ordering::Acquire) {
                recorder::stop_recording_autosave(state.clone(), app.clone()).map(|_| ())
            } else {
                recorder::start_recording(state.clone())
            }
        }
        HOTKEY_ID_PLAY => {
            if state.playing.load(std::sync::atomic::Ordering::Acquire) {
                player::stop_playback(state.clone())
            } else {
                let macro_file = state
                    .macros
                    .lock()
                    .ok()
                    .and_then(|macros| macros.first().cloned())
                    .ok_or_else(|| "No macro is available to play".to_string());
                macro_file.and_then(|macro_file| {
                    player::start_playback(state.clone(), macro_file, app.clone())
                })
            }
        }
        HOTKEY_ID_STOP => {
            let playback = player::stop_playback(state.clone());
            let recording = recorder::stop_recording_without_save(state.clone());
            playback.and(recording)
        }
        _ => Ok(()),
    };

    if let Err(err) = result {
        let _ = app.emit("macro-error", err);
    }
}

#[cfg(all(windows, not(test)))]
fn register_all(config: &HotkeyConfig) -> Result<(), String> {
    validate_config(config)?;
    register_hotkey(HOTKEY_ID_RECORD, &config.record_toggle)?;
    if let Err(err) = register_hotkey(HOTKEY_ID_PLAY, &config.play_toggle) {
        unregister_all();
        return Err(err);
    }
    if let Err(err) = register_hotkey(HOTKEY_ID_STOP, &config.emergency_stop) {
        unregister_all();
        return Err(err);
    }
    Ok(())
}

#[cfg(all(windows, not(test)))]
fn unregister_all() {
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID_RECORD);
        let _ = UnregisterHotKey(None, HOTKEY_ID_PLAY);
        let _ = UnregisterHotKey(None, HOTKEY_ID_STOP);
    }
}

#[cfg(all(windows, not(test)))]
fn register_hotkey(id: i32, binding: &crate::hotkeys::HotkeyBinding) -> Result<(), String> {
    let mut modifiers = MOD_NOREPEAT.0;
    if binding.modifiers.ctrl {
        modifiers |= MOD_CONTROL.0;
    }
    if binding.modifiers.alt {
        modifiers |= MOD_ALT.0;
    }
    if binding.modifiers.shift {
        modifiers |= MOD_SHIFT.0;
    }

    unsafe {
        RegisterHotKey(
            None,
            id,
            HOT_KEY_MODIFIERS(modifiers),
            binding.vk_code as u32,
        )
    }
    .map_err(|_| {
        format!(
            "Failed to register hotkey '{}': it may be in use by another application",
            binding.key
        )
    })
}

#[cfg(not(test))]
pub fn reload_macro_cache(state: &AppState) -> Result<Vec<macro_file::MacroFile>, String> {
    state.refresh_macros_from_disk()
}
