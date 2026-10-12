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
import { Copy, Check, RotateCcw, Trash2, Keyboard } from "lucide-react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { copyToClipboard } from "../utils/clipboard";
import { KEY_PRESETS } from "../config/keyPresets";
import { hasSeenHotkeyHint, markHotkeyHintSeen } from "../onboarding";

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

/** 触发键名的显示文案：预设键名走 i18n，其余（自定义键名、键码兜底名）原样显示。 */
function keyDisplayName(name: string, t: (key: string) => string): string {
  const preset = KEY_PRESETS.find((p) => p.value === name);
  return preset ? t(preset.labelKey) : name;
}

/** 模板里的 {name} 占位逐项替换，用于带多个变量的文案。 */
function fill(template: string, vars: Record<string, string>): string {
  return Object.entries(vars).reduce(
    (text, [name, value]) => text.split(`{${name}}`).join(value),
    template,
  );
}

/** 键码按十六进制显示（如 0xA4），与“按下以设置”捕获到的原始码同形。 */
function codeLabel(code: number): string {
  return `0x${code.toString(16).toUpperCase().padStart(2, "0")}`;
}

/** 自检命令的返回。`observed` 为 null 表示等待期间没有收到任何按键。 */
interface ActivationKeyProbeResponse {
  observed: {
    keyName: string;
    linuxEvdevCode?: number | null;
    windowsVkCode?: number | null;
  } | null;
  configuredKeyName: string;
  configuredCode: number | null;
  matched: boolean;
}

/** 自检卡的界面状态。`prompt` 只在首次进主页出现，之后入口收成一行按钮。 */
type ProbeView =
  | { kind: "prompt" }
  | { kind: "closed" }
  | { kind: "probing" }
  | { kind: "matched"; key: string }
  | {
      kind: "mismatch";
      observedLabel: string;
      configuredLabel: string;
      observedCode: number | null;
      configuredCode: number | null;
    }
  | { kind: "timeout" }
  | { kind: "error"; message: string };

/** 展开态的自检卡拿不到 `closed`，用排除后的联合让渲染分支可穷尽。 */
type ProbeCardView = Exclude<ProbeView, { kind: "closed" }>;

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

interface ProbeCardProps {
  probe: ProbeCardView;
  triggerLabel: string;
  t: (key: string) => string;
  onBegin: () => void;
  onRetry: () => void;
  onClose: () => void;
  onSettings: () => void;
}

/**
 * 触发键自检卡：把“按下的键能不能被认出来”讲成一句话，并给出下一步。
 *
 * 抓到的键码与配置键码在不一致时一并显示：出问题的机器常常不在开发者手边，
 * 这两行是用户唯一能抄回来的现场数据。
 */
