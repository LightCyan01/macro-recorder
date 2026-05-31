import { describe, expect, test } from "bun:test";
import { keyboardEvents, keyNameFromVk, labelForEvent } from "./eventLabels";
import type { MacroEvent } from "./types";

describe("event labels", () => {
  test("shows readable keyboard presses in the event log", () => {
    expect(labelForEvent({ type: "Delay", duration_ms: 250, elapsed_ms: 250 })).toBe(
      "DELAY 250 ms"
    );
    expect(labelForEvent({ type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 })).toBe(
      "Keyboard A down"
    );
    expect(labelForEvent({ type: "KeyUp", vk_code: 0x10, elapsed_ms: 1 })).toBe(
      "Keyboard Shift up"
    );
  });

  test("extracts keyboard events even when mouse movement dominates the macro", () => {
    const events: MacroEvent[] = [
      ...Array.from({ length: 300 }, (_, index) => ({
        type: "MouseMove" as const,
        x: index,
        y: index,
        elapsed_ms: index
      })),
      { type: "KeyDown", vk_code: 0x42, elapsed_ms: 301 },
      { type: "KeyUp", vk_code: 0x42, elapsed_ms: 302 }
    ];

    expect(keyboardEvents(events).map(labelForEvent)).toEqual([
      "Keyboard B down",
      "Keyboard B up"
    ]);
  });

  test("uses stable fallback labels for unmapped virtual keys", () => {
    expect(keyNameFromVk(255)).toBe("VK 255");
  });
});
