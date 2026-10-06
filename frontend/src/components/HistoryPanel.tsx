/** 转写历史面板：主页内的历史列表，支持复制、手动润色与批量删除。 */
import { AlertCircle } from "lucide-react";
import { resolveUiLang, useTranslation } from "../i18n";
import { useHistoryPanel } from "./history/useHistoryPanel";
import { HistoryToolbar } from "./history/HistoryToolbar";
import { HistoryEntryList } from "./history/HistoryEntryList";

export default function HistoryPanel() {
  const { t, lang } = useTranslation();
  // lang 可能是空串（auto，跟随系统语言），时间格式必须走解析后的语言。
  const uiLang = resolveUiLang(lang);
  const h = useHistoryPanel(t);

  return (
    <div className="history-page">
      <HistoryToolbar
        t={t}
        allSelected={h.allSelected}
        someSelected={h.someSelected}
        hasEntries={h.entries.length > 0}
        loading={h.loading}
        onToggleAll={h.toggleAll}
        onDeleteSelected={() => void h.handleDeleteSelected()}
        onClearAll={h.handleClearAll}
      />

      {h.error && (
        <div className="history-error" role="alert">
          <AlertCircle size={18} />
          <span>{h.error}</span>
        </div>
      )}

      {h.loading ? (
        <p className="history-muted">{t("history.loading")}</p>
      ) : h.entries.length === 0 ? (
        <p className="history-empty">{t("history.empty")}</p>
      ) : (
        <HistoryEntryList t={t} uiLang={uiLang} h={h} />
      )}
    </div>
  );
}
