import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { FolderOpen, Keyboard, Library, Settings, Square, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  deleteMacro,
  getHotkeys,
  getPlaybackOptions,
  getSaveDirectory,
  listMacros,
  pickDirectory,
  playMacro,
  renameMacro,
  setHotkeys,
  setPlaybackOptions,
  setSaveDirectory,
  startRecording,
  stopPlayback,
  stopRecording,
  updateMacro
} from "./api";
import { HotkeyConfigPanel } from "./components/HotkeyConfig";
import { MacroEditor } from "./components/MacroEditor";
import { MacroList } from "./components/MacroList";
import { RecordBar } from "./components/RecordBar";
import {
  DEFAULT_HOTKEYS,
  type HotkeyConfig,
  type MacroFile,
  type PlaybackOptions,
  type PlaybackProgressEvent,
  type RecordingCountEvent
} from "./types";

type View = "library" | "settings";
type Mode = "idle" | "recording" | "playing";

const DEFAULT_PLAYBACK: PlaybackOptions = {
  loop_count: 1,
  speed_multiplier: 1,
  infinite_loop: false
};

export default function App() {
  const [view, setView] = useState<View>("library");
  const [mode, setMode] = useState<Mode>("idle");
  const [macros, setMacros] = useState<MacroFile[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [recordedCount, setRecordedCount] = useState(0);
  const [playbackProgress, setPlaybackProgress] = useState<PlaybackProgressEvent | null>(null);
  const [hotkeys, setHotkeyState] = useState<HotkeyConfig>(DEFAULT_HOTKEYS);
  const [playbackOptions, setPlaybackState] = useState<PlaybackOptions>(DEFAULT_PLAYBACK);
  const [saveDirectory, setSaveDirectoryState] = useState<string>("");
  const [notice, setNotice] = useState<string | null>(null);
  const [noticeFading, setNoticeFading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const noticeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const selectedMacro = useMemo(
    () => macros.find((macro) => macro.name === selectedName) ?? macros[0] ?? null,
    [macros, selectedName]
  );

  const playbackPercent = playbackProgress?.total
    ? Math.min(100, (playbackProgress.fired / playbackProgress.total) * 100)
    : 0;

  // Auto-dismiss notice
  const showNotice = useCallback((message: string) => {
    if (noticeTimerRef.current) {
      clearTimeout(noticeTimerRef.current);
    }
    setNoticeFading(false);
    setNotice(message);

    noticeTimerRef.current = setTimeout(() => {
      setNoticeFading(true);
      setTimeout(() => {
        setNotice(null);
        setNoticeFading(false);
      }, 400);
    }, 3000);
  }, []);

  useEffect(() => {
    return () => {
      if (noticeTimerRef.current) {
        clearTimeout(noticeTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function boot() {
      try {
        const [loadedMacros, loadedHotkeys, loadedPlayback, loadedSaveDir] = await Promise.all([
          listMacros(),
          getHotkeys(),
          getPlaybackOptions(),
          getSaveDirectory()
        ]);
        if (cancelled) {
          return;
        }
        setMacros(loadedMacros);
        setHotkeyState(loadedHotkeys);
        setPlaybackState(loadedPlayback);
        setSaveDirectoryState(loadedSaveDir);
        setSelectedName(loadedMacros[0]?.name ?? null);
      } catch (caught) {
        if (!cancelled) {
          setError(String(caught));
        }
      }
    }

    void boot();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn[] = [];
    let cancelled = false;

    async function wireEvents() {
      const listeners = await Promise.all([
        listen<RecordingCountEvent>("recording-event-count", (event) => {
          setRecordedCount(event.payload.count);
          setMode("recording");
        }),
        listen<MacroFile[]>("macros-updated", (event) => {
          setMacros(event.payload);
          setSelectedName((current) => current ?? event.payload[0]?.name ?? null);
        }),
        listen<MacroFile>("recording-saved", (event) => {
          setMode("idle");
          setRecordedCount(0);
          setSelectedName(event.payload.name);
          showNotice(`Saved ${event.payload.name}`);
        }),
        listen<PlaybackProgressEvent>("playback-progress", (event) => {
          setMode("playing");
          setPlaybackProgress(event.payload);
        }),
        listen<string>("playback-stopped", () => {
          setMode("idle");
          setPlaybackProgress(null);
        }),
        listen<string>("macro-error", (event) => {
          setError(event.payload);
        })
      ]);

      if (cancelled) {
        listeners.forEach((dispose) => dispose());
        return;
      }
      unlisten = listeners;
    }

    void wireEvents();
    return () => {
      cancelled = true;
      unlisten.forEach((dispose) => dispose());
    };
  }, [showNotice]);

  async function refreshMacros() {
    const next = await listMacros();
    setMacros(next);
    setSelectedName((current) => current ?? next[0]?.name ?? null);
  }

  async function handleStartRecording() {
    setError(null);
    await startRecording();
    setRecordedCount(0);
    setMode("recording");
  }

  async function handleStopRecording() {
    const fallbackName = `Macro ${new Date().toLocaleString().replace(/[/:,]/g, "-")}`;
    const name = window.prompt("Macro name", fallbackName);
    if (!name) {
      return;
    }

    setError(null);
    const saved = await stopRecording(name);
    setMode("idle");
    setRecordedCount(0);
    setSelectedName(saved.name);
    await refreshMacros();
  }

  async function handlePlayMacro(name: string) {
    setError(null);
    await playMacro(name);
    setPlaybackProgress({ name, fired: 0, total: 1, loop_index: 1 });
    setMode("playing");
  }

  async function handleStopPlayback() {
    setError(null);
    await stopPlayback();
    setMode("idle");
    setPlaybackProgress(null);
  }

  async function handleDelete(name: string) {
    if (!window.confirm(`Delete ${name}?`)) {
      return;
    }
    const next = await deleteMacro(name);
    setMacros(next);
    setSelectedName(next[0]?.name ?? null);
    if (playbackProgress?.name === name) {
      setMode("idle");
      setPlaybackProgress(null);
    }
  }

  async function handleRename(macro: MacroFile) {
    const nextName = window.prompt("New macro name", macro.name);
    if (!nextName || nextName === macro.name) {
      return;
    }
    const renamed = await renameMacro(macro.name, nextName);
    await refreshMacros();
    setSelectedName(renamed.name);
    if (playbackProgress?.name === macro.name) {
      setMode("idle");
      setPlaybackProgress(null);
    }
  }

  async function handleUpdateMacro(macro: MacroFile) {
    const saved = await updateMacro(macro);
    setMacros((current) =>
      current.map((item) => (item.name === saved.name ? saved : item))
    );
    setSelectedName(saved.name);
    showNotice(`Saved ${saved.name}`);
    return saved;
  }

  async function handleSaveHotkeys(next: HotkeyConfig) {
    await setHotkeys(next);
    setHotkeyState(next);
    showNotice("Hotkeys saved");
  }

  async function handlePlaybackOptionChange(next: PlaybackOptions) {
    const saved = await setPlaybackOptions(next);
    setPlaybackState(saved);
  }

  async function handleBrowseSaveDirectory() {
    const selected = await pickDirectory(saveDirectory || undefined);
    if (!selected) {
      return;
    }
    setError(null);
    const saved = await setSaveDirectory(selected);
    setSaveDirectoryState(saved);
    showNotice("Save directory updated");
    await refreshMacros();
  }

  return (
    <main className="app-shell">
      <aside className="side-rail">
        <div className="brand-lockup">
          <span>MR</span>
          <div>
            <strong>Macro Recorder</strong>
            <small>Windows input lab</small>
          </div>
        </div>

        <nav className="nav-stack" aria-label="Primary">
          <button
            className={view === "library" ? "active" : undefined}
            type="button"
            onClick={() => setView("library")}
          >
            <Library size={16} />
            Library
          </button>
          <button
            className={view === "settings" ? "active" : undefined}
            type="button"
            onClick={() => setView("settings")}
          >
            <Settings size={16} />
            Settings
          </button>
        </nav>

        <div className="hotkey-stack">
          <Keyboard size={14} />
          <span>{hotkeys.record_toggle.key}</span>
          <span>{hotkeys.play_toggle.key}</span>
          <span>{hotkeys.emergency_stop.key}</span>
        </div>
      </aside>

      <section className="workbench">
        <RecordBar
          mode={mode}
          eventCount={recordedCount}
          playbackLabel={
            playbackProgress
              ? `${playbackProgress.name} (${playbackProgress.fired}/${playbackProgress.total})`
              : "No active run"
          }
          playbackPercent={playbackPercent}
          onStartRecording={() => void handleStartRecording().catch(setErrorFromUnknown)}
          onStopRecording={() => void handleStopRecording().catch(setErrorFromUnknown)}
          onStopPlayback={() => void handleStopPlayback().catch(setErrorFromUnknown)}
        />

        {notice ? (
          <div className={`notice${noticeFading ? " fading" : ""}`}>{notice}</div>
        ) : null}
        {error ? (
          <div className="error-banner">
            <Square size={14} />
            {error}
            <button
              type="button"
              className="error-dismiss"
              title="Dismiss error"
              onClick={() => setError(null)}
            >
              <X size={14} />
            </button>
          </div>
        ) : null}

        {view === "library" ? (
          <div className="library-grid">
            <MacroList
              macros={macros}
              selectedName={selectedMacro?.name ?? null}
              onSelect={(macro) => setSelectedName(macro.name)}
              onPlay={(name) => void handlePlayMacro(name).catch(setErrorFromUnknown)}
              onDelete={(name) => void handleDelete(name).catch(setErrorFromUnknown)}
              onRename={(macro) => void handleRename(macro).catch(setErrorFromUnknown)}
            />
            <MacroEditor macro={selectedMacro} onSave={handleUpdateMacro} />
          </div>
        ) : (
          <div className="settings-grid">
            <HotkeyConfigPanel persistedConfig={hotkeys} onSave={handleSaveHotkeys} />
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              <section className="settings-panel" aria-label="Playback settings">
                <div className="section-title">
                  <h2>Playback</h2>
                </div>
                <label className="field-row">
                  <span>Loop Count</span>
                  <input
                    type="number"
                    min={1}
                    max={999}
                    value={playbackOptions.loop_count}
                    disabled={playbackOptions.infinite_loop}
                    onChange={(event) =>
                      void handlePlaybackOptionChange({
                        ...playbackOptions,
                        loop_count: Number(event.currentTarget.value)
                      }).catch(setErrorFromUnknown)
                    }
                  />
                </label>
                <label className="check-row">
                  <input
                    type="checkbox"
                    checked={playbackOptions.infinite_loop}
                    onChange={(event) =>
                      void handlePlaybackOptionChange({
                        ...playbackOptions,
                        infinite_loop: event.currentTarget.checked
                      }).catch(setErrorFromUnknown)
                    }
                  />
                  <span>Infinite loop</span>
                </label>
                <label className="field-row">
                  <span>Speed</span>
                  <input
                    type="range"
                    min={0.5}
                    max={2}
                    step={0.05}
                    value={playbackOptions.speed_multiplier}
                    onChange={(event) =>
                      void handlePlaybackOptionChange({
                        ...playbackOptions,
                        speed_multiplier: Number(event.currentTarget.value)
                      }).catch(setErrorFromUnknown)
                    }
                  />
                  <strong>{playbackOptions.speed_multiplier.toFixed(2)}x</strong>
                </label>
              </section>

              <section className="settings-panel" aria-label="Storage settings">
                <div className="section-title">
                  <h2>Storage</h2>
                </div>
                <p style={{ color: "var(--text-secondary)", marginBottom: 12, fontSize: 13 }}>
                  Choose where macro recordings are saved on disk.
                </p>
                <div className="storage-row">
                  <input
                    type="text"
                    readOnly
                    value={saveDirectory}
                    title={saveDirectory}
                    placeholder="Default app data directory"
                  />
                  <button
                    type="button"
                    onClick={() => void handleBrowseSaveDirectory().catch(setErrorFromUnknown)}
                  >
                    <FolderOpen size={14} />
                    Browse
                  </button>
                </div>
              </section>
            </div>
          </div>
        )}
      </section>
    </main>
  );

  function setErrorFromUnknown(caught: unknown) {
    setError(String(caught));
  }
}
