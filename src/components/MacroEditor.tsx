import { Clock3, RotateCcw, Save, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { MacroFile } from "../types";
import { labelForEvent } from "../eventLabels";
import {
  applyDelayDurationEdit,
  buildTimelineCommands,
  deleteTimelineCommand,
  eventElapsedMs,
  formatDelay,
  decomposeDuration,
  updateDurationPart,
  type DelayCommand,
  type DurationField,
  type TimelineCommand
} from "../timelineCommands";

interface MacroEditorProps {
  macro: MacroFile | null;
  onSave: (macro: MacroFile) => Promise<MacroFile>;
}

export function MacroEditor({ macro, onSave }: MacroEditorProps) {
  const [draft, setDraft] = useState<MacroFile | null>(macro);
  const [selectedDelayId, setSelectedDelayId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [editorError, setEditorError] = useState<string | null>(null);
  const activeMacro = draft ?? macro;
  const commands = useMemo(
    () => (activeMacro ? buildTimelineCommands(activeMacro) : []),
    [activeMacro]
  );
  const selectedDelay = selectedDelayId
    ? commands.find(
        (command): command is DelayCommand =>
          command.kind === "Delay" && command.id === selectedDelayId
      ) ?? null
    : null;
  const dirty = activeMacro && macro ? !sameMacro(activeMacro, macro) : false;

  useEffect(() => {
    setDraft(macro ? cloneMacro(macro) : null);
    setSelectedDelayId(null);
    setEditorError(null);
  }, [macro?.name]);

  useEffect(() => {
    if (!selectedDelayId) {
      return;
    }

    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setSelectedDelayId(null);
      }
    }

    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [selectedDelayId]);

  if (!macro || !activeMacro) {
    return (
      <aside className="timeline-panel">
        <h2>Timeline</h2>
        <p>Select a macro.</p>
      </aside>
    );
  }

  return (
    <aside className="timeline-panel">
      <div className="timeline-head">
        <div>
          <h2>{activeMacro.name}</h2>
          <p>
            {commands.length.toLocaleString()} commands, {formatMs(activeMacro.duration_ms)}
          </p>
        </div>
        <div className="editor-actions">
          <button
            type="button"
            title="Revert timeline edits"
            disabled={!dirty || saving}
            onClick={() => {
              setDraft(cloneMacro(macro));
              setSelectedDelayId(null);
              setEditorError(null);
            }}
          >
            <RotateCcw size={15} />
          </button>
          <button
            type="button"
            title="Save timeline edits"
            disabled={!dirty || saving}
            onClick={() => void saveDraft()}
          >
            <Save size={15} />
          </button>
        </div>
      </div>

      {editorError ? <p className="inline-error timeline-error">{editorError}</p> : null}

      <section className="command-terminal" aria-label="Macro command terminal">
        <div className="command-terminal-head">
          <div>
            <span>Command Terminal</span>
            <strong>{commands.length.toLocaleString()} commands</strong>
          </div>
          <span>{commands.length > 320 ? "Showing first 320" : "Event stream"}</span>
        </div>

        <ol className="event-list command-timeline">
          {commands.slice(0, 320).map((command) => (
            <li
              key={command.id}
              onClick={command.kind === "Delay" ? () => setSelectedDelayId(command.id) : undefined}
              className={[
                "timeline-command",
                command.kind === "Delay" ? "delay-command" : "action-command",
                selectedDelayId === command.id ? "selected-command" : ""
              ]
                .filter(Boolean)
                .join(" ")}
            >
              <span className="command-index">{(command.index + 1).toString().padStart(3, "0")}</span>
              <span className="command-time">{formatTimestamp(command.elapsed_ms)}</span>
              {command.kind === "Delay" ? (
                <button
                  type="button"
                  className="command-body"
                  title="Edit delay command"
                  onClick={(event) => {
                    event.stopPropagation();
                    setSelectedDelayId(command.id);
                  }}
                >
                  <span className="command-primary">
                    <Clock3 size={14} />
                    <strong>Delay</strong>
                  </span>
                  <span className="command-detail">
                    {formatDelay(command.duration_ms)}
                    <em>{command.duration_ms.toLocaleString()} ms</em>
                  </span>
                </button>
              ) : (
                <div className="command-body">
                  <span className="command-primary">
                    <strong>{labelForEvent(command.event)}</strong>
                  </span>
                  <span className="command-detail">{commandDetail(command.eventIndex)}</span>
                </div>
              )}
              <button
                type="button"
                className="command-delete"
                title="Delete command"
                disabled={saving}
                onClick={(event) => {
                  event.stopPropagation();
                  void deleteCommand(command);
                }}
              >
                <Trash2 size={14} />
              </button>
            </li>
          ))}
        </ol>
      </section>

      {selectedDelay ? (
        <DelayEditorDialog
          delay={selectedDelay}
          saving={saving}
          onDone={commitSelectedDelay}
          onCancel={() => setSelectedDelayId(null)}
        />
      ) : null}
    </aside>
  );

  async function commitSelectedDelay(durationMs: number) {
    if (!selectedDelay || !activeMacro) {
      return;
    }

    const result = applyDelayDurationEdit(activeMacro, selectedDelay, durationMs);
    setDraft(result.macro);
    const saved = await saveMacro(result.macro);
    if (saved) {
      setSelectedDelayId(null);
    }
  }

  async function deleteCommand(command: TimelineCommand) {
    if (!activeMacro || saving) {
      return;
    }

    const result = deleteTimelineCommand(activeMacro, command);
    setSelectedDelayId(null);
    setDraft(result.macro);
    await saveMacro(result.macro);
  }

  async function saveDraft() {
    if (!draft || !dirty) {
      return;
    }
    await saveMacro(draft);
  }

  async function saveMacro(next: MacroFile) {
    setSaving(true);
    setEditorError(null);
    try {
      const saved = await onSave(next);
      setDraft(cloneMacro(saved));
      return saved;
    } catch (caught) {
      setEditorError(String(caught));
      return null;
    } finally {
      setSaving(false);
    }
  }
}

interface DelayEditorDialogProps {
  delay: DelayCommand;
  saving: boolean;
  onDone: (durationMs: number) => Promise<void>;
  onCancel: () => void;
}

export function DelayEditorDialog({
  delay,
  saving,
  onDone,
  onCancel
}: DelayEditorDialogProps) {
  const [durationMs, setDurationMs] = useState(delay.duration_ms);
  const parts = useMemo(() => decomposeDuration(durationMs), [durationMs]);

  useEffect(() => {
    setDurationMs(delay.duration_ms);
  }, [delay.id, delay.duration_ms]);

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onCancel();
        }
      }}
    >
      <section
        className="delay-editor delay-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="delay-dialog-title"
        aria-describedby="delay-dialog-description"
      >
        <div className="delay-editor-head">
          <div>
            <h3 id="delay-dialog-title">Delay Command</h3>
            <p id="delay-dialog-description">
              The Delay command suspends the execution of the current macro,
              expressed in milliseconds.
            </p>
          </div>
          <button
            type="button"
            className="icon-button"
            title="Close delay editor"
            disabled={saving}
            onClick={onCancel}
          >
            <X size={16} />
          </button>
        </div>

        <div className="delay-total">
          <span>Duration</span>
          <strong>{durationMs.toLocaleString()} ms</strong>
        </div>

        <div className="delay-grid">
          <DelayField
            label="Hours"
            field="hours"
            value={parts.hours}
            onChange={editDurationPart}
          />
          <DelayField
            label="Minutes"
            field="minutes"
            value={parts.minutes}
            onChange={editDurationPart}
          />
          <DelayField
            label="Seconds"
            field="seconds"
            value={parts.seconds}
            onChange={editDurationPart}
          />
          <DelayField
            label="Milliseconds"
            field="milliseconds"
            value={parts.milliseconds}
            onChange={editDurationPart}
          />
        </div>

        <div className="delay-dialog-actions">
          <button
            type="button"
            disabled={saving}
            onClick={() => void onDone(durationMs)}
          >
            {saving ? "Saving..." : "Done"}
          </button>
        </div>
      </section>
    </div>
  );

  function editDurationPart(field: DurationField, value: string) {
    setDurationMs((current) => updateDurationPart(current, field, value));
  }
}

