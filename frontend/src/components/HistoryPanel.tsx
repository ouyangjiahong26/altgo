/** 转写历史面板：主页内的历史列表，支持复制、手动润色与批量删除。 */
import { useEffect, useRef } from "react";
import { AlertCircle, ListMusic } from "lucide-react";
import { resolveUiLang, useTranslation } from "../i18n";
import { useHistoryPanel } from "./history/useHistoryPanel";
import { HistoryToolbar } from "./history/HistoryToolbar";
import { HistoryEntryList } from "./history/HistoryEntryList";

export default function HistoryPanel() {
  const { t, lang } = useTranslation();
  // lang 可能是空串（auto，跟随系统语言），时间格式必须走解析后的语言。
  const uiLang = resolveUiLang(lang);
  const h = useHistoryPanel(t);
  const sentinelRef = useRef<HTMLDivElement | null>(null);

  // 滚动接近列表尾部时补挂下一块条目，主页重挂载的首屏渲染量因此恒定。
  useEffect(() => {
    const el = sentinelRef.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) h.showMore();
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [h.showMore, h.hasMore]);

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
        // 空态给图标与一句指引，动作留给上方的状态块，避免两个入口互相抢注意力
        <div className="empty-state history-empty">
          <div className="empty-state-icon">
            <ListMusic size={18} />
          </div>
          <p className="empty-state-hint">{t("history.empty")}</p>
        </div>
      ) : (
        <>
          <HistoryEntryList t={t} uiLang={uiLang} h={h} />
          {h.hasMore && (
            <div ref={sentinelRef} className="history-more-sentinel" aria-hidden="true" />
          )}
        </>
      )}
    </div>
  );
}
