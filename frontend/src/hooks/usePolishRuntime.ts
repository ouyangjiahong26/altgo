import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { message as showMessageDialog } from "@tauri-apps/plugin-dialog";
import { useTauriEvent } from "./useTauri";

/** 本地润色运行时状态（与后端 polish_runtime_status 的 camelCase 序列化对齐）。 */
export interface PolishRuntimeStatus {
  supported: boolean;
  installed: boolean;
  running: boolean;
  build: string;
  archiveSize: number;
}

export interface UsePolishRuntimeOptions {
  /** 错误对话框的翻译函数。 */
  t: (key: string) => string;
}

export interface UsePolishRuntimeResult {
  status: PolishRuntimeStatus | null;
  downloading: boolean;
  progress: { downloaded: number; total: number };
  refreshStatus: () => void;
  downloadRuntime: () => Promise<void>;
  /** 重新下载：先移除已装运行时，成功后再走一次下载。 */
  redownload: () => Promise<void>;
  removeRuntime: () => Promise<void>;
}

function reportError(t: (k: string) => string, err: unknown): Promise<void> {
  return showMessageDialog(String(err), {
    title: t("settings.model_error_title"),
    kind: "error",
  }).then(() => undefined);
}

/**
 * 持有本地润色运行时的安装状态与下载生命周期。
 * 状态在挂载时查询一次，下载或移除成功后重新查询。
 */
export function usePolishRuntime({
  t,
}: UsePolishRuntimeOptions): UsePolishRuntimeResult {
  const [status, setStatus] = useState<PolishRuntimeStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const progress = useTauriEvent("polish-runtime-download-progress", {
    downloaded: 0,
    total: 0,
  });

  const refreshStatus = useCallback(() => {
    invoke<PolishRuntimeStatus>("polish_runtime_status")
      .then(setStatus)
      .catch((e) => reportError(t, e));
  }, [t]);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const downloadRuntime = useCallback(async () => {
    setDownloading(true);
    type FinishPayload = { success: boolean; error?: string };
    let resolveFinished!: (v: FinishPayload) => void;
    const finished = new Promise<FinishPayload>((resolve) => {
      resolveFinished = resolve;
    });

    const unlisten = await listen<FinishPayload>(
      "polish-runtime-download-finished",
      (event) => resolveFinished(event.payload),
    );

    try {
      await invoke("download_polish_runtime");
      const result = await finished;
      if (!result.success) {
        await reportError(t, result.error ?? "download failed");
        return;
      }
      refreshStatus();
    } catch (e) {
      await reportError(t, e);
    } finally {
      unlisten();
      setDownloading(false);
    }
  }, [t, refreshStatus]);

  const removeRuntime = useCallback(async () => {
    try {
      await invoke("remove_polish_runtime");
      refreshStatus();
    } catch (e) {
      await reportError(t, e);
    }
  }, [t, refreshStatus]);

  // 后端 download_polish_runtime 幂等：文件在位时直接返回，不重新拉取。
  // 因此重新下载必须先真正移除，移除失败就中止，避免按钮点了没反应。
  const redownload = useCallback(async () => {
    try {
      await invoke("remove_polish_runtime");
    } catch (e) {
      await reportError(t, e);
      return;
    }
    await downloadRuntime();
  }, [t, downloadRuntime]);

  return {
    status,
    downloading,
    progress,
    refreshStatus,
    downloadRuntime,
    redownload,
    removeRuntime,
  };
}
