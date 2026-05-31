use crate::{
    app_state::{AppState, PlaybackOptions},
    engine::input,
    engine::worker::RunControl,
    macro_file::{MacroEvent, MouseButton},
};
#[cfg(not(test))]
use crate::{engine::TimerResolutionGuard, macro_file::MacroFile};
#[cfg(not(test))]
use serde::Serialize;
use std::collections::HashSet;
#[cfg(not(test))]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};
use std::{
    thread,
    time::{Duration, Instant},
};
#[cfg(not(test))]
use tauri::{AppHandle, Emitter};

#[cfg(all(windows, not(test)))]
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

#[cfg(not(test))]
pub struct PlayerRuntime {
    active: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

#[cfg(not(test))]
impl PlayerRuntime {
    pub fn stop(mut self, state: &AppState) {
        self.active.store(false, Ordering::Release);
        state.run_gen.fetch_add(1, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(not(test))]
#[derive(Serialize, Clone)]
struct PlaybackProgress {
    name: String,
    fired: u64,
    total: u64,
    loop_index: u32,
}

#[cfg(not(test))]
pub fn start_playback(
    state: Arc<AppState>,
    macro_file: MacroFile,
    app: AppHandle,
) -> Result<(), String> {
    if state.playing.swap(true, Ordering::AcqRel) {
        return Err("Playback is already running".to_string());
    }
    state.set_playing_macro(Some(macro_file.name.clone()))?;

    let generation = state.next_generation();
    let active = Arc::new(AtomicBool::new(true));
    let control = RunControl::new(state.clone(), generation, active.clone());
    let options = state
        .playback_options
        .lock()
        .map_err(|_| "Playback options lock was poisoned".to_string())?
        .clone()
        .sanitized();
    let macro_name = macro_file.name.clone();
    let total_events = macro_file.events.len() as u64;
    let initial_name = macro_file.name.clone();
    let app_for_thread = app.clone();
    let state_for_thread = state.clone();
    let macro_for_thread = macro_file.clone();

    let join = thread::spawn(move || {
        let _timer_guard = TimerResolutionGuard::request_500us();
        elevate_thread_priority();

        let result = playback_loop(&macro_for_thread, options, &control, &app_for_thread);
        state_for_thread.playing.store(false, Ordering::Release);
        state_for_thread
            .played_event_count
            .store(0, Ordering::Release);
        let _ = state_for_thread.set_playing_macro(None);

        match result {
            Ok(()) => {
                let _ = app_for_thread.emit("playback-stopped", macro_name);
            }
            Err(err) => {
                let _ = app_for_thread.emit("macro-error", err);
            }
        }
    });

    *state
        .player_runtime
        .lock()
        .map_err(|_| "Player runtime lock was poisoned".to_string())? = Some(PlayerRuntime {
        active,
        join: Some(join),
    });

    app.emit(
        "playback-progress",
        PlaybackProgress {
            name: initial_name,
            fired: 0,
            total: total_events,
            loop_index: 1,
        },
    )
    .ok();

    Ok(())
}

#[cfg(not(test))]
pub fn stop_playback(state: Arc<AppState>) -> Result<(), String> {
    state.playing.store(false, Ordering::Release);
    if let Some(runtime) = state
        .player_runtime
        .lock()
        .map_err(|_| "Player runtime lock was poisoned".to_string())?
        .take()
    {
        runtime.stop(&state);
    }
    state.set_playing_macro(None)?;
    Ok(())
}

pub fn is_playing_macro(state: &AppState, name: &str) -> bool {
    state
        .current_playing_macro()
        .as_deref()
        .is_some_and(|playing_name| playing_name == name)
}

#[cfg(not(test))]
fn playback_loop(
    macro_file: &MacroFile,
    options: PlaybackOptions,
    control: &RunControl,
    app: &AppHandle,
) -> Result<(), String> {
    if macro_file.events.is_empty() {
        return Ok(());
    }

    let total_events = macro_file.events.len() as u64;
    let mut loop_index = 0u32;

    loop {
        if !control.is_active() {
            break;
        }
        if !options.infinite_loop && loop_index >= options.loop_count {
            break;
        }

        loop_index += 1;
        let play_start = Instant::now();
        let mut cursor = PlaybackCursor::new(input::cursor_position());
        let mut held_inputs = PlaybackHeldInputs::default();
        let mut dispatch_error = None;

        for (index, event) in macro_file.events.iter().enumerate() {
            if !control.is_active() {
                break;
            }

            let scaled_ms = event.elapsed_ms() / options.speed_multiplier;
            wait_until(
                play_start + Duration::from_secs_f64(scaled_ms / 1000.0),
                control,
            );
            if !control.is_active() {
                break;
            }

            if let Err(err) = dispatch_event(event, &mut cursor, &mut held_inputs) {
                dispatch_error = Some(err);
                break;
            }
            let fired = index as u64 + 1;
            app.emit(
                "playback-progress",
                PlaybackProgress {
                    name: macro_file.name.clone(),
                    fired,
                    total: total_events,
                    loop_index,
                },
            )
            .ok();
        }

        let release_result = held_inputs.release_all();
        if let Some(err) = dispatch_error {
            return Err(err);
        }
        release_result?;
    }

    Ok(())
}

pub(crate) fn wait_until(target: Instant, control: &RunControl) {
    let spin_window = Duration::from_millis(2);

    loop {
        if !control.is_active() {
            return;
        }

        let now = Instant::now();
        if now >= target {
            return;
        }

        if target.saturating_duration_since(now) > spin_window {
            thread::sleep(Duration::from_millis(1));
        } else {
            break;
        }
    }

    while control.is_active() && Instant::now() < target {
        std::hint::spin_loop();
    }
}

#[cfg(test)]
pub(crate) fn playback_events_for_test<F>(
    events: &[MacroEvent],
    options: PlaybackOptions,
    control: &RunControl,
    mut on_event: F,
) -> Result<(), String>
where
    F: FnMut(usize, &MacroEvent, Instant, Instant) -> Result<(), String>,
{
    if events.is_empty() {
        return Ok(());
    }

    let options = options.sanitized();
    let mut loop_index = 0u32;

    loop {
        if !control.is_active() {
            break;
        }
        if !options.infinite_loop && loop_index >= options.loop_count {
            break;
        }

        loop_index += 1;
        let play_start = Instant::now();

        for (index, event) in events.iter().enumerate() {
            if !control.is_active() {
                break;
            }

            let scaled_ms = event.elapsed_ms() / options.speed_multiplier;
            wait_until(
                play_start + Duration::from_secs_f64(scaled_ms / 1000.0),
                control,
            );
            if !control.is_active() {
                break;
            }

            on_event(index, event, play_start, Instant::now())?;
        }
    }

    Ok(())
}

#[cfg(not(test))]
fn dispatch_event(
    event: &MacroEvent,
    cursor: &mut PlaybackCursor,
    held_inputs: &mut PlaybackHeldInputs,
) -> Result<(), String> {
    match event {
        MacroEvent::Delay { .. } => Ok(()),
        MacroEvent::MouseMove { x, y, .. } => move_cursor_to(*x, *y, cursor),
        MacroEvent::MouseDown { button, x, y, .. } => {
            move_cursor_to(*x, *y, cursor)?;
            if held_inputs.mouse_down(*button) {
                input::send_mouse_button(*button, true)
            } else {
                Ok(())
            }
        }
        MacroEvent::MouseUp { button, x, y, .. } => {
            move_cursor_to(*x, *y, cursor)?;
            if held_inputs.mouse_up(*button) {
                input::send_mouse_button(*button, false)
            } else {
                Ok(())
            }
        }
        MacroEvent::MouseScroll { delta, .. } => input::send_mouse_scroll(*delta),
        MacroEvent::KeyDown { vk_code, .. } => {
            if held_inputs.key_down(*vk_code) {
                input::send_key(*vk_code, true)
            } else {
                Ok(())
            }
        }
        MacroEvent::KeyUp { vk_code, .. } => {
            if held_inputs.key_up(*vk_code) {
                input::send_key(*vk_code, false)
            } else {
                Ok(())
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct PlaybackHeldInputs {
    keys_down: HashSet<u16>,
    mouse_buttons_down: HashSet<MouseButton>,
}

impl PlaybackHeldInputs {
    pub(crate) fn key_down(&mut self, vk_code: u16) -> bool {
        self.keys_down.insert(vk_code)
    }

    pub(crate) fn key_up(&mut self, vk_code: u16) -> bool {
        self.keys_down.remove(&vk_code)
    }

    pub(crate) fn mouse_down(&mut self, button: MouseButton) -> bool {
        self.mouse_buttons_down.insert(button)
    }

    pub(crate) fn mouse_up(&mut self, button: MouseButton) -> bool {
        self.mouse_buttons_down.remove(&button)
    }

    #[cfg(not(test))]
    fn release_all(&mut self) -> Result<(), String> {
        self.release_all_with(input::send_key, input::send_mouse_button)
    }

    #[cfg(test)]
    pub(crate) fn release_all_with<SendKey, SendMouse>(
        &mut self,
        mut send_key: SendKey,
        mut send_mouse_button: SendMouse,
    ) -> Result<(), String>
    where
        SendKey: FnMut(u16, bool) -> Result<(), String>,
        SendMouse: FnMut(MouseButton, bool) -> Result<(), String>,
    {
        let keys = self.keys_down.drain().collect::<Vec<_>>();
        for vk_code in keys {
            send_key(vk_code, false)?;
        }

        let buttons = self.mouse_buttons_down.drain().collect::<Vec<_>>();
        for button in buttons {
            send_mouse_button(button, false)?;
        }

        Ok(())
    }

    #[cfg(not(test))]
    fn release_all_with<SendKey, SendMouse>(
        &mut self,
        mut send_key: SendKey,
        mut send_mouse_button: SendMouse,
    ) -> Result<(), String>
    where
        SendKey: FnMut(u16, bool) -> Result<(), String>,
        SendMouse: FnMut(MouseButton, bool) -> Result<(), String>,
    {
        let keys = self.keys_down.drain().collect::<Vec<_>>();
        for vk_code in keys {
            send_key(vk_code, false)?;
        }

        let buttons = self.mouse_buttons_down.drain().collect::<Vec<_>>();
        for button in buttons {
            send_mouse_button(button, false)?;
        }

        Ok(())
    }
}

#[cfg(test)]
impl PlaybackHeldInputs {
    pub(crate) fn release_all(&mut self) -> Result<(), String> {
        self.release_all_with(|_, _| Ok(()), |_, _| Ok(()))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlaybackCursor {
    current: Option<(i32, i32)>,
    needs_anchor: bool,
}

impl PlaybackCursor {
    pub(crate) fn new(current: Option<(i32, i32)>) -> Self {
        Self {
            current,
            needs_anchor: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<(i32, i32)> {
        self.current
    }
}

pub(crate) fn move_cursor_to_with<SetCursor, ActivateWindow, ReadCursor, SendMove>(
    x: i32,
    y: i32,
    cursor: &mut PlaybackCursor,
    mut set_cursor: SetCursor,
    mut activate_window: ActivateWindow,
    mut read_cursor: ReadCursor,
    send_move: SendMove,
) -> Result<(), String>
where
    SetCursor: FnMut(i32, i32) -> Result<(), String>,
    ActivateWindow: FnMut(i32, i32),
    ReadCursor: FnMut() -> Option<(i32, i32)>,
    SendMove: FnMut(i32, i32) -> Result<(), String>,
{
    if cursor.needs_anchor {
        set_cursor(x, y)?;
        activate_window(x, y);
        cursor.current = read_cursor().or(Some((x, y)));
        cursor.needs_anchor = false;
        return Ok(());
    }

    input::drive_cursor_to(x, y, &mut cursor.current, read_cursor, send_move)
}

#[cfg(test)]
pub(crate) fn should_dispatch_keyboard_event(
    keys_down: &mut HashSet<u16>,
    event: &MacroEvent,
) -> bool {
    match event {
        MacroEvent::KeyDown { vk_code, .. } => keys_down.insert(*vk_code),
        MacroEvent::KeyUp { vk_code, .. } => keys_down.remove(vk_code),
        _ => true,
    }
}

#[cfg(not(test))]
fn move_cursor_to(x: i32, y: i32, cursor: &mut PlaybackCursor) -> Result<(), String> {
    move_cursor_to_with(
        x,
        y,
        cursor,
        input::set_cursor_position,
        |target_x, target_y| {
            let _ = input::activate_window_at(target_x, target_y);
        },
        input::cursor_position,
        input::send_mouse_move,
    )
}

#[cfg(not(test))]
fn elevate_thread_priority() {
    #[cfg(windows)]
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
    }
}
