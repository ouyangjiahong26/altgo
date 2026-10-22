import { Keyboard, Mic } from "lucide-react";
import { useId } from "react";
import { KEY_PRESETS, isPresetKeyName, presetSelectValue } from "../../config/keyPresets";
import type { AppConfig } from "../../hooks/useConfigForm";
import { SectionHead, sectionNote } from "./SectionHead";

interface RecordingSectionProps {
  "data-settings-order"?: number;
  t: (key: string) => string;
  config: AppConfig;
  onKeyNameChange: (keyName: string) => void;
  saving: boolean;
  keyCapturing: boolean;
  onCaptureActivationKey: () => Promise<void>;
}

/** 录音区块：触发键预设选择、自定义键名与按键捕获。 */
export function RecordingSection({
  "data-settings-order": order,
  t,
  config,
  onKeyNameChange,
  saving,
  keyCapturing,
  onCaptureActivationKey,
}: RecordingSectionProps) {
  return (
    <section data-settings-order={order} className="settings-section settings-section--recording">
      <SectionHead
        leading={<Mic size={14} aria-hidden="true" />}
        title={t("settings.recording")}
        note={sectionNote(t, "settings.recording_note")}
      />
      <div className="card settings-rows">
        <KeyNameField t={t} config={config} onKeyNameChange={onKeyNameChange} />
        <CustomKeyField t={t} config={config} onKeyNameChange={onKeyNameChange} />
        <CaptureActivationField
          t={t}
          saving={saving}
          keyCapturing={keyCapturing}
          onCaptureActivationKey={onCaptureActivationKey}
        />
      </div>
    </section>
  );
}

function KeyNameField({
  t,
  config,
  onKeyNameChange,
}: Pick<RecordingSectionProps, "t" | "config" | "onKeyNameChange">) {
  const labelId = useId();
  // 非预设按键的键名是抓到的原始 keysym，放在标签下方当读数，比塞进下拉旁边更易读。
  const custom = !isPresetKeyName(config.keyName);

  return (
    <div className="settings-field">
      <div className="settings-field-label">
        <span className="settings-field-label-text" id={labelId}>
          {t("settings.key_name")}
        </span>
        {custom && (
          <span className="settings-field-label-hint">
            {t("settings.key_binding_active")}
            <code className="kbd">{config.keyName}</code>
          </span>
        )}
      </div>
      <div className="settings-field-control">
        <select
          className="select"
          aria-labelledby={labelId}
          value={presetSelectValue(config.keyName)}
          onChange={(e) => {
            if (e.target.value === "__custom__") return;
            onKeyNameChange(e.target.value);
          }}
        >
          {KEY_PRESETS.map((p) => (
            <option key={p.value} value={p.value}>
              {t(p.labelKey)}
            </option>
          ))}
          <option value="__custom__">{t("settings.key_custom")}</option>
        </select>
      </div>
    </div>
  );
}

function CustomKeyField({
  t,
  config,
  onKeyNameChange,
}: Pick<RecordingSectionProps, "t" | "config" | "onKeyNameChange">) {
  const labelId = useId();
  // 仅自定义键名且尚未绑定 evdev 码时可手改键名。带码的绑定只能由按键捕获流程写回。
  if (isPresetKeyName(config.keyName) || config.linuxEvdevCode != null) return null;

  return (
    <div className="settings-field">
      <span className="settings-field-label-text" id={labelId}>
        {t("settings.key_custom_value")}
      </span>
      <div className="settings-field-control">
        <input
          type="text"
          className="field"
          aria-labelledby={labelId}
          value={config.keyName}
          onChange={(e) => onKeyNameChange(e.target.value)}
        />
      </div>
    </div>
  );
}

function CaptureActivationField({
  t,
  saving,
  keyCapturing,
  onCaptureActivationKey,
}: Pick<
  RecordingSectionProps,
  "t" | "saving" | "keyCapturing" | "onCaptureActivationKey"
>) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.capture_activation")}</span>
      <div className="settings-field-control">
        <button
          type="button"
          className="btn btn-sm btn-secondary"
          onClick={() => void onCaptureActivationKey()}
          disabled={saving || keyCapturing}
        >
          <Keyboard size={14} aria-hidden="true" />
          {keyCapturing ? t("settings.capture_waiting") : t("settings.capture_activation_short")}
        </button>
      </div>
    </div>
  );
}
