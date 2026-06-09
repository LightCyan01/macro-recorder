use crate::{
    app_state::{global_state, AppState},
    macro_file::MacroEvent,
};
#[cfg(not(test))]
use crate::{
    engine::TimerResolutionGuard,
    macro_file::{self, MacroFile, MouseButton},
};
use std::{collections::HashSet, sync::atomic::Ordering, time::Instant};
#[cfg(not(test))]
use std::{
    sync::{atomic::AtomicU32, mpsc, Arc, Mutex, OnceLock},
    thread::{self, JoinHandle},
    time::Duration,
};
#[cfg(not(test))]
use tauri::{AppHandle, Emitter};

#[cfg(all(windows, not(test)))]
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
        WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN,
        WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
};

#[cfg(all(windows, not(test)))]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(all(windows, not(test)))]
use windows::Win32::System::Threading::GetCurrentThreadId;

#[cfg(not(test))]
enum RawHookEvent {
    Key {
        vk_code: u16,
        down: bool,
        timestamp: Instant,
    },
    MouseMove {
        x: i32,
        y: i32,
        timestamp: Instant,
    },
    MouseButton {
        button: MouseButton,
        down: bool,
        x: i32,
        y: i32,
        timestamp: Instant,
    },
    MouseScroll {
        delta: i32,
        timestamp: Instant,
    },
}

#[cfg(not(test))]
static HOOK_SENDER: OnceLock<Mutex<Option<mpsc::SyncSender<RawHookEvent>>>> =
    OnceLock::new();

#[cfg(not(test))]
fn hook_sender_slot() -> &'static Mutex<Option<mpsc::SyncSender<RawHookEvent>>> {
    HOOK_SENDER.get_or_init(|| Mutex::new(None))
}

#[cfg(not(test))]
fn try_send_hook_event(event: RawHookEvent) {
    if let Ok(guard) = hook_sender_slot().lock() {
        if let Some(sender) = guard.as_ref() {
            let _ = sender.try_send(event);
        }
    }
}

#[cfg(not(test))]
pub struct RecorderRuntime {
    thread_id: Arc<AtomicU32>,
    join: Option<JoinHandle<()>>,
    drain_join: Option<JoinHandle<()>>,
}

#[cfg(not(test))]
impl RecorderRuntime {
    pub fn stop(mut self) {
        let thread_id = self.thread_id.load(Ordering::Acquire);
        if thread_id != 0 {
            #[cfg(windows)]
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }

        if let Some(join) = self.join.take() {
            let _ = join.join();
        }

        if let Ok(mut guard) = hook_sender_slot().lock() {
            *guard = None;
        }

        if let Some(join) = self.drain_join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(not(test))]
#[derive(serde::Serialize, Clone)]
struct RecordingCount {
    count: u64,
}

#[cfg(not(test))]
pub fn start_recording(state: Arc<AppState>) -> Result<(), String> {
    if state.recording.swap(true, Ordering::AcqRel) {
        return Err("Recording is already running".to_string());
    }

    state.recorded_event_count.store(0, Ordering::Release);
    state
        .events_buf
        .lock()
        .map_err(|_| "Recording buffer lock was poisoned".to_string())?
        .clear();
    state
        .recording_keys_down
        .lock()
        .map_err(|_| "Recording key state lock was poisoned".to_string())?
        .clear();

    let (event_tx, event_rx) = mpsc::sync_channel::<RawHookEvent>(4096);

    if let Ok(mut guard) = hook_sender_slot().lock() {
        *guard = Some(event_tx);
    }

    let thread_id = Arc::new(AtomicU32::new(0));
    let thread_id_for_runtime = thread_id.clone();
    let state_for_thread = state.clone();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let recording_start = Instant::now();
    if let Ok(mut start_guard) = state.recording_started.lock() {
        *start_guard = Some(recording_start);
    }

    let state_for_drain = state.clone();
    let drain_join = thread::spawn(move || {
        drain_loop(state_for_drain, event_rx, recording_start);
    });

    let join = thread::spawn(move || {
        let _timer_guard = TimerResolutionGuard::request_500us();

        run_hook_thread(state_for_thread.clone(), thread_id, ready_tx);

        state_for_thread.recording.store(false, Ordering::Release);
        if let Ok(mut start_guard) = state_for_thread.recording_started.lock() {
            *start_guard = None;
        }
    });

    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => {
            *state
                .recorder_runtime
                .lock()
                .map_err(|_| "Recorder runtime lock was poisoned".to_string())? =
                Some(RecorderRuntime {
                    thread_id: thread_id_for_runtime,
                    join: Some(join),
                    drain_join: Some(drain_join),
                });
            emit_recording_count(&state, 0);
            Ok(())
        }
        Ok(Err(err)) => {
            state.recording.store(false, Ordering::Release);
            if let Ok(mut guard) = hook_sender_slot().lock() {
                *guard = None;
            }
            let _ = join.join();
            let _ = drain_join.join();
            Err(err)
        }
        Err(_) => {
            state.recording.store(false, Ordering::Release);
            let thread_id = thread_id_for_runtime.load(Ordering::Acquire);
            if thread_id != 0 {
                #[cfg(windows)]
                unsafe {
                    let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
                }
            }
            if let Ok(mut guard) = hook_sender_slot().lock() {
                *guard = None;
            }
            let _ = join.join();
            let _ = drain_join.join();
            Err("Timed out while starting recorder hook thread".to_string())
        }
    }
}

