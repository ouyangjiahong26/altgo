/** 历史面板的状态与副作用：加载、选择、删除、复制与手动润色。
 * HistoryPanel 组件只负责渲染。 */
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { copyToClipboard } from "../../utils/clipboard";

export interface HistoryEntry {
  id: string;
  createdAtMs: number;
  rawText: string;
  text: string;
}

interface PolishConfig {
  polishModel: string;
  polishApiBaseUrl: string;
  hasPolisherApiKey: boolean;
}

export function useHistoryPanel(t: (key: string) => string) {
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

  return {
    entries,
    loading,
    selected,
    copiedId,
    polishingId,
    instructionId,
    instructionText,
    setInstructionText,
    error,
    allSelected,
    someSelected,
    toggleAll,
    toggleOne,
    handleDeleteSelected,
    handleClearAll,
    handleCopy,
    handlePolish,
    openInstruction,
    closeInstruction,
  };
}
