import { RotateCcw, Save } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { DEFAULT_HOTKEYS, type HotkeyBinding, type HotkeyConfig } from "../types";

type HotkeyKey = keyof HotkeyConfig;

interface HotkeyConfigProps {
  persistedConfig: HotkeyConfig;
  onSave: (config: HotkeyConfig) => Promise<void>;
}

const LABELS: Record<HotkeyKey, string> = {
  record_toggle: "Record Toggle",
  play_toggle: "Play Toggle",
  emergency_stop: "Emergency Stop"
};

export function HotkeyConfigPanel({ persistedConfig, onSave }: HotkeyConfigProps) {
  const [config, setConfig] = useState<HotkeyConfig>(persistedConfig);
  const [capturing, setCapturing] = useState<HotkeyKey | null>(null);
  const [errors, setErrors] = useState<Partial<Record<HotkeyKey | "save", string>>>({});
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setConfig(persistedConfig);
  }, [persistedConfig]);

  useEffect(() => {
    if (!capturing) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      const mapped = bindingFromKeyboardEvent(event);
      if (!mapped) {
        setErrors((current) => ({
          ...current,
          [capturing]: "Use a non-modifier key."
        }));
        return;
      }

      const conflict = findConflict(mapped, config, capturing);
      if (conflict) {
        setErrors((current) => ({
          ...current,
          [capturing]: `This key is already used for ${conflict}.`
        }));
        return;
      }

      setConfig((current) => ({ ...current, [capturing]: mapped }));
      setErrors((current) => ({ ...current, [capturing]: undefined, save: undefined }));
      setCapturing(null);
    };

    window.addEventListener("keydown", handleKeyDown, { once: false, capture: true });
    return () => window.removeEventListener("keydown", handleKeyDown, { capture: true });
  }, [capturing, config]);

  const dirty = useMemo(
    () => JSON.stringify(config) !== JSON.stringify(persistedConfig),
    [config, persistedConfig]
  );

  async function save() {
    setSaving(true);
    setErrors((current) => ({ ...current, save: undefined }));
    try {
      await onSave(config);
    } catch (error) {
      setErrors((current) => ({ ...current, save: String(error) }));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="settings-panel" aria-label="Hotkey settings">
      <div className="section-title">
        <h2>Hotkeys</h2>
        <button type="button" onClick={() => setConfig(DEFAULT_HOTKEYS)}>
          <RotateCcw size={15} />
          Defaults
        </button>
      </div>

      {(Object.keys(LABELS) as HotkeyKey[]).map((key) => (
        <div className="hotkey-row" key={key}>
          <label>{LABELS[key]}</label>
          <button
            className={capturing === key ? "capture-field capturing" : "capture-field"}
            type="button"
            onClick={() => {
              setCapturing(key);
              setErrors((current) => ({ ...current, [key]: undefined }));
            }}
          >
            {capturing === key ? "Press any key..." : config[key].key}
          </button>
          <button type="button" onClick={() => setCapturing(key)}>
            Change
          </button>
          {errors[key] ? <p className="inline-error">{errors[key]}</p> : null}
        </div>
      ))}

      {dirty ? (
        <div className="settings-actions">
          <button type="button" onClick={save} disabled={saving}>
            <Save size={15} />
            Save
          </button>
          {errors.save ? <p className="inline-error">{errors.save}</p> : null}
        </div>
      ) : null}
    </section>
  );
}

function findConflict(
  newBinding: HotkeyBinding,
  currentConfig: HotkeyConfig,
  excludeKey: HotkeyKey
): string | null {
  for (const [bindingKey, binding] of Object.entries(currentConfig) as [
    HotkeyKey,
    HotkeyBinding
  ][]) {
    if (bindingKey === excludeKey) {
      continue;
    }

    if (
      binding.vk_code === newBinding.vk_code &&
      binding.modifiers.ctrl === newBinding.modifiers.ctrl &&
      binding.modifiers.alt === newBinding.modifiers.alt &&
      binding.modifiers.shift === newBinding.modifiers.shift
    ) {
      return LABELS[bindingKey];
    }
  }
  return null;
}

function bindingFromKeyboardEvent(event: KeyboardEvent): HotkeyBinding | null {
  const vkCode = vkCodeFromEvent(event);
  if (vkCode === 0) {
    return null;
  }

  const key = normalizeDisplayKey(event, vkCode);
  const modifiers = {
    ctrl: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey
  };
  const prefix = [
    modifiers.ctrl ? "Ctrl" : null,
    modifiers.alt ? "Alt" : null,
    modifiers.shift ? "Shift" : null
  ].filter(Boolean);

  return {
    key: [...prefix, key].join("+"),
    vk_code: vkCode,
    modifiers
  };
}

function vkCodeFromEvent(event: KeyboardEvent) {
  if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) {
    return 0;
  }

  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.key)) {
    return 0x70 + Number(event.key.slice(1)) - 1;
  }

  if (/^Key[A-Z]$/.test(event.code)) {
    return event.code.charCodeAt(3);
  }

  if (/^Digit[0-9]$/.test(event.code)) {
    return event.code.charCodeAt(5);
  }

  const special: Record<string, number> = {
    Escape: 0x1b,
    Space: 0x20,
    Enter: 0x0d,
    Tab: 0x09,
    Backspace: 0x08,
    Delete: 0x2e,
    Insert: 0x2d,
    Home: 0x24,
    End: 0x23,
    PageUp: 0x21,
    PageDown: 0x22,
    ArrowLeft: 0x25,
    ArrowUp: 0x26,
    ArrowRight: 0x27,
    ArrowDown: 0x28
  };

  return special[event.key] ?? 0;
}

function normalizeDisplayKey(event: KeyboardEvent, vkCode: number) {
  if (vkCode >= 0x70 && vkCode <= 0x87) {
    return `F${vkCode - 0x70 + 1}`;
  }
  if (/^Key[A-Z]$/.test(event.code)) {
    return event.code.slice(3);
  }
  if (/^Digit[0-9]$/.test(event.code)) {
    return event.code.slice(5);
  }
  return event.key.length === 1 ? event.key.toUpperCase() : event.key;
}