#[cfg(not(test))]
fn drain_loop(
    state: Arc<AppState>,
    rx: mpsc::Receiver<RawHookEvent>,
    recording_start: Instant,
) {
    let mut last_emit = Instant::now();
    let emit_interval = Duration::from_millis(50);

    loop {
        let event = match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(event) => Some(event),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if !state.recording.load(Ordering::Acquire) {
            break;
        }

        if let Some(event) = event {
            let elapsed_ms = event_timestamp(&event)
                .duration_since(recording_start)
                .as_secs_f64()
                * 1000.0;

            let macro_event = match event {
                RawHookEvent::Key { vk_code, down, .. } => {
                    if down {
                        Some(MacroEvent::KeyDown { vk_code, elapsed_ms })
                    } else {
                        Some(MacroEvent::KeyUp { vk_code, elapsed_ms })
                    }
                }
                RawHookEvent::MouseMove { x, y, .. } => {
                    Some(MacroEvent::MouseMove { x, y, elapsed_ms })
                }
                RawHookEvent::MouseButton { button, down, x, y, .. } => {
                    if down {
                        Some(MacroEvent::MouseDown { button, x, y, elapsed_ms })
                    } else {
                        Some(MacroEvent::MouseUp { button, x, y, elapsed_ms })
                    }
                }
                RawHookEvent::MouseScroll { delta, .. } => {
                    Some(MacroEvent::MouseScroll { delta, elapsed_ms })
                }
            };

            if let Some(event) = macro_event {
                if push_recorded_event(&state, event) {
                    state.recorded_event_count.fetch_add(1, Ordering::AcqRel);
                }
            }
        }

        let now = Instant::now();
        if now.duration_since(last_emit) >= emit_interval {
            let count = state.recorded_event_count.load(Ordering::Acquire);
            emit_recording_count(&state, count);
            last_emit = now;
        }
    }

    let count = state.recorded_event_count.load(Ordering::Acquire);
    emit_recording_count(&state, count);
}

#[cfg(not(test))]
fn event_timestamp(event: &RawHookEvent) -> Instant {
    match event {
        RawHookEvent::Key { timestamp, .. }
        | RawHookEvent::MouseMove { timestamp, .. }
        | RawHookEvent::MouseButton { timestamp, .. }
        | RawHookEvent::MouseScroll { timestamp, .. } => *timestamp,
    }
}

