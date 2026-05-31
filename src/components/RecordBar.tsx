import { Circle, Play, Square } from "lucide-react";

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
              ? `${eventCount.toLocaleString()} events`
              : mode === "playing"
                ? playbackLabel
                : "No active run"}
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
          <button className="danger" type="button" onClick={onStopRecording}>
            <Square size={16} />
            Stop
          </button>
        ) : (
          <button type="button" onClick={onStartRecording} disabled={mode === "playing"}>
            <Circle size={16} />
            Record
          </button>
        )}

        <button type="button" onClick={onStopPlayback} disabled={mode !== "playing"}>
          <Play size={16} />
          Stop Play
        </button>
      </div>
    </section>
  );
}
