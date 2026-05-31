use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type")]
pub enum MacroEvent {
    Delay {
        duration_ms: f64,
        elapsed_ms: f64,
    },
    MouseMove {
        x: i32,
        y: i32,
        elapsed_ms: f64,
    },
    MouseDown {
        button: MouseButton,
        x: i32,
        y: i32,
        elapsed_ms: f64,
    },
    MouseUp {
        button: MouseButton,
        x: i32,
        y: i32,
        elapsed_ms: f64,
    },
    MouseScroll {
        delta: i32,
        elapsed_ms: f64,
    },
    KeyDown {
        vk_code: u16,
        elapsed_ms: f64,
    },
    KeyUp {
        vk_code: u16,
        elapsed_ms: f64,
    },
}

impl MacroEvent {
    pub fn elapsed_ms(&self) -> f64 {
        match self {
            Self::Delay { elapsed_ms, .. }
            | Self::MouseMove { elapsed_ms, .. }
            | Self::MouseDown { elapsed_ms, .. }
            | Self::MouseUp { elapsed_ms, .. }
            | Self::MouseScroll { elapsed_ms, .. }
            | Self::KeyDown { elapsed_ms, .. }
            | Self::KeyUp { elapsed_ms, .. } => *elapsed_ms,
        }
    }

    pub fn set_elapsed_ms(&mut self, next_elapsed_ms: f64) {
        match self {
            Self::Delay { elapsed_ms, .. }
            | Self::MouseMove { elapsed_ms, .. }
            | Self::MouseDown { elapsed_ms, .. }
            | Self::MouseUp { elapsed_ms, .. }
            | Self::MouseScroll { elapsed_ms, .. }
            | Self::KeyDown { elapsed_ms, .. }
            | Self::KeyUp { elapsed_ms, .. } => *elapsed_ms = next_elapsed_ms,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MacroFile {
    pub name: String,
    pub created_at: String,
    pub duration_ms: f64,
    pub events: Vec<MacroEvent>,
}

impl MacroFile {
    #[cfg(test)]
    pub fn new(name: impl Into<String>, events: Vec<MacroEvent>) -> Self {
        let duration_ms = events
            .iter()
            .map(MacroEvent::elapsed_ms)
            .fold(0.0, f64::max);

        Self::from_recording(name, events, duration_ms)
    }

    pub fn from_recording(
        name: impl Into<String>,
        events: Vec<MacroEvent>,
        duration_ms: f64,
    ) -> Self {
        let requested_duration_ms = finite_non_negative_or_zero(duration_ms);
        let events = materialize_timeline_events(events, requested_duration_ms, false)
            .unwrap_or_else(|_| Vec::new());
        let duration_ms = events
            .last()
            .map(MacroEvent::elapsed_ms)
            .unwrap_or_default()
            .max(requested_duration_ms);
        Self {
            name: sanitize_macro_name(&name.into()),
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            duration_ms,
            events,
        }
    }
}

pub fn sanitize_macro_name(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    let trimmed = sanitized.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "macro".to_string()
    } else {
        trimmed
    }
}

pub fn macro_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", sanitize_macro_name(name)))
}

pub fn save_macro(dir: &Path, macro_file: &MacroFile) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|err| format!("Failed to create macro directory: {err}"))?;
    let path = macro_path(dir, &macro_file.name);
    let json = serde_json::to_string_pretty(macro_file)
        .map_err(|err| format!("Failed to serialize macro '{}': {err}", macro_file.name))?;
    fs::write(&path, json)
        .map_err(|err| format!("Failed to save macro '{}': {err}", path.display()))
}

pub fn update_macro(dir: &Path, macro_file: MacroFile) -> Result<MacroFile, String> {
    let macro_file = prepare_macro_for_save(macro_file)?;
    save_macro(dir, &macro_file)?;
    Ok(macro_file)
}

pub fn prepare_macro_for_save(mut macro_file: MacroFile) -> Result<MacroFile, String> {
    macro_file.name = sanitize_macro_name(&macro_file.name);

    let requested_duration_ms = finite_non_negative(
        macro_file.duration_ms,
        "Macro duration must be a finite non-negative number",
    )?;
    macro_file.events =
        materialize_timeline_events(macro_file.events, requested_duration_ms, true)?;
    macro_file.duration_ms = macro_file
        .events
        .last()
        .map(MacroEvent::elapsed_ms)
        .unwrap_or_default()
        .max(requested_duration_ms);
    Ok(macro_file)
}

