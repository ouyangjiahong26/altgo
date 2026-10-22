import { Check, Sparkles } from "lucide-react";
import { useId } from "react";
import type { AppConfig } from "../../hooks/useConfigForm";
import { openUpdateNotes, type UpdateInfo } from "../../updateNotes";
import { SectionHead, sectionNote } from "./SectionHead";

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

/** 关于与更新区块：品牌标记、版本号、手动检查更新与结果提示。 */
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
      <SectionHead
        leading={
          <img src="/altgo-logo.svg" alt="" width={20} height={20} className="settings-section-head-mark" />
        }
        title={t("settings.about")}
        note={sectionNote(t, "settings.about_tagline")}
      />
      <div className="card settings-rows">
        <VersionField
          t={t}
          appVersion={appVersion}
          checkingUpdate={checkingUpdate}
          onCheckUpdate={onCheckUpdate}
        />
        <AutoCheckUpdateField t={t} config={config} update={update} />
        <UpdateStatusMessages t={t} updateInfo={updateInfo} updateError={updateError} />
      </div>
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
      <div className="settings-field-label">
        <span className="settings-field-label-text">{t("settings.version")}</span>
        <span className="settings-field-label-hint">
          <span className="settings-mono tabular">{appVersion || "…"}</span>
        </span>
      </div>
      <div className="settings-field-control">
        <button
          type="button"
          className="btn btn-sm btn-secondary"
          onClick={() => void onCheckUpdate()}
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
  const labelId = useId();

  return (
    <div className="settings-field">
      <span className="settings-field-label-text" id={labelId}>
        {t("settings.auto_check_update")}
      </span>
      <div className="settings-field-control">
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={config.autoCheckUpdate}
          aria-labelledby={labelId}
          onClick={() => update("autoCheckUpdate", !config.autoCheckUpdate)}
        />
      </div>
    </div>
  );
}

function UpdateStatusMessages({
  t,
  updateInfo,
  updateError,
}: Pick<AboutSectionProps, "t" | "updateInfo" | "updateError">) {
  if (updateError) {
    return <p className="settings-card-note settings-test-err">{updateError}</p>;
  }

  if (!updateInfo) return null;

  if (!updateInfo.hasUpdate) {
    return (
      <p className="settings-card-note">
        <span className="badge badge-success">
          <Check size={11} aria-hidden="true" />
          {t("settings.update_not_found")}
        </span>
        <span className="settings-mono tabular">{updateInfo.latestVersion}</span>
      </p>
    );
  }

  return (
    <div className="settings-row-actions">
      <span className="badge badge-warning">
        <Sparkles size={11} aria-hidden="true" />
        {t("settings.update_available")}
        <span className="tabular">v{updateInfo.latestVersion}</span>
      </span>
      <button
        type="button"
        className="btn btn-sm btn-secondary"
        onClick={() => void openUpdateNotes(updateInfo)}
      >
        {t("settings.view_update_notes")}
      </button>
    </div>
  );
}
