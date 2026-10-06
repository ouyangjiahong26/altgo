import { Globe } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";

interface LanguageSectionProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
}

/** 界面语言区块：选择配置里的 gui_language。 */
export function LanguageSection({ t, config, update }: LanguageSectionProps) {
  return (
    <section className="settings-section settings-section--language">
      <h3 className="settings-section-title">
        <Globe size={14} />
        {t("settings.gui_language")}
      </h3>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.gui_language")}</span>
        <div className="settings-field-control">
          <select
            className="select"
            value={config.guiLanguage}
            onChange={(e) => update("guiLanguage", e.target.value)}
          >
            <option value="">{t("settings.language_auto")}</option>
            <option value="zh">{t("settings.language_zh")}</option>
            <option value="en">{t("settings.language_en")}</option>
          </select>
        </div>
      </div>
    </section>
  );
}
