import { Download, X } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "../i18n";
import ReleaseNotes, { parseReleaseNotes } from "./ReleaseNotes";
import { RELEASES_URL, type UpdateInfo } from "../updateNotes";

/**
 * 更新说明窗口的展示层：数据与回调全由外部传入，自身不碰 Tauri API。
 *
 * 抽出来的原因是窗口标记只能有一份：更新说明窗口（`update-notes.tsx`）与
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

      {info && <VersionSummary t={t} info={info} />}

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
            className="btn btn-primary btn-lg btn-block"
            onClick={onInstall}
            disabled={installing}
          >
            <Download size={13} aria-hidden />
            {installing ? t("settings.update_installing") : t("settings.update_install")}
          </button>
        ) : (
          <button
            type="button"
            className="btn btn-primary btn-lg btn-block"
            onClick={() => void openUrl(RELEASES_URL)}
          >
            {t("settings.update_open_release")}
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * 版本摘要：一句「发现新版本」加版本号，再跟一行当前版本与发布日期。
 *
 * 版本号是这扇窗里唯一的读数，给它整块抬一级的面与最大字号；两个版本号不再
 * 并排比较（读者关心的是要不要升，而不是两串数字的差），当前版本退成脚注。
 */
function VersionSummary({
  t,
  info,
}: {
  t: (key: string) => string;
  info: UpdateInfo;
}) {
  return (
    <div className="update-notes-summary">
      <div className="update-notes-headline">
        <span className="update-notes-kicker">{t("settings.update_available")}</span>
        <span className="update-notes-version-number tabular">v{info.latestVersion}</span>
      </div>
      <div className="update-notes-versions">
        <span>
          {t("update_notes.current")} {info.currentVersion}
        </span>
        {info.date && (
          // latest.json 的日期是完整 ISO 时间戳，头部只展示日期部分。
          <span className="update-notes-date">{info.date.slice(0, 10)}</span>
        )}
      </div>
    </div>
  );
}
