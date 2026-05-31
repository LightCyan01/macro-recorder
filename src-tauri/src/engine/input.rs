use crate::macro_file::MouseButton;
#[cfg(test)]
use std::cell::Cell;

#[cfg(windows)]
use windows::Win32::{
    Foundation::POINT,
    UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
        MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEEVENTF_WHEEL, MOUSEINPUT, VIRTUAL_KEY,
    },
    UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos, SetForegroundWindow, WindowFromPoint},
};

#[cfg(windows)]
fn send_input(input: INPUT) -> Result<(), String> {
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent == 1 {
        Ok(())
    } else {
        Err(format!(
            "SendInput failed: {}",
            windows::core::Error::from_win32()
        ))
    }
}

#[cfg(windows)]
pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point).is_ok() } {
        Some((point.x, point.y))
    } else {
        None
    }
}

#[cfg(not(windows))]
pub fn cursor_position() -> Option<(i32, i32)> {
    None
}

#[cfg(windows)]
pub fn set_cursor_position(x: i32, y: i32) -> Result<(), String> {
    unsafe { SetCursorPos(x, y) }
        .map_err(|err| format!("Failed to position cursor before playback: {err}"))
}

#[cfg(not(windows))]
pub fn set_cursor_position(_x: i32, _y: i32) -> Result<(), String> {
    Err("Mouse playback is only available on Windows".to_string())
}

#[cfg(windows)]
pub fn activate_window_at(x: i32, y: i32) -> bool {
    let hwnd = unsafe { WindowFromPoint(POINT { x, y }) };
    if hwnd.0.is_null() {
        return false;
    }

    unsafe { SetForegroundWindow(hwnd).as_bool() }
}

#[cfg(not(windows))]
pub fn activate_window_at(_x: i32, _y: i32) -> bool {
    false
}

#[cfg(windows)]
pub fn send_mouse_move(dx: i32, dy: i32) -> Result<(), String> {
    if dx == 0 && dy == 0 {
        return Ok(());
    }

    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_input(input)
}

#[cfg(not(windows))]
pub fn send_mouse_move(_dx: i32, _dy: i32) -> Result<(), String> {
    Err("Mouse playback is only available on Windows".to_string())
}

#[cfg(windows)]
pub fn send_mouse_button(button: MouseButton, down: bool) -> Result<(), String> {
    let dw_flags = match (button, down) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
    };

    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_input(input)
}

#[cfg(not(windows))]
pub fn send_mouse_button(_button: MouseButton, _down: bool) -> Result<(), String> {
    Err("Mouse playback is only available on Windows".to_string())
}

#[cfg(windows)]
pub fn send_mouse_scroll(delta: i32) -> Result<(), String> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: delta as u32,
                dwFlags: MOUSEEVENTF_WHEEL,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_input(input)
}

#[cfg(not(windows))]
pub fn send_mouse_scroll(_delta: i32) -> Result<(), String> {
    Err("Mouse playback is only available on Windows".to_string())
}

#[cfg(windows)]
pub fn send_key(vk_code: u16, down: bool) -> Result<(), String> {
    let flags = if down {
        KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };

    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk_code),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_input(input)
}

#[cfg(not(windows))]
pub fn send_key(_vk_code: u16, _down: bool) -> Result<(), String> {
    Err("Keyboard playback is only available on Windows".to_string())
}

pub(crate) fn mouse_delta(from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
    (to.0 - from.0, to.1 - from.1)
}

pub(crate) fn drive_cursor_to<ReadCursor, SendMove>(
    target_x: i32,
    target_y: i32,
    cursor: &mut Option<(i32, i32)>,
    mut read_cursor: ReadCursor,
    mut send_move: SendMove,
) -> Result<(), String>
where
    ReadCursor: FnMut() -> Option<(i32, i32)>,
    SendMove: FnMut(i32, i32) -> Result<(), String>,
{
    let target = (target_x, target_y);
    let mut current = cursor
        .as_ref()
        .copied()
        .or_else(&mut read_cursor)
        .unwrap_or(target);

    for _ in 0..16 {
        let (dx, dy) = mouse_delta(current, target);
        if dx == 0 && dy == 0 {
            *cursor = Some(current);
            return Ok(());
        }

        send_move(dx, dy)?;
        let reported = read_cursor().unwrap_or(target);
        current = reported;
    }

    let (dx, dy) = mouse_delta(current, target);
    if dx != 0 || dy != 0 {
        send_move(dx, dy)?;
        current = read_cursor().unwrap_or(target);
    }

    *cursor = Some(current);
    Ok(())
}

#[cfg(test)]
pub(crate) fn simulated_cursor_drive(
    start: (i32, i32),
    target: (i32, i32),
    divisor: i32,
) -> ((i32, i32), Vec<(i32, i32)>) {
    let actual = Cell::new(start);
    let mut cursor = Some(start);
    let mut deltas = Vec::new();

    drive_cursor_to(
        target.0,
        target.1,
        &mut cursor,
        || Some(actual.get()),
        |dx, dy| {
            deltas.push((dx, dy));
            let current = actual.get();
            let step_x = if dx == 0 {
                0
            } else {
                dx.signum() * (dx.abs() / divisor).max(1)
            };
            let step_y = if dy == 0 {
                0
            } else {
                dy.signum() * (dy.abs() / divisor).max(1)
            };
            actual.set((current.0 + step_x, current.1 + step_y));
            Ok(())
        },
    )
    .expect("simulated cursor drive should not fail");

    (cursor.unwrap_or(start), deltas)
}
