import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { DelayEditorDialog, MacroEditor } from "./MacroEditor";
import type { DelayCommand } from "../timelineCommands";
import type { MacroFile } from "../types";

const delayCommand: DelayCommand = {
  kind: "Delay",
  id: "delay-event-1",
  index: 1,
  start_ms: 25,
  elapsed_ms: 3_723_029,
  duration_ms: 3_723_004,
  source: { type: "event", eventIndex: 1 }
};

describe("MacroEditor delay dialog", () => {
  test("renders a popup editor with synchronized duration fields", () => {
    const html = renderToStaticMarkup(
      <DelayEditorDialog
        delay={delayCommand}
        saving={false}
        onDone={async () => undefined}
        onCancel={() => undefined}
      />
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("Delay Command");
    expect(html).toContain(
      "The Delay command suspends the execution of the current macro, expressed in milliseconds."
    );
    expect(html).toContain("3,723,004 ms");
    expect(html).toContain("Hours");
    expect(html).toContain("Minutes");
    expect(html).toContain("Seconds");
    expect(html).toContain("Milliseconds");
    expect(html).toContain('value="1"');
    expect(html).toContain('value="2"');
    expect(html).toContain('value="3"');
    expect(html).toContain('value="4"');
  });
});

describe("MacroEditor command terminal", () => {
  test("renders keyboard commands inside the terminal without a separate keyboard panel", () => {
    const macro: MacroFile = {
      name: "Keyboard macro",
      created_at: "2026-05-15T12:00:00.000Z",
      duration_ms: 25,
      events: [
        { type: "KeyDown", vk_code: 0x41, elapsed_ms: 0 },
        { type: "KeyUp", vk_code: 0x41, elapsed_ms: 25 }
      ]
    };

    const html = renderToStaticMarkup(
      <MacroEditor macro={macro} onSave={async (next) => next} />
    );

    expect(html).toContain("Command Terminal");
    expect(html).toContain("Keyboard A down");
    expect(html).toContain("Keyboard A up");
    expect(html).not.toContain("Recorded keyboard events");
    expect(html).not.toContain("keyboard-strip");
  });
});
