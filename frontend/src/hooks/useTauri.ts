import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 订阅一个 Tauri 事件，返回最新载荷（尚无事件时返回 `initial`）。
 *
 * 回调在 effect 闭包中被捕获，因此改动它不会重新订阅。
 * 用返回的 `payload` 驱动 UI；若需要派生转换，请在调用方完成。
 */
export function useTauriEvent<T>(
  event: string,
  initial: T,
  callback?: EventCallback<T>,
): T {
  const [state, setState] = useState<T>(initial);

  useEffect(() => {
    let active = true;
    const unlistenPromise: Promise<UnlistenFn> = listen<T>(event, (event) => {
      if (!active) return;
      setState(event.payload);
      callback?.(event);
    });
    return () => {
      active = false;
      unlistenPromise.then((fn) => fn());
    };
    // callback 特意不作为依赖项；需要的消费方可自行 memo。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [event]);

  return state;
}

export function useStatus(): string {
  return useTauriEvent<string>("pipeline-status", "idle");
}

export function useLatestTranscription(): string | null {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    let timer: number | null = null;
    const unlisten = listen<string>("transcription-result", (event) => {
      setText(event.payload);
      if (timer !== null) {
        clearTimeout(timer);
      }
      timer = window.setTimeout(() => setText(null), 5000);
    });
    return () => {
      unlisten.then((fn) => fn());
      if (timer !== null) {
        clearTimeout(timer);
      }
    };
  }, []);
  return text;
}

/**
 * 事件通道传来的错误码结构：`code` 对应前端字典 `error.<code>` 词条，
 * `params` 供模板占位符插值（与 Rust `UserFacingError` 的 camelCase 序列化对齐）。
 */
export interface PipelineErrorPayload {
  code: string;
  params?: Record<string, string>;
}

export function usePipelineError(): PipelineErrorPayload | null {
  return useTauriEvent<PipelineErrorPayload | null>("pipeline-error", null);
}

export function useKeyListenerBackend(): string | null {
  return useTauriEvent<string | null>("key-listener-backend", null);
}

export function useTranscriptionProgress(): {
  phase: string;
  fraction: number | null;
} | null {
  const [progress, setProgress] = useState<{
    phase: string;
    fraction: number | null;
  } | null>(null);
  useEffect(() => {
    let active = true;
    const unlistenProgress = listen<{
      phase: string;
      fraction: number | null;
    }>("transcription-progress", (event) => {
      if (active) setProgress(event.payload);
    });
    const unlistenStatus = listen<string>("pipeline-status", (event) => {
      if (!active) return;
      if (event.payload !== "processing") {
        setProgress(null);
      }
    });
    return () => {
      active = false;
      unlistenProgress.then((fn) => fn());
      unlistenStatus.then((fn) => fn());
    };
  }, []);
  return progress;
}

/** 待重试录音元信息：时长与上次失败原因（音频本体留在 Rust 侧内存）。 */
export interface PendingRecordingInfo {
  durationMs: number;
  error: PipelineErrorPayload;
}

/**
 * 订阅待重试录音槽位：挂载时查一次初始状态，之后跟随
 * `pending-recording-changed` 事件（保留为对象，清空为 null）。
 */
export function usePendingRecording(): PendingRecordingInfo | null {
  const [pending, setPending] = useState<PendingRecordingInfo | null>(null);

  useEffect(() => {
    let active = true;

    invoke<PendingRecordingInfo | null>("get_pending_recording")
      .then((value) => {
        if (active) setPending(value);
      })
      .catch(() => {
        // 查询失败按无待重试处理，不打扰用户。
      });

    const unlisten = listen<PendingRecordingInfo | null>(
      "pending-recording-changed",
      (event) => {
        if (active) setPending(event.payload);
      },
    );

    return () => {
      active = false;
      unlisten.then((fn) => fn());
    };
  }, []);

  return pending;
}

export function useModelDownloadProgress(): {
  name: string | null;
  downloaded: number;
  total: number;
} {
  return useTauriEvent("model-download-progress", {
    name: null,
    downloaded: 0,
    total: 0,
  });
}
