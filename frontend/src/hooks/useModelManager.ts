import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { message as showMessageDialog } from "@tauri-apps/plugin-dialog";
import { useModelDownloadProgress } from "./useTauri";

export interface ModelEntry {
  name: string;
  filename: string;
  sizeBytes: number;
  description: string;
  downloaded: boolean;
}

export interface UseModelManagerOptions {
  /** Translation function for error dialogs. */
  t: (key: string) => string;
}

export interface UseModelManagerResult {
  models: ModelEntry[];
  downloading: string | null;
  refreshModels: () => void;
  downloadAndUse: (name: string, onUse: (name: string) => Promise<void>) => Promise<void>;
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

/**
 * Owns model listing and the download lifecycle with progress tracking.
 */
export function useModelManager({ t }: UseModelManagerOptions): UseModelManagerResult {
  const [models, setModels] = useState<ModelEntry[]>([]);
  const [downloading, setDownloading] = useState<string | null>(null);
  const progress = useModelDownloadProgress();

  const refreshModels = useCallback(() => {
    invoke<ModelEntry[]>("list_models").then(setModels).catch((e) => reportError(t, e));
  }, [t]);

  useEffect(() => {
    refreshModels();
  }, [refreshModels]);

  const downloadAndUse = useCallback(
    async (name: string, onUse: (name: string) => Promise<void>) => {
      setDownloading(name);
      type FinishPayload = {
        name: string;
        success: boolean;
        path?: string;
        error?: string;
      };
      let resolveFinished!: (v: {
        success: boolean;
        path?: string;
        error?: string;
      }) => void;
      const finished = new Promise<{
        success: boolean;
        path?: string;
        error?: string;
      }>((resolve) => {
        resolveFinished = resolve;
      });

      const unlisten = await listen<FinishPayload>("model-download-finished", (event) => {
        const p = event.payload;
        if (p.name !== name) return;
        resolveFinished({ success: p.success, path: p.path, error: p.error });
      });

      try {
        await invoke("download_model", { name });
        const result = await finished;
        if (!result.success) {
          await reportError(t, result.error ?? "download failed");
          return;
        }
        const updated = await invoke<ModelEntry[]>("list_models");
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
        await invoke("delete_model", { name });
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
    downloadAndUse,
    deleteModel,
    getDownloadProgress,
  };
}
