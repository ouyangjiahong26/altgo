import { Globe } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import { SectionHead, sectionNote } from "./SectionHead";

interface LanguageSectionProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
}

/** 界面语言区块：选择配置里的 gui_language。 */
export function LanguageSection({ t, config, update }: LanguageSectionProps) {
  return (
    <section className="settings-section settings-section--language">
      <SectionHead
        leading={<Globe size={14} aria-hidden="true" />}
        title={t("settings.gui_language")}
        note={sectionNote(t, "settings.gui_language_note")}
      />
      {/* 本区块只有一项，区块头已经写明它是什么，行内不再重复一遍同名标签，
          可访问名称由 aria-label 提供。 */}
      <div className="card settings-rows">
        <div className="settings-field settings-field--solo">
          <div className="settings-field-control">
            <select
              className="select"
              aria-label={t("settings.gui_language")}
              value={config.guiLanguage}
              onChange={(e) => update("guiLanguage", e.target.value)}
            >
              <option value="">{t("settings.language_auto")}</option>
              <option value="zh">{t("settings.language_zh")}</option>
              <option value="en">{t("settings.language_en")}</option>
            </select>
          </div>
        </div>
      </div>
    </section>
  );
}
