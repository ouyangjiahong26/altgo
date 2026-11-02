/** 历史条目的候选术语面板状态：读取配置、请求候选、手动补充与写入术语表。
 *  面板只消费这里的状态与回调，不直接调 IPC。 */
import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  glossarySkipMessage,
  mergeGlossaryTerms,
  splitGlossaryInput,
  type SkippedGlossaryTerm,
} from "../../utils/glossary";

export interface GlossaryCandidateInfo {
  term: string;
  fragment: string;
}

interface CandidateConfig {
  polishBackend?: string;
  polishApiBaseUrl?: string;
  polishModel?: string;
  hasPolisherApiKey?: boolean;
  polishGlossary?: string[];
}

export interface GlossaryCandidateController {
  openId: string | null;
  loading: boolean;
  remoteReady: boolean;
  glossaryLoaded: boolean;
  error: string | null;
  candidates: GlossaryCandidateInfo[];
  selected: Set<string>;
  manualTerms: string[];
  manualInput: string;
  manualError: string;
  adding: boolean;
  addedCount: number;
  skipped: SkippedGlossaryTerm[];
  glossaryCount: number;
  open: (id: string) => void;
  close: () => void;
  toggle: (term: string) => void;
  setManualInput: (value: string) => void;
  addManual: (text?: string) => void;
  removeManual: (term: string) => void;
  add: () => void;
}

/** 候选筛选只允许远端润色后端。本地后端是用户对文本不外发的选择，
 *  即使还留着远端地址与密钥也不发请求。 */
function isRemoteReady(config: CandidateConfig): boolean {
  const backend = (config.polishBackend || "remote").trim().toLowerCase();
  return (
    backend === "remote" &&
    Boolean((config.polishApiBaseUrl ?? "").trim()) &&
    Boolean((config.polishModel ?? "").trim()) &&
    Boolean(config.hasPolisherApiKey)
  );
}

export function useGlossaryCandidates(
  t: (key: string) => string,
): GlossaryCandidateController {
  const [openId, setOpenId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [remoteReady, setRemoteReady] = useState(false);
  const [glossaryLoaded, setGlossaryLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<GlossaryCandidateInfo[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [manualTerms, setManualTerms] = useState<string[]>([]);
  const [manualInput, setManualInputValue] = useState("");
  const [manualError, setManualError] = useState("");
  const [adding, setAdding] = useState(false);
  const [addedCount, setAddedCount] = useState(0);
  const [skipped, setSkipped] = useState<SkippedGlossaryTerm[]>([]);
  const [glossary, setGlossary] = useState<string[]>([]);

  // 关闭或换条目后，仍在途的请求结果不能再落到面板上。
  const requestSeq = useRef(0);

  const reset = () => {
    setLoading(false);
    setRemoteReady(false);
    setGlossaryLoaded(false);
    setError(null);
    setCandidates([]);
    setSelected(new Set());
    setManualTerms([]);
    setManualInputValue("");
    setManualError("");
    setAdding(false);
    setAddedCount(0);
    setSkipped([]);
    setGlossary([]);
  };

  const close = () => {
    requestSeq.current += 1;
    setOpenId(null);
    reset();
  };

  const open = (id: string) => {
    const seq = ++requestSeq.current;
    setOpenId(id);
    setLoading(true);
    setRemoteReady(false);
    setGlossaryLoaded(false);
    setError(null);
    setCandidates([]);
    setSelected(new Set());
    setManualTerms([]);
    setManualInputValue("");
    setManualError("");
    setAdding(false);
    setAddedCount(0);
    setSkipped([]);
    setGlossary([]);

    void (async () => {
      try {
        const config = await invoke<CandidateConfig>("get_config");
        if (seq !== requestSeq.current) return;

        const terms = Array.isArray(config.polishGlossary)
          ? config.polishGlossary
              .filter((item): item is string => typeof item === "string")
              .map((item) => item.trim())
              .filter(Boolean)
          : [];
        setGlossary(terms);
        setGlossaryLoaded(true);

        const ready = isRemoteReady(config);
        setRemoteReady(ready);
        if (!ready) return;

        try {
          const list = await invoke<GlossaryCandidateInfo[]>(
            "suggest_glossary_terms",
            { historyId: id },
          );
          if (seq !== requestSeq.current) return;
          setCandidates(list);
        } catch (requestError) {
          if (seq !== requestSeq.current) return;
          setError(String(requestError));
        }
      } catch (configError) {
        if (seq !== requestSeq.current) return;
        setError(String(configError));
      } finally {
        if (seq === requestSeq.current) setLoading(false);
      }
    })();
  };

  const toggle = (term: string) => {
    if (adding) return;
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(term)) {
        next.delete(term);
      } else {
        next.add(term);
      }
      return next;
    });
    setError(null);
    setSkipped([]);
    setAddedCount(0);
  };

  const updateManualInput = (value: string) => {
    if (adding) return;
    setManualInputValue(value);
    setManualError("");
    setSkipped([]);
    setAddedCount(0);
  };

  const addManual = (text?: string) => {
    if (adding) return;
    const incoming = splitGlossaryInput(text ?? manualInput);
    if (incoming.length === 0) return;

    const selectedTerms = candidates
      .filter((candidate) => selected.has(candidate.term))
      .map((candidate) => candidate.term);
    const result = mergeGlossaryTerms(
      [...glossary, ...manualTerms, ...selectedTerms],
      incoming,
    );
    if (result.added.length > 0) {
      setManualTerms((prev) => [...prev, ...result.added]);
      setManualInputValue("");
    }
    setManualError(
      result.skipped[0]
        ? glossarySkipMessage(result.skipped[0].reason, t)
        : "",
    );
    setSkipped([]);
    setAddedCount(0);
  };

  const removeManual = (term: string) => {
    if (adding) return;
    setManualTerms((prev) => prev.filter((item) => item !== term));
    setManualError("");
    setSkipped([]);
    setAddedCount(0);
  };

  const add = () => {
    if (!glossaryLoaded || adding) return;
    const seq = requestSeq.current;
    const selectedTerms = candidates
      .filter((candidate) => selected.has(candidate.term))
      .map((candidate) => candidate.term);
    const incoming = [...selectedTerms, ...manualTerms];
    if (incoming.length === 0) return;

    const result = mergeGlossaryTerms(glossary, incoming);
    setSkipped(result.skipped);
    setAddedCount(0);
    if (result.added.length === 0) return;

    setAdding(true);
    setError(null);
    void (async () => {
      try {
        await invoke("save_config", {
          patch: { polishGlossary: result.terms },
        });
        // 面板在保存期间被关闭或换条目时，磁盘已经写好，但不要用旧结果
        // 覆盖新面板的状态。下次打开会重新读取配置。
        if (seq !== requestSeq.current) return;
        const added = new Set(result.added);
        setGlossary(result.terms);
        setCandidates((prev) =>
          prev.filter((candidate) => !added.has(candidate.term)),
        );
        setSelected(new Set());
        setManualTerms([]);
        setManualInputValue("");
        setManualError("");
        setAddedCount(result.added.length);
      } catch (saveError) {
        if (seq === requestSeq.current) setError(String(saveError));
      } finally {
        if (seq === requestSeq.current) setAdding(false);
      }
    })();
  };

  return {
    openId,
    loading,
    remoteReady,
    glossaryLoaded,
    error,
    candidates,
    selected,
    manualTerms,
    manualInput,
    manualError,
    adding,
    addedCount,
    skipped,
    glossaryCount: glossary.length,
    open,
    close,
    toggle,
    setManualInput: updateManualInput,
    addManual,
    removeManual,
    add,
  };
}
