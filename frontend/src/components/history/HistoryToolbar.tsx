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
      {/* 面板嵌在主页里，标题用分组标签的语气，不跟页面标题抢层级 */}
      <h1 className="section-label history-page-title">{t("history.title")}</h1>
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
          className="btn btn-sm btn-danger"
          disabled={!someSelected || loading}
          onClick={onDeleteSelected}
        >
          <Trash2 size={13} />
          {t("history.delete_selected")}
        </button>
        {/* 清空全部比删除所选重一档，用描边把它与前者分开 */}
        <button
          type="button"
          className="btn btn-sm btn-outline btn-danger"
          disabled={!hasEntries || loading}
          onClick={onClearAll}
        >
          <Trash2 size={13} />
          {t("history.clear_all")}
        </button>
      </div>
    </div>
  );
}
