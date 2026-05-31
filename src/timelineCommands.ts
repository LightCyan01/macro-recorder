import type { MacroEvent, MacroFile } from "./types";

const SECOND_MS = 1000;
const MINUTE_MS = 60 * SECOND_MS;
const HOUR_MS = 60 * MINUTE_MS;
export const MAX_DELAY_MS = 999 * HOUR_MS + 59 * MINUTE_MS + 59 * SECOND_MS + 999;

export type DurationField = "hours" | "minutes" | "seconds" | "milliseconds";

export interface DurationParts {
  hours: number;
  minutes: number;
  seconds: number;
  milliseconds: number;
}

export interface DelayCommand {
  kind: "Delay";
  id: string;
  index: number;
  start_ms: number;
  elapsed_ms: number;
  duration_ms: number;
  source:
    | { type: "event"; eventIndex: number }
    | { type: "gap"; beforeEventIndex: number }
    | { type: "trailing" }
    | { type: "empty" };
}

export interface ActionCommand {
  kind: "Action";
  id: string;
  index: number;
  elapsed_ms: number;
  eventIndex: number;
  event: Exclude<MacroEvent, { type: "Delay" }>;
}

export type TimelineCommand = DelayCommand | ActionCommand;

export interface DelayEditResult {
  macro: MacroFile;
  selectedDelayId: string;
}

export interface TimelineEditResult {
  macro: MacroFile;
}

export function buildTimelineCommands(macro: MacroFile): TimelineCommand[] {
  const commands: TimelineCommand[] = [];
  let cursorMs = 0;

  macro.events.forEach((event, eventIndex) => {
    if (isDelayEvent(event)) {
      const durationMs = sanitizeDelayMs(event.duration_ms);
      if (durationMs <= 0) {
        return;
      }
      const delay = makeDelayCommand(
        commands.length,
        cursorMs,
        durationMs,
        `delay-event-${eventIndex}`,
        { type: "event", eventIndex }
      );
      commands.push(delay);
      cursorMs = delay.elapsed_ms;
      return;
    }

    const eventElapsedMs = sanitizeElapsedMs(event.elapsed_ms);
    const gapMs = Math.max(0, eventElapsedMs - cursorMs);
    if (gapMs > 0) {
      const delay = makeDelayCommand(
        commands.length,
        cursorMs,
        gapMs,
        `delay-gap-${eventIndex}`,
        { type: "gap", beforeEventIndex: eventIndex }
      );
      commands.push(delay);
      cursorMs = delay.elapsed_ms;
    }

    commands.push({
      kind: "Action",
      id: `action-${eventIndex}`,
      index: commands.length,
      elapsed_ms: cursorMs,
      eventIndex,
      event
    });
    cursorMs = Math.max(cursorMs, eventElapsedMs);
  });

  const trailingMs = Math.max(0, sanitizeElapsedMs(macro.duration_ms) - cursorMs);
  if (trailingMs > 0) {
    commands.push(
      makeDelayCommand(
        commands.length,
        cursorMs,
        trailingMs,
        macro.events.length === 0 ? "delay-empty" : "delay-trailing",
        macro.events.length === 0 ? { type: "empty" } : { type: "trailing" }
      )
    );
  }

  return commands;
}

