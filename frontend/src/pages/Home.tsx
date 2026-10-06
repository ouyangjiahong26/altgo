import {
  useStatus,
  useLatestTranscription,
  usePipelineError,
  useKeyListenerBackend,
  useTranscriptionProgress,
  usePendingRecording,
} from "../hooks/useTauri";
import { tError, useTranslation } from "../i18n";
import HistoryPanel from "../components/HistoryPanel";
import { Copy, Check, RotateCcw, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { copyToClipboard } from "../utils/clipboard";
import { KEY_PRESETS } from "../config/keyPresets";

const statusColor = {
  idle: "var(--color-text-muted)",
  recording: "var(--color-red)",
  processing: "var(--color-amber)",
  done: "var(--color-green)",
} as const;

const statusI18n = {
  idle: "status.idle",
  recording: "status.recording",
  processing: "status.processing",
  done: "status.done",
} as const;

/** 把提示文案里的 {key} 占位换成键位标记，键名来自配置，按文本节点渲染。 */
function renderTriggerPrompt(template: string, keyLabel: string) {
  const [before, after = ""] = template.split("{key}");
  return (
    <>
      {before}
      <span className="kbd">{keyLabel}</span>
      {after}
    </>
  );
}

export default function Home() {
  const { t } = useTranslation();
  const status = useStatus();
  const transcription = useLatestTranscription();
  const error = usePipelineError();
  const keyBackend = useKeyListenerBackend();
  const txProgress = useTranscriptionProgress();
  const pending = usePendingRecording();
  const [copied, setCopied] = useState(false);
  const [triggerLabel, setTriggerLabel] = useState("");

  const handleRetryPending = async () => {
    try {
      await invoke("retry_pending_transcription");
    } catch {
      // 重试请求失败（流水线未运行等）：横幅保留，用户可稍后再试。
    }
  };

  const handleDiscardPending = async () => {
    try {
      await invoke("discard_pending_recording");
    } catch {
      // 放弃失败不影响主流程，事件未到时横幅暂留。
    }
  };

  const handleCopy = async () => {
    if (transcription) {
      const ok = await copyToClipboard(transcription);
      if (ok) {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      }
    }
  };

  // 空态要说清“按住哪个键”，这里取一次配置，失败就退回默认预设文案。
  useEffect(() => {
    invoke<{ keyName?: string }>("get_config")
      .then((cfg) => {
        const keyName = cfg?.keyName ?? "Alt_R";
        const preset = KEY_PRESETS.find((p) => p.value === keyName);
        setTriggerLabel(preset ? t(preset.labelKey) : keyName);
      })
      .catch(() => setTriggerLabel(t(KEY_PRESETS[0].labelKey)));
    // t 随语言切换变化，切换语言时需要重新取标签文案
  }, [t]);

  const statusMap: Record<string, "idle" | "recording" | "processing" | "done"> = {
    idle: "idle",
    recording: "recording",
    processing: "processing",
    done: "done",
  };

  const mappedStatus = statusMap[status] || "idle";

  return (
    <div className="home">
      {error && (
        <div className="home-error">
          <span className="error-icon">⚠</span>
          <p className="error-text">{tError(error.code, error.params)}</p>
        </div>
      )}
      {pending && (
        <div className="home-pending" role="status">
          <div className="home-pending-text">
            <span className="home-pending-title">{t("main.pending_title")}</span>
            <span className="home-pending-meta">
              {t("main.pending_duration_seconds").replace(
                "{seconds}",
                String(Math.max(1, Math.round(pending.durationMs / 1000))),
              )}
              {" · "}
              {tError(pending.error.code, pending.error.params)}
            </span>
          </div>
          <div className="home-pending-actions">
            <button
              type="button"
              className="btn btn-sm home-pending-retry"
              onClick={handleRetryPending}
            >
              <RotateCcw size={13} />
              <span>{t("main.pending_retry")}</span>
            </button>
            <button
              type="button"
              className="btn btn-sm btn-secondary"
              onClick={handleDiscardPending}
            >
              <Trash2 size={13} />
              <span>{t("main.pending_discard")}</span>
            </button>
          </div>
        </div>
      )}
      {!transcription ? (
        <div className="home-idle">
          <div className="home-status-line">
            <span
              className={`home-status-dot ${mappedStatus === "recording" ? "breathing" : ""}`}
              style={{ background: statusColor[mappedStatus] }}
            />
            <span className="home-status-text" style={{ color: statusColor[mappedStatus] }}>
              {t(statusI18n[mappedStatus])}
            </span>
          </div>
          {mappedStatus === "idle" && triggerLabel && (
            <p className="home-trigger-hint">{renderTriggerPrompt(t("main.trigger_prompt"), triggerLabel)}</p>
          )}
          {mappedStatus === "processing" && (
            <div className="home-tx-progress-wrap">
              <span className="home-tx-progress-phase">
                {txProgress?.phase === "polish"
                  ? t("overlay.polishing")
                  : t("overlay.transcribing")}
              </span>
              <div className="home-tx-progress-bar">
                <div
                  className={`home-tx-progress-fill ${
                    txProgress?.fraction == null ? "indeterminate" : ""
                  }`}
                  style={
                    txProgress?.fraction != null
                      ? {
                          width: `${Math.min(100, Math.max(0, txProgress.fraction * 100))}%`,
                        }
                      : undefined
                  }
                />
              </div>
            </div>
          )}
          {keyBackend && (
            <p className="home-key-backend">{t(`main.key_backend_${keyBackend}`)}</p>
          )}
        </div>
      ) : (
        <div className="home-result">
          <span className="home-result-label">{t("main.result_label")}</span>
          <div className="home-result-card">
            <p className="home-result-text">{transcription}</p>
            <div className="home-result-foot">
              <span className="home-result-meta">{t("main.result_meta")}</span>
              <button
                className={`btn btn-sm btn-secondary home-copy-btn ${copied ? "copied" : ""}`}
                onClick={handleCopy}
              >
                {copied ? (
                  <>
                    <Check size={13} />
                    <span>{t("overlay.copied")}</span>
                  </>
                ) : (
                  <>
                    <Copy size={13} />
                    <span>{t("main.copy")}</span>
                  </>
                )}
              </button>
            </div>
          </div>
        </div>
      )}
      <HistoryPanel />
    </div>
  );
}
