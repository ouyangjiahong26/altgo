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
      {/* 完成标记用语义绿的小圆盘，形状与悬浮窗的完成指示一致。 */}
      <div className="onboarding-done-icon">
        <Check size={20} strokeWidth={2.5} aria-hidden />
      </div>
      <h2 className="onboarding-title">{t("onboarding.done_title")}</h2>
      <div className="card onboarding-card onboarding-list onboarding-summary">
        <SummaryRow label={t("settings.key_name")} value={config.keyName} />
        <SummaryRow
          label={t("settings.transcriber_backend")}
          value={
            onlineActive
              ? t("settings.transcriber_backend_online")
              : t("settings.transcriber_backend_local")
          }
        />
        <SummaryRow label={t("settings.polishing")} value={polishSummary(t, config)} />
      </div>
      {message && message !== "saved" && (
        <p className="onboarding-error">{message}</p>
      )}
    </div>
  );
}

/** 一行摘要：标签在左、取值在右，取值过长时换行而不是截断。 */
function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="onboarding-item">
      <span className="onboarding-field-label">{label}</span>
      <span className="onboarding-summary-value">{value}</span>
    </div>
  );
}

/** 润色摘要：本地档显示后端、档位与模型；云端档显示模型与档位。 */
function polishSummary(t: (key: string) => string, config: AppConfig): string {
  if (config.polishBackend === "local") {
    return [
      t("settings.polish_backend_local"),
      config.polishLevel === "none"
        ? t("settings.polish_local_level_none")
        : t("settings.polish_local_level_on"),
      config.polishLocalModel,
    ]
      .filter(Boolean)
      .join(" · ");
  }
  if (config.polishLevel === "none") return t("settings.polish_none");
  return [
    config.polishModel,
    t(POLISH_LEVEL_KEYS[config.polishLevel] ?? "settings.polish_none"),
  ]
    .filter(Boolean)
    .join(" · ");
}
