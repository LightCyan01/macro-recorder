import { Pencil, Play, Trash2 } from "lucide-react";
import type { MacroFile } from "../types";

interface MacroListProps {
  macros: MacroFile[];
  selectedName: string | null;
  onSelect: (macro: MacroFile) => void;
  onPlay: (name: string) => void;
  onDelete: (name: string) => void;
  onRename: (macro: MacroFile) => void;
}

export function MacroList({
  macros,
  selectedName,
  onSelect,
  onPlay,
  onDelete,
  onRename
}: MacroListProps) {
  if (macros.length === 0) {
    return (
      <section className="empty-state">
        <h2>Macro Library</h2>
        <p>No saved macros yet. Hit Record to create your first one.</p>
      </section>
    );
  }

  return (
    <section className="macro-table-wrap" aria-label="Macro library">
      <table className="macro-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Duration</th>
            <th>Events</th>
            <th>Created</th>
            <th aria-label="Actions" />
          </tr>
        </thead>
        <tbody>
          {macros.map((macro) => (
            <tr
              key={macro.name}
              className={selectedName === macro.name ? "selected" : undefined}
              onClick={() => onSelect(macro)}
            >
              <td>
                <strong>{macro.name}</strong>
              </td>
              <td>{formatDuration(macro.duration_ms)}</td>
              <td>{macro.events.length.toLocaleString()}</td>
              <td>{formatDate(macro.created_at)}</td>
              <td>
                <div className="row-actions">
                  <button
                    type="button"
                    title={`Play ${macro.name}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      onPlay(macro.name);
                    }}
                  >
                    <Play size={14} />
                  </button>
                  <button
                    type="button"
                    title={`Rename ${macro.name}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      onRename(macro);
                    }}
                  >
                    <Pencil size={14} />
                  </button>
                  <button
                    className="icon-danger"
                    type="button"
                    title={`Delete ${macro.name}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      onDelete(macro.name);
                    }}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

function formatDuration(ms: number) {
  if (ms < 1000) {
    return `${ms.toFixed(1)} ms`;
  }
  return `${(ms / 1000).toFixed(2)} s`;
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit"
  }).format(date);
}