#[cfg(not(test))]
pub fn stop_recording(
    state: Arc<AppState>,
    app: AppHandle,
    name: impl Into<String>,
) -> Result<MacroFile, String> {
    if !state.recording.swap(false, Ordering::AcqRel) {
        return Err("Recording is not running".to_string());
    }

    let duration_ms = elapsed_ms(&state).unwrap_or_default();

    if let Some(runtime) = state
        .recorder_runtime
        .lock()
        .map_err(|_| "Recorder runtime lock was poisoned".to_string())?
        .take()
    {
        runtime.stop();
    }

    let mut keys_down = state
        .recording_keys_down
        .lock()
        .map_err(|_| "Recording key state lock was poisoned".to_string())?;
    let mut events_guard = state
        .events_buf
        .lock()
        .map_err(|_| "Recording buffer lock was poisoned".to_string())?;
    append_missing_keyups(&mut events_guard, &mut keys_down, duration_ms);
    let events = events_guard.clone();
    drop(events_guard);
    drop(keys_down);

    let macro_file = MacroFile::from_recording(name, events, duration_ms);
    let dir = state.macro_dir()?;
    macro_file::save_macro(&dir, &macro_file)?;

    let macros = macro_file::load_macros(&dir)?;
    state.replace_macro_cache(macros.clone())?;

    app.emit("macros-updated", macros).ok();
    app.emit("recording-saved", macro_file.clone()).ok();
    Ok(macro_file)
}

#[cfg(not(test))]
pub fn stop_recording_without_save(state: Arc<AppState>) -> Result<(), String> {
    state.recording.store(false, Ordering::Release);
    if let Some(runtime) = state
        .recorder_runtime
        .lock()
        .map_err(|_| "Recorder runtime lock was poisoned".to_string())?
        .take()
    {
        runtime.stop();
    }
    Ok(())
}

#[cfg(not(test))]
pub fn stop_recording_autosave(state: Arc<AppState>, app: AppHandle) -> Result<MacroFile, String> {
    let name = format!(
        "Recording {}",
        chrono::Utc::now().format("%Y-%m-%d %H-%M-%S")
    );
    stop_recording(state, app, name)
}

#[cfg(not(test))]
fn emit_recording_count(state: &AppState, count: u64) {
    if let Some(app) = state.app_handle() {
        app.emit("recording-event-count", RecordingCount { count })
            .ok();
    }
}

#[cfg(test)]
pub(crate) fn record_keyboard_event_for_test(state: &AppState, vk_code: u16) {
    record_keyboard_transition_for_test(state, vk_code, true);
}

#[cfg(test)]
pub(crate) fn record_keyboard_transition_for_test(state: &AppState, vk_code: u16, down: bool) {
    if !state.recording.load(Ordering::Acquire) {
        return;
    }

    if state.is_hotkey_vk(vk_code) {
        return;
    }

    let Some(elapsed_ms) = elapsed_ms(state) else {
        return;
    };

    let event = if down {
        MacroEvent::KeyDown {
            vk_code,
            elapsed_ms,
        }
    } else {
        MacroEvent::KeyUp {
            vk_code,
            elapsed_ms,
        }
    };
    push_recorded_event(state, event);
}

#[cfg(test)]
pub(crate) fn reset_recording_clock_for_test(state: &AppState) {
    if let Ok(mut start) = state.recording_started.lock() {
        *start = Some(Instant::now());
    }
}

fn elapsed_ms(state: &AppState) -> Option<f64> {
    state.recording_started.lock().ok().and_then(|start| {
        start
            .as_ref()
            .map(|instant| instant.elapsed().as_secs_f64() * 1000.0)
    })
}

fn push_recorded_event(state: &AppState, event: MacroEvent) -> bool {
    if !should_keep_recorded_event(state, &event) {
        return false;
    }

    let Ok(mut events) = state.events_buf.lock() else {
        return false;
    };
    events.push(event);
    true
}

fn should_keep_recorded_event(state: &AppState, event: &MacroEvent) -> bool {
    match event {
        MacroEvent::KeyDown { vk_code, .. } => state
            .recording_keys_down
            .lock()
            .map(|mut keys_down| keys_down.insert(*vk_code))
            .unwrap_or(false),
        MacroEvent::KeyUp { vk_code, .. } => state
            .recording_keys_down
            .lock()
            .map(|mut keys_down| keys_down.remove(vk_code))
            .unwrap_or(false),
        _ => true,
    }
}

pub(crate) fn append_missing_keyups(
    events: &mut Vec<MacroEvent>,
    keys_down: &mut HashSet<u16>,
    elapsed_ms: f64,
) {
    let mut keys = keys_down.drain().collect::<Vec<_>>();
    keys.sort_unstable();

    for vk_code in keys {
        events.push(MacroEvent::KeyUp {
            vk_code,
            elapsed_ms,
        });
    }
}

