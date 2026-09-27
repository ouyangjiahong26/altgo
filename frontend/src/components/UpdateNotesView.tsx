import { Download, X } from "lucide-react";
import { useTranslation } from "../i18n";
import ReleaseNotes, { parseReleaseNotes } from "./ReleaseNotes";
import { RELEASES_URL, type UpdateInfo } from "../updateNotes";

/**
 * 更新说明窗口的展示层：数据与回调全由外部传入，自身不碰 Tauri API。
 *
 * 抽出来的原因是窗口标记只能有一份——更新说明窗口（`update-notes.tsx`）与
 * 样式预览页都渲染它，各写一份会悄悄漂移。
 */

export interface UpdateNotesViewProps {
  info: UpdateInfo | null;
  installing: boolean;
  /** `install_update` 的失败原因（就地更新档才可能出现）。 */
  installError: string | null;
  onInstall: () => void;
  onClose: () => void;
}

export default function UpdateNotesView({
  info,
  installing,
  installError,
  onInstall,
  onClose,
}: UpdateNotesViewProps) {
  const { t } = useTranslation();
  const body = info?.body ?? "";
  const hasNotes = parseReleaseNotes(body).length > 0;

  return (
    <div className="update-notes-window">
      <header className="update-notes-header" data-tauri-drag-region>
        <span className="update-notes-title" data-tauri-drag-region>
          {t("update_notes.title")}
        </span>
        <button
          type="button"
          className="update-notes-close"
          onClick={onClose}
          title={t("overlay.close")}
          aria-label={t("overlay.close")}
        >
          <X size={14} strokeWidth={2.5} aria-hidden />
        </button>
      </header>

      {info && (
        <div className="update-notes-summary">
          <div className="update-notes-headline">
            {t("settings.update_available")} v{info.latestVersion}
          </div>
          <div className="update-notes-versions">
            <span className="update-notes-version">
              {t("update_notes.current")} {info.currentVersion}
            </span>
            <span className="update-notes-version update-notes-version--latest">
              {t("update_notes.latest")} {info.latestVersion}
            </span>
            {info.date && (
              // latest.json 的日期是完整 ISO 时间戳，头部只展示日期部分。
              <span className="update-notes-date">{info.date.slice(0, 10)}</span>
            )}
          </div>
        </div>
      )}

      <div className="update-notes-body">
        {hasNotes ? (
          <ReleaseNotes md={body} />
        ) : (
          <p className="update-notes-empty">{t("update_notes.empty")}</p>
        )}
      </div>

      <div className="update-notes-actions">
        {installError && <div className="update-notes-error">{installError}</div>}
        {info?.supportTier === "in_place" ? (
          <button
            type="button"
            className="update-notes-btn update-notes-btn--primary"
            onClick={onInstall}
            disabled={installing}
          >
            <Download size={13} aria-hidden />
            {installing ? t("settings.update_installing") : t("settings.update_install")}
          </button>
        ) : (
          <button
            type="button"
            className="update-notes-btn update-notes-btn--primary"
            onClick={() => window.open(RELEASES_URL, "_blank")}
          >
            {t("settings.update_open_release")}
          </button>
        )}
      </div>
    </div>
  );
}
