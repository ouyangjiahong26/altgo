/** 历史面板的状态与副作用：加载、选择、删除、复制与手动润色。
 * HistoryPanel 组件只负责渲染。 */
import { useCallback, useEffect, useState, type Dispatch, type SetStateAction } from "react";
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

/** 历史面板全部状态与副作用的唯一入口，HistoryPanel 只消费其返回值。 */
export function useHistoryPanel(t: (key: string) => string) {
  const list = useHistoryList();
  const selection = useHistorySelection(list.entries, list.selected, list.setSelected);
  const removal = useHistoryRemoval({
    t,
    entries: list.entries,
    selected: list.selected,
    setSelected: list.setSelected,
    load: list.load,
    setError: list.setError,
  });
  const copy = useHistoryCopy(t, list.setError);
  const polish = useHistoryPolish(t, list.setEntries, list.setError);

  useEffect(() => {
    list.load();
  }, [list.load]);

  useEffect(() => {
    const unlisten = listen("history-updated", () => {
      list.load();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [list.load]);

  return {
    entries: list.entries, loading: list.loading, selected: list.selected,
    copiedId: copy.copiedId, polishingId: polish.polishingId,
    instructionId: polish.instructionId, instructionText: polish.instructionText,
    setInstructionText: polish.setInstructionText, error: list.error,
    allSelected: selection.allSelected, someSelected: selection.someSelected,
    toggleAll: selection.toggleAll, toggleOne: selection.toggleOne,
    handleDeleteSelected: removal.handleDeleteSelected,
    handleClearAll: removal.handleClearAll,
    handleCopy: copy.handleCopy, handlePolish: polish.handlePolish,
    openInstruction: polish.openInstruction, closeInstruction: polish.closeInstruction,
  };
}

/** 条目列表、选中集与错误态：加载历史并保留仍存在条目的选中。 */
function useHistoryList() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

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

  return {
    entries, setEntries, loading, error, setError, load, selected, setSelected,
  };
}

/** 选中态操作：全选切换与单条勾选。 */
function useHistorySelection(
  entries: HistoryEntry[],
  selected: Set<string>,
  setSelected: Dispatch<SetStateAction<Set<string>>>,
) {
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

  return { allSelected, someSelected, toggleAll, toggleOne };
}

interface HistoryRemovalDeps {
  t: (key: string) => string;
  entries: HistoryEntry[];
  selected: Set<string>;
  setSelected: Dispatch<SetStateAction<Set<string>>>;
  load: () => Promise<void>;
  setError: Dispatch<SetStateAction<string | null>>;
}

/** 批量删除与清空：确认后调后端命令并重载列表。 */
function useHistoryRemoval({
  t, entries, selected, setSelected, load, setError,
}: HistoryRemovalDeps) {
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

  return { handleDeleteSelected, handleClearAll };
}

/** 复制操作：优先后端剪贴板，成功后短暂标记对应按钮为已复制。 */
function useHistoryCopy(
  t: (key: string) => string,
  setError: Dispatch<SetStateAction<string | null>>,
) {
  // 记录“已复制”按钮的复合键（`{id}:text` / `{id}:raw`），区分复制润色文本与原始转写。
  const [copiedId, setCopiedId] = useState<string | null>(null);

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

  return { copiedId, handleCopy };
}

/** 读取润色配置：判定手动润色的 API 是否已配齐。 */
function usePolishConfig() {
  const [polishConfig, setPolishConfig] = useState<PolishConfig | null>(null);

  useEffect(() => {
    invoke<PolishConfig>("get_config")
      .then((c) => setPolishConfig(c))
      .catch(() => {});
  }, []);

  // 手动润色固定 medium 档（后端绕过全局 none），此处只需校验 API 已配置。
  return (
    !polishConfig?.polishApiBaseUrl?.trim() ||
    !polishConfig?.polishModel?.trim() ||
    !polishConfig?.hasPolisherApiKey
  );
}

/** 附令润色输入区：记录打开的条目与输入中的指令文本。 */
function usePolishInstruction(setError: Dispatch<SetStateAction<string | null>>) {
  const [instructionId, setInstructionId] = useState<string | null>(null);
  const [instructionText, setInstructionText] = useState("");

  const openInstruction = (id: string) => {
    setError(null);
    setInstructionText("");
    setInstructionId(id);
  };

  const closeInstruction = () => {
    setInstructionId(null);
    setInstructionText("");
  };

  return { instructionId, instructionText, setInstructionText, openInstruction, closeInstruction };
}

/** 手动润色：校验配置后调后端润色命令，成功后更新对应条目。 */
function useHistoryPolish(
  t: (key: string) => string,
  setEntries: Dispatch<SetStateAction<HistoryEntry[]>>,
  setError: Dispatch<SetStateAction<string | null>>,
) {
  const [polishingId, setPolishingId] = useState<string | null>(null);
  const polishConfigMissing = usePolishConfig();
  const instruction = usePolishInstruction(setError);

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
      instruction.closeInstruction();
    } catch (e) {
      setError(String(e));
    } finally {
      setPolishingId(null);
    }
  };

  return {
    polishingId,
    handlePolish,
    instructionId: instruction.instructionId,
    instructionText: instruction.instructionText,
    setInstructionText: instruction.setInstructionText,
    openInstruction: instruction.openInstruction,
    closeInstruction: instruction.closeInstruction,
  };
}
