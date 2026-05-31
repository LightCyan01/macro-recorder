import { describe, expect, test } from "bun:test";
import {
  MAX_DELAY_MS,
  applyDelayDurationEdit,
  buildTimelineCommands,
  deleteTimelineCommand,
  decomposeDuration,
  updateDurationPart,
  type DelayCommand
} from "./timelineCommands";
import type { MacroFile } from "./types";

const baseMacro: Omit<MacroFile, "events" | "duration_ms"> = {
  name: "test",
  created_at: "2026-05-15T12:00:00.000Z"
};

describe("timeline commands", () => {
  test("renders timestamp gaps as Jitbit-style delay commands", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 35,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 25 },
        { type: "MouseScroll", delta: -120, elapsed_ms: 35 }
      ]
    };

    const commands = buildTimelineCommands(macro);

    expect(commands.map((command) => command.kind)).toEqual([
      "Action",
      "Delay",
      "Action",
      "Delay",
      "Action"
    ]);
    expect(commands[1]).toMatchObject({
      kind: "Delay",
      start_ms: 0,
      duration_ms: 25,
      elapsed_ms: 25
    });
    expect(commands[3]).toMatchObject({
      kind: "Delay",
      start_ms: 25,
      duration_ms: 10,
      elapsed_ms: 35
    });
  });

  test("editing an implicit delay inserts a delay command and shifts later events", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 25,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 25 }
      ]
    };
    const delay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand => command.kind === "Delay"
    );

    expect(delay).toBeDefined();
    const result = applyDelayDurationEdit(macro, delay, 1500);

    expect(result.selectedDelayId).toBe("delay-event-1");
    expect(result.macro.duration_ms).toBe(1500);
    expect(result.macro.events).toEqual([
      { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
      { type: "Delay", duration_ms: 1500, elapsed_ms: 1500 },
      { type: "KeyUp", vk_code: 0x41, elapsed_ms: 1500 }
    ]);
  });

  test("repeated delay edits update one command instead of adding duplicate delays", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 25,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 25 }
      ]
    };
    const initialDelay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand => command.kind === "Delay"
    );
    const first = applyDelayDurationEdit(macro, initialDelay, 100);
    const selected = buildTimelineCommands(first.macro).find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.id === first.selectedDelayId
    );
    const second = applyDelayDurationEdit(first.macro, selected, 200);

    const delayEvents = second.macro.events.filter((event) => event.type === "Delay");
    expect(delayEvents).toEqual([{ type: "Delay", duration_ms: 200, elapsed_ms: 200 }]);
    expect(second.macro.events).toEqual([
      { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
      { type: "Delay", duration_ms: 200, elapsed_ms: 200 },
      { type: "KeyUp", vk_code: 0x41, elapsed_ms: 200 }
    ]);
  });

  test("editing an empty timeline creates a playable delay-only macro", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 0,
      events: []
    };
    const delay = buildTimelineCommands(macro)[0];

    expect(delay).toBeUndefined();
    const placeholder: DelayCommand = {
      kind: "Delay",
      id: "delay-empty",
      index: 0,
      start_ms: 0,
      elapsed_ms: 0,
      duration_ms: 0,
      source: { type: "empty" }
    };
    const result = applyDelayDurationEdit(macro, placeholder, 500);

    expect(result.macro).toMatchObject({
      duration_ms: 500,
      events: [{ type: "Delay", duration_ms: 500, elapsed_ms: 500 }]
    });
  });

  test("editing an earlier delay preserves the trailing delay command duration", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 100,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 25 }
      ]
    };
    const delay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.source.type === "gap"
    );

    const result = applyDelayDurationEdit(macro, delay, 30);
    const commands = buildTimelineCommands(result.macro);
    const trailing = commands.find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.source.type === "trailing"
    );

    expect(result.macro.duration_ms).toBe(105);
    expect(trailing?.duration_ms).toBe(75);
  });

  test("editing an explicit delay shifts only following commands", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 120,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "Delay", duration_ms: 100, elapsed_ms: 100 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 100 },
        { type: "KeyDown", vk_code: 0x42, elapsed_ms: 120 }
      ]
    };
    const delay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.source.type === "event"
    );

    const result = applyDelayDurationEdit(macro, delay, 250);

    expect(result.macro.duration_ms).toBe(270);
    expect(result.macro.events).toEqual([
      { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
      { type: "Delay", duration_ms: 250, elapsed_ms: 250 },
      { type: "KeyUp", vk_code: 0x41, elapsed_ms: 250 },
      { type: "Delay", duration_ms: 20, elapsed_ms: 270 },
      { type: "KeyDown", vk_code: 0x42, elapsed_ms: 270 }
    ]);
  });

  test("shrinking an implicit leading delay never creates negative timestamps", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 40,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 20 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 40 }
      ]
    };
    const delay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.source.type === "gap"
    );

    const result = applyDelayDurationEdit(macro, delay, 0);

    expect(result.macro.duration_ms).toBe(20);
    expect(result.macro.events).toEqual([
      { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
      { type: "Delay", duration_ms: 20, elapsed_ms: 20 },
      { type: "KeyUp", vk_code: 0x41, elapsed_ms: 20 }
    ]);
  });

  test("editing a trailing delay creates a playable delay event at the end", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 100,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 20 }
      ]
    };
    const delay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.source.type === "trailing"
    );

    const result = applyDelayDurationEdit(macro, delay, 150);

    expect(result.selectedDelayId).toBe("delay-event-2");
    expect(result.macro.duration_ms).toBe(170);
    expect(result.macro.events).toEqual([
      { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
      { type: "Delay", duration_ms: 20, elapsed_ms: 20 },
      { type: "KeyUp", vk_code: 0x41, elapsed_ms: 20 },
      { type: "Delay", duration_ms: 150, elapsed_ms: 170 }
    ]);
  });

  test("deleting an explicit delay collapses later command timestamps", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 125,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "Delay", duration_ms: 100, elapsed_ms: 100 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 100 },
        { type: "KeyDown", vk_code: 0x42, elapsed_ms: 125 }
      ]
    };
    const delay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.source.type === "event"
    );

    const result = deleteTimelineCommand(macro, delay);

    expect(result.macro.duration_ms).toBe(25);
    expect(result.macro.events).toEqual([
      { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
      { type: "KeyUp", vk_code: 0x41, elapsed_ms: 0 },
      { type: "Delay", duration_ms: 25, elapsed_ms: 25 },
      { type: "KeyDown", vk_code: 0x42, elapsed_ms: 25 }
    ]);
  });

  test("deleting an action removes only that command and keeps the remaining timeline valid", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 100,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "Delay", duration_ms: 50, elapsed_ms: 50 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 50 },
        { type: "Delay", duration_ms: 50, elapsed_ms: 100 }
      ]
    };
    const action = buildTimelineCommands(macro).find(
      (command) => command.kind === "Action" && command.event.type === "KeyUp"
    );

    const result = deleteTimelineCommand(macro, action);

    expect(result.macro.duration_ms).toBe(100);
    expect(result.macro.events).toEqual([
      { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
      { type: "Delay", duration_ms: 100, elapsed_ms: 100 }
    ]);
  });

  test("zero millisecond delays are hidden and removed during normalization", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 1000,
      events: [
        { type: "Delay", duration_ms: 1000, elapsed_ms: 1000 },
        { type: "Delay", duration_ms: 0, elapsed_ms: 1000 },
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 1000 }
      ]
    };

    const commands = buildTimelineCommands(macro);
    expect(commands.map((command) => [command.kind, command.kind === "Delay" ? command.duration_ms : command.event.type])).toEqual([
      ["Delay", 1000],
      ["Action", "KeyDown"]
    ]);

    const action = commands.find((command) => command.kind === "Action");
    expect(action).toBeDefined();
    const result = deleteTimelineCommand(macro, action!);
    expect(result.macro.events).toEqual([{ type: "Delay", duration_ms: 1000, elapsed_ms: 1000 }]);
  });

  test("deleting the only long delay leaves no undeletable zero millisecond placeholder", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 120_000,
      events: [{ type: "Delay", duration_ms: 120_000, elapsed_ms: 120_000 }]
    };
    const delay = buildTimelineCommands(macro)[0];

    const result = deleteTimelineCommand(macro, delay);

    expect(result.macro).toMatchObject({ duration_ms: 0, events: [] });
    expect(buildTimelineCommands(result.macro)).toEqual([]);
  });

  test("deleting an action between delays merges the neighboring delay commands", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 10_002,
      events: [
        { type: "Delay", duration_ms: 10_000, elapsed_ms: 10_000 },
        { type: "MouseMove", x: 10, y: 20, elapsed_ms: 10_000 },
        { type: "Delay", duration_ms: 2, elapsed_ms: 10_002 },
        { type: "MouseMove", x: 11, y: 20, elapsed_ms: 10_002 }
      ]
    };
    const action = buildTimelineCommands(macro).find(
      (command) => command.kind === "Action" && command.eventIndex === 1
    );
    expect(action).toBeDefined();

    const result = deleteTimelineCommand(macro, action!);

    expect(result.macro.events).toEqual([
      { type: "Delay", duration_ms: 10_002, elapsed_ms: 10_002 },
      { type: "MouseMove", x: 11, y: 20, elapsed_ms: 10_002 }
    ]);
    expect(buildTimelineCommands(result.macro).map((command) => command.kind === "Delay" ? command.duration_ms : command.event.type)).toEqual([
      10_002,
      "MouseMove"
    ]);
  });

  test("small delay commands revealed after edits remain deletable", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 10_002,
      events: [
        { type: "Delay", duration_ms: 10_000, elapsed_ms: 10_000 },
        { type: "MouseMove", x: 10, y: 20, elapsed_ms: 10_000 },
        { type: "Delay", duration_ms: 2, elapsed_ms: 10_002 },
        { type: "MouseMove", x: 11, y: 20, elapsed_ms: 10_002 }
      ]
    };
    const tinyDelay = buildTimelineCommands(macro).find(
      (command): command is DelayCommand =>
        command.kind === "Delay" && command.duration_ms === 2
    );
    expect(tinyDelay).toBeDefined();

    const result = deleteTimelineCommand(macro, tinyDelay!);

    expect(result.macro.duration_ms).toBe(10_000);
    expect(result.macro.events).toEqual([
      { type: "Delay", duration_ms: 10_000, elapsed_ms: 10_000 },
      { type: "MouseMove", x: 10, y: 20, elapsed_ms: 10_000 },
      { type: "MouseMove", x: 11, y: 20, elapsed_ms: 10_000 }
    ]);
    expect(buildTimelineCommands(result.macro).some(
      (command) => command.kind === "Delay" && command.duration_ms === 2
    )).toBe(false);
  });

  test("duration fields normalize across hours minutes seconds and milliseconds", () => {
    const total = 1 * 60 * 60 * 1000 + 2 * 60 * 1000 + 3 * 1000 + 4;
    const edited = updateDurationPart(total, "milliseconds", "1500");

    expect(decomposeDuration(edited)).toEqual({
      hours: 1,
      minutes: 2,
      seconds: 4,
      milliseconds: 500
    });

    expect(decomposeDuration(updateDurationPart(0, "seconds", "120"))).toEqual({
      hours: 0,
      minutes: 2,
      seconds: 0,
      milliseconds: 0
    });

    expect(decomposeDuration(updateDurationPart(0, "minutes", "75"))).toEqual({
      hours: 1,
      minutes: 15,
      seconds: 0,
      milliseconds: 0
    });
  });

  test("delay edits clamp oversized values to the maximum supported delay", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: 0,
      events: []
    };
    const placeholder: DelayCommand = {
      kind: "Delay",
      id: "delay-empty",
      index: 0,
      start_ms: 0,
      elapsed_ms: 0,
      duration_ms: 0,
      source: { type: "empty" }
    };
    const result = applyDelayDurationEdit(macro, placeholder, MAX_DELAY_MS + 50_000);

    expect(result.macro.duration_ms).toBe(MAX_DELAY_MS);
    expect(result.macro.events).toEqual([
      { type: "Delay", duration_ms: MAX_DELAY_MS, elapsed_ms: MAX_DELAY_MS }
    ]);
    expect(updateDurationPart(0, "hours", "1000")).toBe(MAX_DELAY_MS);
  });

  test("negative and non-finite values are clamped to a safe zero delay", () => {
    const macro: MacroFile = {
      ...baseMacro,
      duration_ms: Number.NaN,
      events: [{ type: "KeyDown", vk_code: 0x41, elapsed_ms: -100 }]
    };

    expect(buildTimelineCommands(macro)).toHaveLength(1);
    expect(updateDurationPart(Number.POSITIVE_INFINITY, "seconds", "-4")).toBe(0);
    expect(updateDurationPart(1000, "milliseconds", "not-a-number")).toBe(1000);
  });
});
