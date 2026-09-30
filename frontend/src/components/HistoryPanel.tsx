/** 转写历史面板：从独立页面迁为主页内面板。 */
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { resolveUiLang, useTranslation } from "../i18n";
import { copyToClipboard } from "../utils/clipboard";
import {
  Trash2,
  Copy,
  Check,
  FileText,
  Sparkles,
  PenLine,
  Loader2,
  AlertCircle,
} from "lucide-react";

interface PolishConfig {
  polishModel: string;
  polishApiBaseUrl: string;
  hasPolisherApiKey: boolean;
}

interface HistoryEntry {
  id: string;
  createdAtMs: number;
  rawText: string;
  text: string;
}

function formatTime(ms: number, locale: string): string {
  const loc = locale === "en" ? "en-US" : "zh-CN";
  return new Date(ms).toLocaleString(loc, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export default function HistoryPanel() {
  const { t, lang } = useTranslation();
  // lang 可能是空串（auto，跟随系统语言），时间格式必须走解析后的语言。
  const uiLang = resolveUiLang(lang);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [polishingId, setPolishingId] = useState<string | null>(null);
  // 记录“已复制”按钮的复合键（`{id}:text` / `{id}:raw`），区分复制润色文本与原始转写。
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [instructionId, setInstructionId] = useState<string | null>(null);
  const [instructionText, setInstructionText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [polishConfig, setPolishConfig] = useState<PolishConfig | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const list = await invoke<HistoryEntry[]>("list_history");
      setEntries(list);
      setSelected((prev) => {
        const next = new Set<string>();
        for (const id of prev) {
          if (list.some((e) => e.id === id)) {
            next.add(id);
          }
        }
        return next;
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    invoke<PolishConfig>("get_config")
      .then((c) => setPolishConfig(c))
      .catch(() => {});
  }, []);

  useEffect(() => {
    const unlisten = listen("history-updated", () => {
      load();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [load]);

  const allSelected =
    entries.length > 0 && entries.every((e) => selected.has(e.id));
  const someSelected = selected.size > 0;

  const toggleAll = () => {
    if (allSelected) {
      setSelected(new Set());
    } else {
      setSelected(new Set(entries.map((e) => e.id)));
    }
  };

  const toggleOne = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const handleDeleteSelected = async () => {
    if (selected.size === 0) {
      return;
    }
    const ok = window.confirm(t("history.confirm_delete_selected"));
    if (!ok) {
      return;
    }
    setError(null);
    try {
      await invoke("delete_history_entries", { ids: Array.from(selected) });
      setSelected(new Set());
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleClearAll = () => {
    if (entries.length === 0) {
      return;
    }
    const ok = window.confirm(t("history.confirm_clear_all"));
    if (!ok) {
      return;
    }
    void (async () => {
      setError(null);
      try {
        await invoke("clear_history");
        setSelected(new Set());
        await load();
      } catch (e) {
        setError(String(e));
      }
    })();
  };

  const handleCopy = async (key: string, text: string) => {
    setError(null);
    // 优先走后端剪贴板（xclip 等），失败回退 WebView API（copyToClipboard 内置）。
    if (await copyToClipboard(text)) {
      setCopiedId(key);
      window.setTimeout(() => {
        setCopiedId((prev) => (prev === key ? null : prev));
      }, 2000);
    } else {
      setError(t("history.copy_failed"));
    }
  };

  // 手动润色固定 medium 档（后端绕过全局 none），此处只需校验 API 已配置。
  const polishConfigMissing =
    !polishConfig?.polishApiBaseUrl?.trim() ||
    !polishConfig?.polishModel?.trim() ||
    !polishConfig?.hasPolisherApiKey;

  const handlePolish = async (id: string, extraInstruction?: string) => {
    if (polishConfigMissing) {
      setError(t("history.polish_config_missing"));
      return;
    }
    setPolishingId(id);
    setError(null);
    try {
      const updated = await invoke<HistoryEntry>("polish_history_entry", {
        id,
        extraInstruction: extraInstruction ?? "",
      });
      setEntries((prev) =>
        prev.map((e) => (e.id === updated.id ? updated : e)),
      );
      setInstructionId(null);
      setInstructionText("");
    } catch (e) {
      setError(String(e));
    } finally {
      setPolishingId(null);
    }
  };

  const openInstruction = (id: string) => {
    setError(null);
    setInstructionText("");
    setInstructionId(id);
  };

  const closeInstruction = () => {
    setInstructionId(null);
    setInstructionText("");
  };

  return (
    <div className="history-page">
      <div className="history-page-header">
        <h1 className="history-page-title">{t("history.title")}</h1>
        <div className="history-toolbar">
          <label className="history-select-all">
            <input
              type="checkbox"
              checked={allSelected}
              onChange={toggleAll}
              disabled={entries.length === 0 || loading}
            />
            <span>{t("history.select_all")}</span>
          </label>
          <button
            type="button"
            className="history-btn history-btn--danger"
            disabled={!someSelected || loading}
            onClick={() => void handleDeleteSelected()}
          >
            <Trash2 size={16} />
            {t("history.delete_selected")}
          </button>
          <button
            type="button"
            className="history-btn history-btn--danger history-btn--outline"
            disabled={entries.length === 0 || loading}
            onClick={handleClearAll}
          >
            <Trash2 size={16} />
            {t("history.clear_all")}
          </button>
        </div>
      </div>

      {error && (
        <div className="history-error" role="alert">
          <AlertCircle size={18} />
          <span>{error}</span>
        </div>
      )}

      {loading ? (
        <p className="history-muted">{t("history.loading")}</p>
      ) : entries.length === 0 ? (
        <p className="history-empty">{t("history.empty")}</p>
      ) : (
        <ul className="history-list">
          {entries.map((e) => (
            <li key={e.id} className="history-item">
              <label className="history-item-check">
                <input
                  type="checkbox"
                  checked={selected.has(e.id)}
                  onChange={() => toggleOne(e.id)}
                />
              </label>
              <div className="history-item-body">
                <time className="history-item-time" dateTime={new Date(e.createdAtMs).toISOString()}>
                  {formatTime(e.createdAtMs, uiLang)}
                </time>
                <p className="history-item-text">{e.text}</p>
                {e.rawText !== e.text && (
                  <p className="history-item-raw">
                    <span className="history-item-raw-label">{t("history.raw_label")}</span>
                    {e.rawText}
                  </p>
                )}
                <div className="history-item-actions">
                  <button
                    type="button"
                    className={`history-btn history-btn--small ${copiedId === `${e.id}:text` ? "history-btn--copied" : ""}`}
                    onClick={() => void handleCopy(`${e.id}:text`, e.text)}
                    title={t("history.copy")}
                  >
                    {copiedId === `${e.id}:text` ? (
                      <>
                        <Check size={14} />
                        {t("history.copied")}
                      </>
                    ) : (
                      <>
                        <Copy size={14} />
                        {t("history.copy")}
                      </>
                    )}
                  </button>
                  <button
                    type="button"
                    className={`history-btn history-btn--small ${copiedId === `${e.id}:raw` ? "history-btn--copied" : ""}`}
                    onClick={() => void handleCopy(`${e.id}:raw`, e.rawText)}
                    title={t("history.copy_raw")}
                  >
                    {copiedId === `${e.id}:raw` ? (
                      <>
                        <Check size={14} />
                        {t("history.raw_copied")}
                      </>
                    ) : (
                      <>
                        <FileText size={14} />
                        {t("history.copy_raw")}
                      </>
                    )}
                  </button>
                  <button
                    type="button"
                    className="history-btn history-btn--small history-btn--accent"
                    disabled={polishingId === e.id}
                    onClick={() => void handlePolish(e.id)}
                    title={t("history.polish")}
                  >
                    {polishingId === e.id ? (
                      <Loader2 size={14} className="history-spin" />
                    ) : (
                      <Sparkles size={14} />
                    )}
                    {t("history.polish")}
                  </button>
                  <button
                    type="button"
                    className="history-btn history-btn--small"
                    disabled={polishingId === e.id}
                    onClick={() => openInstruction(e.id)}
                    title={t("history.polish_with_instruction")}
                  >
                    <PenLine size={14} />
                    {t("history.polish_with_instruction")}
                  </button>
                </div>
                {instructionId === e.id && (
                  <div className="history-item-instruction">
                    <input
                      type="text"
                      className="history-instruction-input"
                      value={instructionText}
                      onChange={(ev) => setInstructionText(ev.target.value)}
                      onKeyDown={(ev) => {
                        if (ev.key === "Enter" && polishingId !== e.id) {
                          void handlePolish(e.id, instructionText);
                        }
                      }}
                      disabled={polishingId === e.id}
                      placeholder={t("history.instruction_placeholder")}
                    />
                    <button
                      type="button"
                      className="history-btn history-btn--small history-btn--accent"
                      disabled={polishingId === e.id}
                      onClick={() => void handlePolish(e.id, instructionText)}
                    >
                      {polishingId === e.id ? (
                        <Loader2 size={14} className="history-spin" />
                      ) : (
                        t("history.submit")
                      )}
                    </button>
                    <button
                      type="button"
                      className="history-btn history-btn--small"
                      disabled={polishingId === e.id}
                      onClick={closeInstruction}
                    >
                      {t("history.cancel")}
                    </button>
                  </div>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