function ProbeCard({
  probe,
  triggerLabel,
  t,
  onBegin,
  onRetry,
  onClose,
  onSettings,
}: ProbeCardProps) {
  let head = "";
  let detail = "";
  let actions: React.ReactNode = null;

  switch (probe.kind) {
    case "prompt":
      head = t("main.probe_prompt_title");
      detail = t("main.probe_prompt_detail");
      actions = (
        <>
          <button type="button" className="btn btn-sm btn-primary" onClick={onBegin}>
            <Keyboard size={13} />
            <span>{t("main.probe_begin")}</span>
          </button>
          <button type="button" className="btn btn-sm btn-secondary" onClick={onClose}>
            <span>{t("main.probe_skip")}</span>
          </button>
        </>
      );
      break;
    case "probing":
      head = fill(t("main.probe_prompt"), { key: triggerLabel });
      detail = t("main.probe_probing_detail");
      break;
    case "matched":
      head = fill(t("main.probe_matched"), { key: probe.key });
      detail = t("main.probe_matched_detail");
      actions = (
        <button type="button" className="btn btn-sm btn-secondary" onClick={onClose}>
          <span>{t("main.probe_close")}</span>
        </button>
      );
      break;
    case "mismatch":
      head = fill(t("main.probe_mismatch"), {
        observed: probe.observedLabel,
        configured: probe.configuredLabel,
      });
      detail = fill(t("main.probe_mismatch_detail"), { observed: probe.observedLabel });
      actions = (
        <>
          <button type="button" className="btn btn-sm btn-primary" onClick={onSettings}>
            <span>{t("main.probe_settings")}</span>
          </button>
          <button type="button" className="btn btn-sm btn-secondary" onClick={onClose}>
            <span>{t("main.probe_close")}</span>
          </button>
        </>
      );
      break;
    case "timeout":
      head = t("main.probe_timeout");
      detail = t("main.probe_timeout_detail");
      actions = (
        <>
          <button type="button" className="btn btn-sm btn-secondary" onClick={onRetry}>
            <span>{t("main.probe_retry")}</span>
          </button>
          <button type="button" className="btn btn-sm btn-secondary" onClick={onClose}>
            <span>{t("main.probe_close")}</span>
          </button>
        </>
      );
      break;
    case "error":
      head = fill(t("main.probe_error"), { message: probe.message });
      actions = (
        <>
          <button type="button" className="btn btn-sm btn-secondary" onClick={onRetry}>
            <span>{t("main.probe_retry")}</span>
          </button>
          <button type="button" className="btn btn-sm btn-secondary" onClick={onClose}>
            <span>{t("main.probe_close")}</span>
          </button>
        </>
      );
      break;
  }

  const unresolved = t("main.probe_code_unresolved");

  return (
    <div className="home-probe" aria-live="polite">
      <div className="home-probe-head">
        {probe.kind === "probing" && (
          <span className="home-status-dot breathing" style={{ background: "var(--color-amber)" }} />
        )}
        <span>{head}</span>
      </div>
      {detail && <p className="home-probe-detail">{detail}</p>}
      {probe.kind === "mismatch" && (
        <p className="home-probe-diagnostic">
          {fill(t("main.probe_diagnostic"), {
            observed:
              probe.observedCode === null ? unresolved : codeLabel(probe.observedCode),
            configured:
              probe.configuredCode === null ? unresolved : codeLabel(probe.configuredCode),
          })}
        </p>
      )}
      {actions && <div className="home-probe-actions">{actions}</div>}
    </div>
  );
}

export default function Home() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const status = useStatus();
  const transcription = useLatestTranscription();
  const error = usePipelineError();
  const keyBackend = useKeyListenerBackend();
  const txProgress = useTranscriptionProgress();
  const pending = usePendingRecording();
  const [copied, setCopied] = useState(false);
  const [triggerLabel, setTriggerLabel] = useState("");
  // 首次进主页把自检卡展开一次，用户看过之后入口收成一行按钮。
  const [probe, setProbe] = useState<ProbeView>(() =>
    hasSeenHotkeyHint() ? { kind: "closed" } : { kind: "prompt" },
  );

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
        setTriggerLabel(keyDisplayName(keyName, t));
      })
      .catch(() => setTriggerLabel(t(KEY_PRESETS[0].labelKey)));
    // t 随语言切换变化，切换语言时需要重新取标签文案
  }, [t]);

  const runProbe = async () => {
    setProbe({ kind: "probing" });
    try {
      const result = await invoke<ActivationKeyProbeResponse>("probe_activation_key");
      const observed = result.observed;
      if (!observed) {
        setProbe({ kind: "timeout" });
        return;
      }
      if (result.matched) {
        setProbe({ kind: "matched", key: keyDisplayName(observed.keyName, t) });
        return;
      }
      setProbe({
        kind: "mismatch",
        observedLabel: keyDisplayName(observed.keyName, t),
        configuredLabel: keyDisplayName(result.configuredKeyName, t),
        observedCode: observed.windowsVkCode ?? observed.linuxEvdevCode ?? null,
        configuredCode: result.configuredCode,
      });
    } catch (e) {
      setProbe({ kind: "error", message: String(e) });
    }
  };

  const beginProbe = () => {
    markHotkeyHintSeen();
    void runProbe();
  };

  const closeProbe = () => {
    markHotkeyHintSeen();
    setProbe({ kind: "closed" });
  };

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
          {mappedStatus === "idle" && triggerLabel && (
            <div className="home-probe-area">
              {probe.kind === "closed" ? (
                <button
                  type="button"
                  className="btn btn-sm btn-secondary home-probe-open"
                  onClick={beginProbe}
                >
                  <Keyboard size={13} />
                  <span>{t("main.probe_open")}</span>
                </button>
              ) : (
                <ProbeCard
                  probe={probe}
                  triggerLabel={triggerLabel}
                  t={t}
                  onBegin={beginProbe}
                  onRetry={() => void runProbe()}
                  onClose={closeProbe}
                  onSettings={() => navigate("/settings")}
                />
              )}
            </div>
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
