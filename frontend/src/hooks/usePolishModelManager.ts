import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { message as showMessageDialog } from "@tauri-apps/plugin-dialog";
import { useTauriEvent } from "./useTauri";

/** 本地润色模型条目（与后端 list_polish_models 的 camelCase 序列化对齐）。 */
export interface PolishModelEntry {
  name: string;
  description: string;
  sizeBytes: number;
  downloaded: boolean;
}

export interface UsePolishModelManagerOptions {
  /** 错误对话框的翻译函数。 */
  t: (key: string) => string;
}

export interface UsePolishModelManagerResult {
  models: PolishModelEntry[];
  downloading: string | null;
  refreshModels: () => void;
  downloadModel: (name: string, onUse: (name: string) => Promise<void>) => Promise<void>;
  deleteModel: (name: string, onDeleted: (name: string) => void) => Promise<void>;
  getDownloadProgress: (name: string) => {
    percent: number;
    connecting: boolean;
  };
}

function reportError(t: (k: string) => string, err: unknown): Promise<void> {
  return showMessageDialog(String(err), {
    title: t("settings.model_error_title"),
    kind: "error",
  }).then(() => undefined);
}

/** 订阅润色模型下载进度事件，形状与转写模型的进度事件一致。 */
function usePolishDownloadProgress(): {
  name: string | null;
  downloaded: number;
  total: number;
} {
  return useTauriEvent("polish-model-download-progress", {
    name: null,
    downloaded: 0,
    total: 0,
  });
}

/**
 * 持有本地润色模型列表与下载生命周期，并追踪下载进度。
 * 同一时刻只允许一个下载在飞：调用方据 downloading 是否非空禁用其他下载按钮。
 */
export function usePolishModelManager({
  t,
}: UsePolishModelManagerOptions): UsePolishModelManagerResult {
  const [models, setModels] = useState<PolishModelEntry[]>([]);
  const [downloading, setDownloading] = useState<string | null>(null);
  const progress = usePolishDownloadProgress();

  const refreshModels = useCallback(() => {
    invoke<PolishModelEntry[]>("list_polish_models")
      .then(setModels)
      .catch((e) => reportError(t, e));
  }, [t]);

  useEffect(() => {
    refreshModels();
  }, [refreshModels]);

  const downloadModel = useCallback(
    async (name: string, onUse: (name: string) => Promise<void>) => {
      setDownloading(name);
      type FinishPayload = { name: string; success: boolean; error?: string };
      let resolveFinished!: (v: { success: boolean; error?: string }) => void;
      const finished = new Promise<{ success: boolean; error?: string }>((resolve) => {
        resolveFinished = resolve;
      });

      const unlisten = await listen<FinishPayload>(
        "polish-model-download-finished",
        (event) => {
          const p = event.payload;
          if (p.name !== name) return;
          resolveFinished({ success: p.success, error: p.error });
        },
      );

      try {
        await invoke("download_polish_model", { name });
        const result = await finished;
        if (!result.success) {
          await reportError(t, result.error ?? "download failed");
          return;
        }
        const updated = await invoke<PolishModelEntry[]>("list_polish_models");
        setModels(updated);
        await onUse(name);
      } catch (e) {
        await reportError(t, e);
      } finally {
        unlisten();
        setDownloading(null);
      }
    },
    [t],
  );

  const deleteModel = useCallback(
    async (name: string, onDeleted: (name: string) => void) => {
      try {
        await invoke("delete_polish_model", { name });
        refreshModels();
        onDeleted(name);
      } catch (e) {
        await reportError(t, e);
      }
    },
    [t, refreshModels],
  );

  const getDownloadProgress = useCallback(
    (name: string) => {
      const meta = models.find((m) => m.name === name);
      const total =
        progress.name === name && progress.total > 0
          ? progress.total
          : meta?.sizeBytes ?? Math.max(progress.total, 1);
      const downloaded = progress.name === name ? progress.downloaded : 0;
      const percent = Math.round((downloaded / Math.max(total, 1)) * 100);
      const connecting =
        progress.name !== name ||
        (progress.name === name && progress.downloaded === 0);
      return { percent, connecting };
    },
    [models, progress],
  );

  return {
    models,
    downloading,
    refreshModels,
    downloadModel,
    deleteModel,
    getDownloadProgress,
  };
}
