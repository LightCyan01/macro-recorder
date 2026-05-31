export type MouseButton = "Left" | "Right" | "Middle";

export type MacroEvent =
  | { type: "Delay"; duration_ms: number; elapsed_ms: number }
  | { type: "MouseMove"; x: number; y: number; elapsed_ms: number }
  | { type: "MouseDown"; button: MouseButton; x: number; y: number; elapsed_ms: number }
  | { type: "MouseUp"; button: MouseButton; x: number; y: number; elapsed_ms: number }
  | { type: "MouseScroll"; delta: number; elapsed_ms: number }
  | { type: "KeyDown"; vk_code: number; elapsed_ms: number }
  | { type: "KeyUp"; vk_code: number; elapsed_ms: number };

export interface MacroFile {
  name: string;
  created_at: string;
  duration_ms: number;
  events: MacroEvent[];
}

export interface HotkeyModifiers {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
}

export interface HotkeyBinding {
  key: string;
  vk_code: number;
  modifiers: HotkeyModifiers;
}

export interface HotkeyConfig {
  record_toggle: HotkeyBinding;
  play_toggle: HotkeyBinding;
  emergency_stop: HotkeyBinding;
}

export interface PlaybackOptions {
  loop_count: number;
  speed_multiplier: number;
  infinite_loop: boolean;
}

export interface RecordingCountEvent {
  count: number;
}

export interface PlaybackProgressEvent {
  name: string;
  fired: number;
  total: number;
  loop_index: number;
}

export const DEFAULT_HOTKEYS: HotkeyConfig = {
  record_toggle: {
    key: "F9",
    vk_code: 0x78,
    modifiers: { ctrl: false, alt: false, shift: false }
  },
  play_toggle: {
    key: "F10",
    vk_code: 0x79,
    modifiers: { ctrl: false, alt: false, shift: false }
  },
  emergency_stop: {
    key: "F11",
    vk_code: 0x7a,
    modifiers: { ctrl: false, alt: false, shift: false }
  }
};
