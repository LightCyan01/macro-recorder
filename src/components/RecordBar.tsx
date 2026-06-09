import { Circle, Pause, Square } from "lucide-react";

interface RecordBarProps {
  mode: "idle" | "recording" | "playing";
  eventCount: number;
  playbackLabel: string;
  playbackPercent: number;
  onStartRecording: () => void;
  onStopRecording: () => void;
  onStopPlayback: () => void;
}

export function RecordBar({
  mode,
  eventCount,
  playbackLabel,
  playbackPercent,
  onStartRecording,
  onStopRecording,
  onStopPlayback
}: RecordBarProps) {
  return (
    <section className="record-bar" aria-label="Recorder controls">
      <div className={`status-lockup status-${mode}`}>
        <span className="status-dot" />
        <div>
          <p>{mode === "recording" ? "RECORDING" : mode === "playing" ? "PLAYING" : "READY"}</p>
          <strong>
            {mode === "recording"
              ? `${eventCount.toLocaleString()} events captured`
              : mode === "playing"
                ? playbackLabel
                : "Waiting for input"}
          </strong>
        </div>
      </div>

      {mode === "playing" ? (
        <div className="playback-meter" aria-label="Playback progress">
          <span style={{ width: `${playbackPercent}%` }} />
        </div>
      ) : null}

      <div className="control-cluster">
        {mode === "recording" ? (
          <button className="btn-stop-record" type="button" onClick={onStopRecording}>
            <Square size={14} />
            Stop
          </button>
        ) : (
          <button className="btn-record" type="button" onClick={onStartRecording} disabled={mode === "playing"}>
            <Circle size={14} />
            Record
          </button>
        )}

        <button
          className="btn-primary"
          type="button"
          onClick={onStopPlayback}
          disabled={mode !== "playing"}
        >
          <Pause size={14} />
          Stop Play
        </button>
      </div>
    </section>
  );
}
