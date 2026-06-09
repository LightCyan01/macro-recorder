import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { HotkeyConfig, MacroFile, PlaybackOptions } from "./types";

export function listMacros(): Promise<MacroFile[]> {
  return invoke("list_macros");
}

export function startRecording(): Promise<void> {
  return invoke("start_recording");
}

export function stopRecording(name: string): Promise<MacroFile> {
  return invoke("stop_recording", { name });
}

export function playMacro(name: string): Promise<void> {
  return invoke("play_macro", { name });
}

export function stopPlayback(): Promise<void> {
  return invoke("stop_playback");
}

export function deleteMacro(name: string): Promise<MacroFile[]> {
  return invoke("delete_macro", { name });
}

export function renameMacro(oldName: string, newName: string): Promise<MacroFile> {
  return invoke("rename_macro", { oldName, newName });
}

export function updateMacro(macroFile: MacroFile): Promise<MacroFile> {
  return invoke("update_macro", { macroFile });
}

export function getHotkeys(): Promise<HotkeyConfig> {
  return invoke("get_hotkeys");
}

export function setHotkeys(config: HotkeyConfig): Promise<void> {
  return invoke("set_hotkeys", { config });
}

export function getPlaybackOptions(): Promise<PlaybackOptions> {
  return invoke("get_playback_options");
}

export function setPlaybackOptions(options: PlaybackOptions): Promise<PlaybackOptions> {
  return invoke("set_playback_options", { options });
}

export function getSaveDirectory(): Promise<string> {
  return invoke("get_save_directory");
}

export function setSaveDirectory(path: string): Promise<string> {
  return invoke("set_save_directory", { path });
}

export async function pickDirectory(defaultPath?: string): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath
  });
  return selected as string | null;
}
