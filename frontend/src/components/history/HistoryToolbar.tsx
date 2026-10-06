/** 历史面板头部：标题与批量操作工具条。 */
import { Trash2 } from "lucide-react";

interface HistoryToolbarProps {
  t: (key: string) => string;
  allSelected: boolean;
  someSelected: boolean;
  hasEntries: boolean;
  loading: boolean;
  onToggleAll: () => void;
  onDeleteSelected: () => void;
  onClearAll: () => void;
}

export function HistoryToolbar({
  t,
  allSelected,
  someSelected,
  hasEntries,
  loading,
  onToggleAll,
  onDeleteSelected,
  onClearAll,
}: HistoryToolbarProps) {
  return (
    <div className="history-page-header">
      <h1 className="history-page-title">{t("history.title")}</h1>
      <div className="history-toolbar">
        <label className="history-select-all">
          <input
            type="checkbox"
            checked={allSelected}
            onChange={onToggleAll}
            disabled={!hasEntries || loading}
          />
          <span>{t("history.select_all")}</span>
        </label>
        <button
          type="button"
          className="history-btn history-btn--danger"
          disabled={!someSelected || loading}
          onClick={onDeleteSelected}
        >
          <Trash2 size={16} />
          {t("history.delete_selected")}
        </button>
        <button
          type="button"
          className="history-btn history-btn--danger history-btn--outline"
          disabled={!hasEntries || loading}
          onClick={onClearAll}
        >
          <Trash2 size={16} />
          {t("history.clear_all")}
        </button>
      </div>
    </div>
  );
}