#[cfg(all(windows, not(test)))]
fn is_suppressed_hotkey(vk_code: u16) -> bool {
    let Some(state) = global_state() else {
        return false;
    };
    state.is_hotkey_vk(vk_code)
}

pub(crate) fn should_suppress_hotkey_vk(
    vk_code: u16,
    config: &crate::hotkeys::HotkeyConfig,
) -> bool {
    [
        config.record_toggle.vk_code,
        config.play_toggle.vk_code,
        config.emergency_stop.vk_code,
    ]
    .contains(&vk_code)
}

#[cfg(all(windows, not(test)))]
fn run_hook_thread(
    state: Arc<AppState>,
    thread_id: Arc<AtomicU32>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) {
    unsafe {
        thread_id.store(GetCurrentThreadId(), Ordering::Release);
    }

    let hmod = unsafe { GetModuleHandleW(None).unwrap_or_default() };

    let keyboard_hook = unsafe {
        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) {
            Ok(hook) => hook,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("Failed to install keyboard hook: {err}")));
                return;
            }
        }
    };
    let mouse_hook = unsafe {
        match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0) {
            Ok(hook) => hook,
            Err(err) => {
                unhook(keyboard_hook);
                let _ = ready_tx.send(Err(format!("Failed to install mouse hook: {err}")));
                return;
            }
        }
    };

    let _ = ready_tx.send(Ok(()));

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        if !state.recording.load(Ordering::Acquire) {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    unhook(keyboard_hook);
    unhook(mouse_hook);
}

#[cfg(all(not(windows), not(test)))]
fn run_hook_thread(
    _state: Arc<AppState>,
    _thread_id: Arc<AtomicU32>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) {
    let _ = ready_tx.send(Err(
        "Recording hooks are only available on Windows".to_string()
    ));
}

#[cfg(all(windows, not(test)))]
fn unhook(hook: HHOOK) {
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

#[cfg(all(windows, not(test)))]
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let keyboard = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk_code = keyboard.vkCode as u16;

        if !is_suppressed_hotkey(vk_code) {
            let down = match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
                WM_KEYUP | WM_SYSKEYUP => Some(false),
                _ => None,
            };

            if let Some(down) = down {
                try_send_hook_event(RawHookEvent::Key {
                    vk_code,
                    down,
                    timestamp: Instant::now(),
                });
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

#[cfg(all(windows, not(test)))]
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let mouse = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let x = mouse.pt.x;
        let y = mouse.pt.y;
        let timestamp = Instant::now();

        let event = match wparam.0 as u32 {
            WM_MOUSEMOVE => Some(RawHookEvent::MouseMove { x, y, timestamp }),
            WM_LBUTTONDOWN => Some(RawHookEvent::MouseButton {
                button: MouseButton::Left,
                down: true,
                x,
                y,
                timestamp,
            }),
            WM_LBUTTONUP => Some(RawHookEvent::MouseButton {
                button: MouseButton::Left,
                down: false,
                x,
                y,
                timestamp,
            }),
            WM_RBUTTONDOWN => Some(RawHookEvent::MouseButton {
                button: MouseButton::Right,
                down: true,
                x,
                y,
                timestamp,
            }),
            WM_RBUTTONUP => Some(RawHookEvent::MouseButton {
                button: MouseButton::Right,
                down: false,
                x,
                y,
                timestamp,
            }),
            WM_MBUTTONDOWN => Some(RawHookEvent::MouseButton {
                button: MouseButton::Middle,
                down: true,
                x,
                y,
                timestamp,
            }),
            WM_MBUTTONUP => Some(RawHookEvent::MouseButton {
                button: MouseButton::Middle,
                down: false,
                x,
                y,
                timestamp,
            }),
            WM_MOUSEWHEEL => {
                let delta = ((mouse.mouseData >> 16) & 0xffff) as i16 as i32;
                Some(RawHookEvent::MouseScroll { delta, timestamp })
            }
            _ => None,
        };

        if let Some(event) = event {
            try_send_hook_event(event);
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}
