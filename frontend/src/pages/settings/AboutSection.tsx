import { Check, Sparkles } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import { openUpdateNotes, type UpdateInfo } from "../../updateNotes";

interface AboutSectionProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  appVersion: string;
  checkingUpdate: boolean;
  updateInfo: UpdateInfo | null;
  updateError: string | null;
  onCheckUpdate: () => Promise<void>;
}

/** 关于与更新区块：品牌信息、版本号、手动检查更新与结果提示。 */
export function AboutSection({
  t,
  config,
  update,
  appVersion,
  checkingUpdate,
  updateInfo,
  updateError,
  onCheckUpdate,
}: AboutSectionProps) {
  return (
    <section className="settings-section settings-section--about">
      <h3 className="settings-section-title">
        <Sparkles size={14} />
        {t("settings.about")}
      </h3>
      <div className="settings-about-brand">
        <img src="/altgo-logo.svg" alt="" width={40} height={40} className="settings-about-logo" />
        <p className="settings-about-tagline">{t("settings.about_tagline")}</p>
      </div>
      <VersionField
        t={t}
        appVersion={appVersion}
        checkingUpdate={checkingUpdate}
        onCheckUpdate={onCheckUpdate}
      />
      <AutoCheckUpdateField t={t} config={config} update={update} />
      <UpdateStatusMessages t={t} updateInfo={updateInfo} updateError={updateError} />
    </section>
  );
}

function VersionField({
  t,
  appVersion,
  checkingUpdate,
  onCheckUpdate,
}: Pick<AboutSectionProps, "t" | "appVersion" | "checkingUpdate" | "onCheckUpdate">) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.version")}</span>
      <div className="settings-field-control" style={{ display: "flex", gap: "var(--space-2)", alignItems: "center" }}>
        <span className="settings-muted">{appVersion || "…"}</span>
        <button
          type="button"
          className="btn btn-secondary btn-sm"
          onClick={onCheckUpdate}
          disabled={checkingUpdate}
        >
          {checkingUpdate ? t("settings.checking_update") : t("settings.check_update")}
        </button>
      </div>
    </div>
  );
}

function AutoCheckUpdateField({
  t,
  config,
  update,
}: Pick<AboutSectionProps, "t" | "config" | "update">) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.auto_check_update")}</span>
      <div className="settings-field-control">
        <label style={{ display: "flex", alignItems: "center", cursor: "pointer", gap: "var(--space-1-5)" }}>
          <input
            type="checkbox"
            checked={config.autoCheckUpdate}
            onChange={(e) => update("autoCheckUpdate", e.target.checked)}
          />
        </label>
      </div>
    </div>
  );
}

function UpdateStatusMessages({
  t,
  updateInfo,
  updateError,
}: Pick<AboutSectionProps, "t" | "updateInfo" | "updateError">) {
  return (
    <>
      {updateError && (
        <div className="settings-save-msg settings-save-msg--err" style={{ marginTop: "var(--space-2)" }}>
          {updateError}
        </div>
      )}

      {updateInfo && !updateInfo.hasUpdate && !updateError && (
        <div className="settings-save-msg settings-save-msg--ok" style={{ marginTop: "var(--space-2)" }}>
          <Check size={12} /> {t("settings.update_not_found")} ({updateInfo.latestVersion})
        </div>
      )}

      {updateInfo && updateInfo.hasUpdate && (
        <div style={{ marginTop: "var(--space-2)", display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
          <span
            style={{
              display: "flex",
              alignItems: "center",
              gap: "var(--space-1-5)",
              fontSize: "var(--text-sm)",
              fontWeight: 600,
              color: "var(--color-accent)",
            }}
          >
            <Sparkles size={14} />
            {t("settings.update_available")} v{updateInfo.latestVersion}
          </span>
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={() => openUpdateNotes(updateInfo)}
          >
            {t("settings.view_update_notes")}
          </button>
        </div>
      )}
    </>
  );
}
