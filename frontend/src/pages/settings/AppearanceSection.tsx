import { Palette } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import type { ThemePref } from "../../ThemeContext";
import type { FontSizePref, WindowSizePref } from "../../ui-size";

interface AppearanceSectionProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  themePref: ThemePref;
  setTheme: (next: ThemePref) => void;
  fontSize: FontSizePref;
  onFontSizeChange: (value: FontSizePref) => void;
  windowSize: WindowSizePref;
  onWindowSizeChange: (value: WindowSizePref) => void;
}

/** 外观区块：主题、字号、窗口尺寸与悬浮窗位置四组偏好。 */
export function AppearanceSection({
  t,
  config,
  update,
  themePref,
  setTheme,
  fontSize,
  onFontSizeChange,
  windowSize,
  onWindowSizeChange,
}: AppearanceSectionProps) {
  return (
    <section className="settings-section settings-section--appearance">
      <h3 className="settings-section-title">
        <Palette size={14} />
        {t("settings.appearance")}
      </h3>
      <ThemeField t={t} themePref={themePref} setTheme={setTheme} />
      <FontSizeField t={t} fontSize={fontSize} onFontSizeChange={onFontSizeChange} />
      <WindowSizeField t={t} windowSize={windowSize} onWindowSizeChange={onWindowSizeChange} />
      <OverlayPositionField t={t} config={config} update={update} />
    </section>
  );
}

function ThemeField({
  t,
  themePref,
  setTheme,
}: Pick<AppearanceSectionProps, "t" | "themePref" | "setTheme">) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.theme")}</span>
      <div className="settings-field-control">
        <select
          className="select"
          value={themePref}
          onChange={(e) => setTheme(e.target.value as ThemePref)}
        >
          <option value="system">{t("settings.theme_system")}</option>
          <option value="light">{t("settings.theme_light")}</option>
          <option value="dark">{t("settings.theme_dark")}</option>
        </select>
      </div>
    </div>
  );
}

function FontSizeField({
  t,
  fontSize,
  onFontSizeChange,
}: Pick<AppearanceSectionProps, "t" | "fontSize" | "onFontSizeChange">) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.font_size")}</span>
      <div className="settings-field-control">
        <select
          className="select"
          value={fontSize}
          onChange={(e) => onFontSizeChange(e.target.value as FontSizePref)}
        >
          <option value="small">{t("settings.font_size_small")}</option>
          <option value="medium">{t("settings.font_size_medium")}</option>
          <option value="large">{t("settings.font_size_large")}</option>
        </select>
      </div>
    </div>
  );
}

function WindowSizeField({
  t,
  windowSize,
  onWindowSizeChange,
}: Pick<AppearanceSectionProps, "t" | "windowSize" | "onWindowSizeChange">) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.window_size")}</span>
      <div className="settings-field-control">
        <select
          className="select"
          value={windowSize}
          onChange={(e) => onWindowSizeChange(e.target.value as WindowSizePref)}
        >
          <option value="compact">{t("settings.window_size_compact")}</option>
          <option value="standard">{t("settings.window_size_standard")}</option>
          <option value="large">{t("settings.window_size_large")}</option>
        </select>
      </div>
    </div>
  );
}

function OverlayPositionField({
  t,
  config,
  update,
}: Pick<AppearanceSectionProps, "t" | "config" | "update">) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.overlay_position")}</span>
      <div className="settings-field-control">
        <select
          className="select"
          value={config.overlayPosition}
          onChange={(e) => update("overlayPosition", e.target.value)}
        >
          <option value="bottom_center">{t("settings.overlay_position_bottom")}</option>
          <option value="top_center">{t("settings.overlay_position_top")}</option>
        </select>
      </div>
    </div>
  );
}
