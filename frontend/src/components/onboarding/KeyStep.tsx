/** 触发键步：选预设备注键、自定义键名或现场捕获。 */
import { Keyboard } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import { KEY_PRESETS, isPresetKeyName, presetSelectValue } from "../../config/keyPresets";

interface KeyStepProps {
  t: (key: string) => string;
  stepLabel: (n: number) => string;
  config: AppConfig;
  setConfig: React.Dispatch<React.SetStateAction<AppConfig | null>>;
  saving: boolean;
  keyCapturing: boolean;
  captureActivationKey: () => Promise<void>;
}

export function KeyStep({
  t,
  stepLabel,
  config,
  setConfig,
  saving,
  keyCapturing,
  captureActivationKey,
}: KeyStepProps) {
  return (
    <div className="onboarding-step">
      <div className="onboarding-step-label">{stepLabel(1)}</div>
      <h2 className="onboarding-title">{t("onboarding.key_title")}</h2>
      <div className="onboarding-card">
        <div className="settings-field">
          <span className="settings-field-label-text">{t("settings.key_name")}</span>
          <div className="settings-field-control">
            <select
              className="select"
              value={presetSelectValue(config.keyName)}
              onChange={(e) => {
                if (e.target.value === "__custom__") return;
                setConfig((prev) =>
                  prev
                    ? { ...prev, keyName: e.target.value, linuxEvdevCode: null }
                    : prev,
                );
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
        {!isPresetKeyName(config.keyName) && config.linuxEvdevCode == null && (
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.key_custom_value")}</span>
            <div className="settings-field-control">
              <input
                type="text"
                className="field"
                value={config.keyName}
                onChange={(e) =>
                  setConfig((prev) =>
                    prev
                      ? { ...prev, keyName: e.target.value, linuxEvdevCode: null }
                      : prev,
                  )
                }
              />
            </div>
          </div>
        )}
        <div className="settings-field">
          <span className="settings-field-label-text">{t("settings.capture_activation")}</span>
          <div className="settings-field-control">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void captureActivationKey()}
              disabled={saving || keyCapturing}
            >
              <Keyboard size={14} />
              {keyCapturing
                ? t("settings.capture_waiting")
                : t("settings.capture_activation_short")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
