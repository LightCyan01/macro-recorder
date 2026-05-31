import type { MacroEvent } from "./types";

const KEY_NAMES: Record<number, string> = {
  0x08: "Backspace",
  0x09: "Tab",
  0x0d: "Enter",
  0x10: "Shift",
  0x11: "Ctrl",
  0x12: "Alt",
  0x1b: "Esc",
  0x20: "Space",
  0x21: "Page Up",
  0x22: "Page Down",
  0x23: "End",
  0x24: "Home",
  0x25: "Left",
  0x26: "Up",
  0x27: "Right",
  0x28: "Down",
  0x2d: "Insert",
  0x2e: "Delete"
};

for (let code = 0x70; code <= 0x87; code += 1) {
  KEY_NAMES[code] = `F${code - 0x70 + 1}`;
}

export function keyNameFromVk(vkCode: number) {
  if (KEY_NAMES[vkCode]) {
    return KEY_NAMES[vkCode];
  }
  if (vkCode >= 0x30 && vkCode <= 0x39) {
    return String.fromCharCode(vkCode);
  }
  if (vkCode >= 0x41 && vkCode <= 0x5a) {
    return String.fromCharCode(vkCode);
  }
  return `VK ${vkCode}`;
}

export function labelForEvent(event: MacroEvent) {
  switch (event.type) {
    case "Delay":
      return `DELAY ${event.duration_ms.toLocaleString()} ms`;
    case "MouseMove":
      return `Move ${event.x}, ${event.y}`;
    case "MouseDown":
      return `${event.button} down ${event.x}, ${event.y}`;
    case "MouseUp":
      return `${event.button} up ${event.x}, ${event.y}`;
    case "MouseScroll":
      return `Scroll ${event.delta}`;
    case "KeyDown":
      return `Keyboard ${keyNameFromVk(event.vk_code)} down`;
    case "KeyUp":
      return `Keyboard ${keyNameFromVk(event.vk_code)} up`;
  }
}

export function keyboardEvents(events: MacroEvent[]) {
  return events.filter((event) => event.type === "KeyDown" || event.type === "KeyUp");
}