function formatMs(ms: number) {
  if (ms < 1000) {
    return `${ms.toFixed(1)} ms`;
  }
  return `${(ms / 1000).toFixed(2)} s`;
}

function formatTimestamp(ms: number) {
  const totalMs = Math.max(0, Math.round(ms));
  const hours = Math.floor(totalMs / 3_600_000);
  const minutes = Math.floor((totalMs % 3_600_000) / 60_000);
  const seconds = Math.floor((totalMs % 60_000) / 1000);
  const milliseconds = totalMs % 1000;

  const minuteText = hours > 0 ? minutes.toString().padStart(2, "0") : minutes.toString();
  const secondText = seconds.toString().padStart(2, "0");
  const millisecondText = milliseconds.toString().padStart(3, "0");

  if (hours > 0) {
    return `${hours}:${minuteText}:${secondText}.${millisecondText}`;
  }
  return `${minuteText}:${secondText}.${millisecondText}`;
}

function commandDetail(eventIndex: number) {
  return `event ${eventIndex + 1}`;
}

interface DelayFieldProps {
  label: string;
  field: DurationField;
  value: number;
  onChange: (field: DurationField, value: string) => void;
}

function DelayField({ label, field, value, onChange }: DelayFieldProps) {
  return (
    <label>
      <span>{label}</span>
      <input
        type="number"
        min={0}
        step={1}
        value={value}
        onChange={(event) => onChange(field, event.currentTarget.value)}
      />
    </label>
  );
}

function cloneMacro(macro: MacroFile) {
  return {
    ...macro,
    events: macro.events.map((event) => ({ ...event }))
  };
}

function sameMacro(left: MacroFile, right: MacroFile) {
  if (left.name !== right.name || left.duration_ms !== right.duration_ms) {
    return false;
  }

  if (left.events.length !== right.events.length) {
    return false;
  }

  return left.events.every((event, index) => {
    const other = right.events[index];
    return event.type === other.type && eventElapsedMs(event) === eventElapsedMs(other) && shallowEqual(event, other);
  });
}

function shallowEqual(left: Record<string, unknown>, right: Record<string, unknown>) {
  const leftEntries = Object.entries(left);
  if (leftEntries.length !== Object.keys(right).length) {
    return false;
  }

  return leftEntries.every(([key, value]) => right[key] === value);
}
