/** 完成步：按当前配置汇总触发键、转写引擎与润色设置。 */
import { Check } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";

/** 润色级别 → i18n key：完成页摘要显示级别名用。 */
const POLISH_LEVEL_KEYS: Record<string, string> = {
  none: "settings.polish_none",
  light: "settings.polish_light",
  medium: "settings.polish_medium",
  heavy: "settings.polish_heavy",
};

interface DoneStepProps {
  t: (key: string) => string;
  config: AppConfig;
  onlineActive: boolean;
  message: string;
}

export function DoneStep({ t, config, onlineActive, message }: DoneStepProps) {
  return (
    <div className="onboarding-step">
      <div className="onboarding-done-icon">
        <Check size={26} />
      </div>
      <h2 className="onboarding-title">{t("onboarding.done_title")}</h2>
      <div className="onboarding-summary">
        <div className="settings-field">
          <span className="settings-field-label-text">{t("settings.key_name")}</span>
          <div className="settings-field-control">
            <span className="onboarding-summary-value">{config.keyName}</span>
          </div>
        </div>
        <div className="settings-field">
          <span className="settings-field-label-text">{t("settings.transcriber_backend")}</span>
          <div className="settings-field-control">
            <span className="onboarding-summary-value">
              {onlineActive
                ? t("settings.transcriber_backend_online")
                : t("settings.transcriber_backend_local")}
            </span>
          </div>
        </div>
        <div className="settings-field">
          <span className="settings-field-label-text">{t("settings.polishing")}</span>
          <div className="settings-field-control">
            <span className="onboarding-summary-value">
              {config.polishLevel === "none"
                ? t("settings.polish_none")
                : [config.polishModel, t(POLISH_LEVEL_KEYS[config.polishLevel] ?? "settings.polish_none")]
                  .filter(Boolean)
                  .join(" · ")}
            </span>
          </div>
        </div>
      </div>
      {message && message !== "saved" && (
        <p className="settings-hint settings-test-err">{message}</p>
      )}
    </div>
  );
}