export function applyDelayDurationEdit(
  macro: MacroFile,
  command: DelayCommand,
  nextDurationMs: number
): DelayEditResult {
  const durationMs = sanitizeDelayMs(nextDurationMs);
  const events = macro.events.map(cloneEvent);

  if (command.source.type === "event") {
    const eventIndex = command.source.eventIndex;
    const oldEvent = events[eventIndex];
    if (oldEvent && isDelayEvent(oldEvent)) {
      const oldDurationMs = sanitizeDelayMs(oldEvent.duration_ms);
      const deltaMs = durationMs - oldDurationMs;
      events[eventIndex] = {
        ...oldEvent,
        duration_ms: durationMs,
        elapsed_ms: command.start_ms + durationMs
      };
      shiftElapsed(events, eventIndex + 1, deltaMs);
      return finalizeDelayEdit(
        macro,
        events,
        `delay-event-${eventIndex}`,
        sanitizeElapsedMs(macro.duration_ms + deltaMs)
      );
    }
  }

  if (command.source.type === "gap") {
    const insertAt = command.source.beforeEventIndex;
    const deltaMs = durationMs - command.duration_ms;
    shiftElapsed(events, insertAt, deltaMs);
    events.splice(insertAt, 0, {
      type: "Delay",
      duration_ms: durationMs,
      elapsed_ms: command.start_ms + durationMs
    });
    return finalizeDelayEdit(
      macro,
      events,
      `delay-event-${insertAt}`,
      sanitizeElapsedMs(macro.duration_ms + deltaMs)
    );
  }

  const delayEvent: MacroEvent = {
    type: "Delay",
    duration_ms: durationMs,
    elapsed_ms: command.start_ms + durationMs
  };
  events.push(delayEvent);
  return finalizeDelayEdit(
    macro,
    events,
    `delay-event-${events.length - 1}`,
    command.start_ms + durationMs
  );
}

export function deleteTimelineCommand(
  macro: MacroFile,
  command: TimelineCommand
): TimelineEditResult {
  const events = macro.events.map(cloneEvent);

  if (command.kind === "Action") {
    events.splice(command.eventIndex, 1);
    return finalizeTimelineEdit(macro, events, macro.duration_ms);
  }

  if (command.source.type === "event") {
    events.splice(command.source.eventIndex, 1);
    shiftElapsed(events, command.source.eventIndex, -command.duration_ms);
    return finalizeTimelineEdit(macro, events, macro.duration_ms - command.duration_ms);
  }

  if (command.source.type === "gap") {
    shiftElapsed(events, command.source.beforeEventIndex, -command.duration_ms);
    return finalizeTimelineEdit(macro, events, macro.duration_ms - command.duration_ms);
  }

  if (command.source.type === "trailing") {
    return finalizeTimelineEdit(macro, events, command.start_ms);
  }

  return finalizeTimelineEdit(macro, [], 0);
}

export function decomposeDuration(totalMs: number): DurationParts {
  let remainingMs = sanitizeDelayMs(totalMs);
  const hours = Math.floor(remainingMs / HOUR_MS);
  remainingMs -= hours * HOUR_MS;
  const minutes = Math.floor(remainingMs / MINUTE_MS);
  remainingMs -= minutes * MINUTE_MS;
  const seconds = Math.floor(remainingMs / SECOND_MS);
  remainingMs -= seconds * SECOND_MS;

  return {
    hours,
    minutes,
    seconds,
    milliseconds: remainingMs
  };
}

export function updateDurationPart(
  totalMs: number,
  field: DurationField,
  rawValue: string | number
) {
  const parts = decomposeDuration(totalMs);
  const value = parseWholeNumber(rawValue);
  const nextParts = {
    ...parts,
    [field]: value
  };

  return sanitizeDelayMs(
    nextParts.hours * HOUR_MS +
      nextParts.minutes * MINUTE_MS +
      nextParts.seconds * SECOND_MS +
      nextParts.milliseconds
  );
}

export function formatDelay(ms: number) {
  const parts = decomposeDuration(ms);
  if (parts.hours > 0) {
    return `${parts.hours}h ${parts.minutes}m ${parts.seconds}s ${parts.milliseconds}ms`;
  }
  if (parts.minutes > 0) {
    return `${parts.minutes}m ${parts.seconds}s ${parts.milliseconds}ms`;
  }
  if (parts.seconds > 0) {
    return `${parts.seconds}s ${parts.milliseconds}ms`;
  }
  return `${parts.milliseconds}ms`;
}

export function isDelayEvent(event: MacroEvent): event is Extract<MacroEvent, { type: "Delay" }> {
  return event.type === "Delay";
}

