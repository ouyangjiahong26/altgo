/** 设置页：状态与副作用在 settings/useSettingsForm，区块渲染树在 settings/ 目录。 */
import { useSettingsForm } from "./settings/useSettingsForm";
import { SettingsBody } from "./settings/SettingsBody";

export default function Settings() {
  const s = useSettingsForm();
  const { t, config } = s;

  if (!config) {
    return <div className="loading-container">{t("settings.loading")}</div>;
  }

  return (
    <div className="settings-page">
      <div className="settings-form">
        <SettingsBody s={s} />
      </div>
    </div>
  );
}