fn materialize_timeline_events(
    events: Vec<MacroEvent>,
    requested_duration_ms: f64,
    strict: bool,
) -> Result<Vec<MacroEvent>, String> {
    let mut output = Vec::with_capacity(events.len() + 2);
    let mut cursor_ms = 0.0;

    for mut event in events {
        match event {
            MacroEvent::Delay {
                duration_ms,
                elapsed_ms: _,
            } => {
                let duration_ms = checked_non_negative(
                    duration_ms,
                    "Delay duration must be a finite non-negative number",
                    strict,
                )?;
                if duration_ms > 0.0 {
                    push_delay_duration(&mut output, &mut cursor_ms, duration_ms);
                }
            }
            _ => {
                let elapsed_ms = checked_non_negative(
                    event.elapsed_ms(),
                    "Event timestamp must be a finite non-negative number",
                    strict,
                )?
                .max(cursor_ms);
                push_delay_until(&mut output, &mut cursor_ms, elapsed_ms);
                event.set_elapsed_ms(elapsed_ms);
                output.push(event);
                cursor_ms = elapsed_ms;
            }
        }
    }

    if requested_duration_ms > cursor_ms {
        push_delay_until(&mut output, &mut cursor_ms, requested_duration_ms);
    }

    Ok(output)
}

fn push_delay_until(events: &mut Vec<MacroEvent>, cursor_ms: &mut f64, next_ms: f64) {
    let duration_ms = next_ms - *cursor_ms;
    if duration_ms <= 0.0 {
        return;
    }

    push_delay_duration(events, cursor_ms, duration_ms);
}

fn push_delay_duration(events: &mut Vec<MacroEvent>, cursor_ms: &mut f64, duration_ms: f64) {
    if duration_ms <= 0.0 {
        return;
    }

    *cursor_ms += duration_ms;
    if let Some(MacroEvent::Delay {
        duration_ms: last_duration_ms,
        elapsed_ms,
    }) = events.last_mut()
    {
        *last_duration_ms += duration_ms;
        *elapsed_ms = *cursor_ms;
        return;
    }

    events.push(MacroEvent::Delay {
        duration_ms,
        elapsed_ms: *cursor_ms,
    });
}

pub fn load_macro(dir: &Path, name: &str) -> Result<MacroFile, String> {
    let path = macro_path(dir, name);
    let bytes = fs::read(&path)
        .map_err(|err| format!("Failed to read macro '{}': {err}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("Failed to parse macro '{}': {err}", path.display()))
}

pub fn load_macros(dir: &Path) -> Result<Vec<MacroFile>, String> {
    fs::create_dir_all(dir).map_err(|err| format!("Failed to create macro directory: {err}"))?;

    let mut macros = Vec::new();
    for entry in fs::read_dir(dir).map_err(|err| format!("Failed to list macros: {err}"))? {
        let entry = entry.map_err(|err| format!("Failed to read macro directory entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let bytes = fs::read(&path)
            .map_err(|err| format!("Failed to read macro '{}': {err}", path.display()))?;
        let macro_file = serde_json::from_slice::<MacroFile>(&bytes)
            .map_err(|err| format!("Failed to parse macro '{}': {err}", path.display()))?;
        macros.push(macro_file);
    }

    macros.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(macros)
}

pub fn delete_macro(dir: &Path, name: &str) -> Result<(), String> {
    let path = macro_path(dir, name);
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|err| format!("Failed to delete macro '{}': {err}", path.display()))?;
    }
    Ok(())
}

pub fn rename_macro(dir: &Path, old_name: &str, new_name: &str) -> Result<MacroFile, String> {
    let mut macro_file = load_macro(dir, old_name)?;
    delete_macro(dir, old_name)?;
    macro_file.name = sanitize_macro_name(new_name);
    save_macro(dir, &macro_file)?;
    Ok(macro_file)
}

fn finite_non_negative(value: f64, message: &str) -> Result<f64, String> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(message.to_string())
    }
}

fn checked_non_negative(value: f64, message: &str, strict: bool) -> Result<f64, String> {
    if strict {
        finite_non_negative(value, message)
    } else {
        Ok(finite_non_negative_or_zero(value))
    }
}

fn finite_non_negative_or_zero(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}