export function eventElapsedMs(event: MacroEvent) {
  return sanitizeElapsedMs(event.elapsed_ms);
}

function makeDelayCommand(
  index: number,
  startMs: number,
  durationMs: number,
  id: string,
  source: DelayCommand["source"]
): DelayCommand {
  const sanitizedDurationMs = sanitizeDelayMs(durationMs);
  return {
    kind: "Delay",
    id,
    index,
    start_ms: sanitizeElapsedMs(startMs),
    elapsed_ms: sanitizeElapsedMs(startMs + sanitizedDurationMs),
    duration_ms: sanitizedDurationMs,
    source
  };
}

function finalizeDelayEdit(
  macro: MacroFile,
  events: MacroEvent[],
  selectedDelayId: string,
  durationMs: number
): DelayEditResult {
  const normalizedEvents = normalizeEventTimeline(events);
  return {
    macro: {
      ...macro,
      events: normalizedEvents,
      duration_ms: Math.max(calculateDuration(normalizedEvents), sanitizeElapsedMs(durationMs))
    },
    selectedDelayId
  };
}

function finalizeTimelineEdit(
  macro: MacroFile,
  events: MacroEvent[],
  durationMs: number
): TimelineEditResult {
  const normalizedEvents = normalizeEventTimeline(events);
  return {
    macro: {
      ...macro,
      events: normalizedEvents,
      duration_ms: Math.max(calculateDuration(normalizedEvents), sanitizeElapsedMs(durationMs))
    }
  };
}

function normalizeEventTimeline(events: MacroEvent[]) {
  let cursorMs = 0;
  const normalizedEvents: MacroEvent[] = [];

  for (const event of events) {
    if (isDelayEvent(event)) {
      const durationMs = sanitizeDelayMs(event.duration_ms);
      appendDelay(normalizedEvents, durationMs, cursorMs);
      cursorMs += durationMs;
      continue;
    }

    const elapsedMs = Math.max(cursorMs, sanitizeElapsedMs(event.elapsed_ms));
    appendDelay(normalizedEvents, elapsedMs - cursorMs, cursorMs);
    cursorMs = elapsedMs;
    normalizedEvents.push({
      ...event,
      elapsed_ms: elapsedMs
    });
  }

  return normalizedEvents;
}

function calculateDuration(events: MacroEvent[]) {
  return events.reduce((maxElapsedMs, event) => Math.max(maxElapsedMs, eventElapsedMs(event)), 0);
}

function shiftElapsed(events: MacroEvent[], startIndex: number, deltaMs: number) {
  if (deltaMs === 0) {
    return;
  }

  for (let index = startIndex; index < events.length; index += 1) {
    events[index] = {
      ...events[index],
      elapsed_ms: sanitizeElapsedMs(events[index].elapsed_ms + deltaMs)
    };
  }
}

function appendDelay(events: MacroEvent[], durationMs: number, startMs: number) {
  const sanitizedDurationMs = sanitizeDelayMs(durationMs);
  if (sanitizedDurationMs <= 0) {
    return;
  }

  const elapsedMs = sanitizeElapsedMs(startMs + sanitizedDurationMs);
  const lastEvent = events[events.length - 1];
  if (lastEvent && isDelayEvent(lastEvent)) {
    lastEvent.duration_ms = sanitizeDelayMs(lastEvent.duration_ms + sanitizedDurationMs);
    lastEvent.elapsed_ms = elapsedMs;
    return;
  }

  events.push({
    type: "Delay",
    duration_ms: sanitizedDurationMs,
    elapsed_ms: elapsedMs
  });
}

function cloneEvent(event: MacroEvent): MacroEvent {
  return { ...event };
}

function sanitizeElapsedMs(value: number) {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, value);
}

function sanitizeDelayMs(value: number) {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.min(MAX_DELAY_MS, Math.max(0, Math.round(value)));
}

function parseWholeNumber(value: string | number) {
  if (typeof value === "number") {
    return Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0;
  }

  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? Math.max(0, parsed) : 0;
}
