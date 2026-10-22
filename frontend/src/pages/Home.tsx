import {
  useStatus,
  useLatestTranscription,
  usePipelineError,
  useKeyProbeArmed,
  useTranscriptionProgress,
  usePendingRecording,
} from "../hooks/useTauri";
import { tError, useTranslation } from "../i18n";
import HistoryPanel from "../components/HistoryPanel";
import {
  Copy,
  Check,
  RotateCcw,
  Trash2,
  Keyboard,
  Mic,
  AudioLines,
  Loader2,
  CheckCircle2,
  TriangleAlert,
} from "lucide-react";
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
  stopped: "var(--color-red)",
} as const;

const statusI18n = {
  idle: "status.idle",
  recording: "status.recording",
  processing: "status.processing",
  done: "status.done",
  stopped: "status.stopped",
} as const;

/** 界面上可呈现的流水线状态。`stopped` 表示监听已停，此时按键不会有任何反应。 */
type HomeStatus = keyof typeof statusColor;

/** 状态块右上角的图标：一种状态只用一个图形，与状态文字互为补充而非重复。 */
const statusIcon = {
  idle: Mic,
  recording: AudioLines,
  processing: Loader2,
  done: CheckCircle2,
  stopped: TriangleAlert,
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
    injected?: boolean | null;
  } | null;
  configuredKeyName: string;
  configuredCode: number | null;
  /** 抓到的键是不是其他程序注入的。 */
  injected: boolean;
  /** 这次按键能不能触发起录。 */
  matched: boolean;
}

/** 自检卡的界面状态。`prompt` 只在首次进主页出现，之后入口收成一行按钮。 */
type ProbeView =
  | { kind: "prompt" }
  | { kind: "closed" }
  | { kind: "arming" }
  | { kind: "probing" }
  | { kind: "matched"; key: string }
  | { kind: "injected"; key: string }
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

/** 自检卡要用的动作回调。 */
interface ProbeHandlers {
  onBegin: () => void;
  onRetry: () => void;
  onClose: () => void;
  onSettings: () => void;
}

interface ProbeCardProps extends ProbeHandlers {
  probe: ProbeCardView;
  triggerLabel: string;
  t: (key: string) => string;
}

/** 自检卡上的一个动作：标签、点击行为，以及是否为该状态的主操作。 */
interface ProbeAction {
  label: string;
  onClick: () => void;
  primary?: boolean;
  icon?: React.ReactNode;
}

/** 自检卡的动作行：同一行小按钮，主操作最多一个。 */
function ProbeActions({ actions }: { actions: ProbeAction[] }) {
  return (
    <div className="home-probe-actions">
      {actions.map((action) => (
        <button
          key={action.label}
          type="button"
          className={`btn btn-sm ${action.primary ? "btn-primary" : "btn-secondary"}`}
          onClick={action.onClick}
        >
          {action.icon}
          <span>{action.label}</span>
        </button>
      ))}
    </div>
  );
}

/** 每个状态的一句话与补充说明。 */
function probeText(
  probe: ProbeCardView,
  triggerLabel: string,
  t: (key: string) => string,
): { head: string; detail: string } {
  switch (probe.kind) {
    case "prompt":
      return { head: t("main.probe_prompt_title"), detail: t("main.probe_prompt_detail") };
    case "arming":
      return {
        head: t("main.probe_arming"),
        detail: t("main.probe_arming_detail"),
      };
    case "probing":
      return {
        head: fill(t("main.probe_prompt"), { key: triggerLabel }),
        detail: t("main.probe_probing_detail"),
      };
    case "matched":
      return {
        head: fill(t("main.probe_matched"), { key: probe.key }),
        detail: t("main.probe_matched_detail"),
      };
    case "injected":
      return {
        head: fill(t("main.probe_injected"), { key: probe.key }),
        detail: t("main.probe_injected_detail"),
      };
    case "mismatch":
      return {
        head: fill(t("main.probe_mismatch"), {
          observed: probe.observedLabel,
          configured: probe.configuredLabel,
        }),
        detail: fill(t("main.probe_mismatch_detail"), { observed: probe.observedLabel }),
      };
    case "timeout":
      return { head: t("main.probe_timeout"), detail: t("main.probe_timeout_detail") };
    case "error":
      return { head: fill(t("main.probe_error"), { message: probe.message }), detail: "" };
  }
}

/** 每个状态的动作，主操作排在最前。 */
function probeActions(
  probe: ProbeCardView,
  t: (key: string) => string,
  handlers: ProbeHandlers,
): ProbeAction[] {
  switch (probe.kind) {
    case "prompt":
      return [
        {
          label: t("main.probe_begin"),
          onClick: handlers.onBegin,
          primary: true,
          icon: <Keyboard size={13} />,
        },
        { label: t("main.probe_skip"), onClick: handlers.onClose },
      ];
    case "arming":
      return [];
    case "probing":
      return [];
    case "matched":
      return [{ label: t("main.probe_close"), onClick: handlers.onClose }];
    case "injected":
      return [
        { label: t("main.probe_retry"), onClick: handlers.onRetry, primary: true },
        { label: t("main.probe_close"), onClick: handlers.onClose },
      ];
    case "mismatch":
      return [
        { label: t("main.probe_settings"), onClick: handlers.onSettings, primary: true },
        { label: t("main.probe_close"), onClick: handlers.onClose },
      ];
    case "timeout":
      return [
        { label: t("main.probe_retry"), onClick: handlers.onRetry, primary: true },
        { label: t("main.probe_settings"), onClick: handlers.onSettings },
        { label: t("main.probe_close"), onClick: handlers.onClose },
      ];
    case "error":
      return [
        { label: t("main.probe_retry"), onClick: handlers.onRetry },
        { label: t("main.probe_close"), onClick: handlers.onClose },
      ];
  }
}

/**
 * 触发键自检卡：把“按下的键能不能被认出来”讲成一句话，并给出下一步。
 *
 * 抓到的键码与配置键码在不一致时一并显示：出问题的机器常常不在开发者手边，
 * 这两行是用户唯一能抄回来的现场数据。
 */
function ProbeCard({ probe, triggerLabel, t, onBegin, onRetry, onClose, onSettings }: ProbeCardProps) {
  const { head, detail } = probeText(probe, triggerLabel, t);
  const actions = probeActions(probe, t, { onBegin, onRetry, onClose, onSettings });
  const unresolved = t("main.probe_code_unresolved");

  return (
    <div className="card home-probe" aria-live="polite">
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
            observed: probe.observedCode === null ? unresolved : codeLabel(probe.observedCode),
            configured:
              probe.configuredCode === null ? unresolved : codeLabel(probe.configuredCode),
          })}
        </p>
      )}
      {actions.length > 0 && <ProbeActions actions={actions} />}
    </div>
  );
}

export default function Home() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const status = useStatus();
  const transcription = useLatestTranscription();
  const error = usePipelineError();
  const pending = usePendingRecording();
  const probeArmed = useKeyProbeArmed();
  const txProgress = useTranscriptionProgress();
  const [copied, setCopied] = useState(false);
  const [triggerLabel, setTriggerLabel] = useState("");
  // 首次进主页把自检卡展开一次，展开即记下：用户没交互就离开，回来也不再展开。
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

  const handleRestartListener = async () => {
    try {
      // 先停后起：监听线程自行退出时控制器的句柄还在，start_pipeline 会报“已在运行”。
      await invoke("restart_listener");
    } catch {
      // 起不来就维持“监听已停止”，用户可到设置页查看配置校验失败的原因。
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

  useEffect(() => {
    if (probe.kind === "prompt") markHotkeyHintSeen();
  }, [probe.kind]);

  // 订阅就绪后才把“准备中”切成“请按下”，注册之前按下的键收不到。
  useEffect(() => {
    setProbe((current) => (current.kind === "arming" ? { kind: "probing" } : current));
  }, [probeArmed]);

  const runProbe = async () => {
    setProbe({ kind: "arming" });
    try {
      const result = await invoke<ActivationKeyProbeResponse>("probe_activation_key");
      const observed = result.observed;
      if (!observed) {
        setProbe({ kind: "timeout" });
        return;
      }
      // 注入的键排在最前：键码对得上也触发起不了录，报成可用会误导用户。
      const key = keyDisplayName(observed.keyName, t);
      if (result.injected) {
        setProbe({ kind: "injected", key });
        return;
      }
      if (result.matched) {
        setProbe({ kind: "matched", key });
        return;
      }
      setProbe({
        kind: "mismatch",
        observedLabel: key,
        configuredLabel: keyDisplayName(result.configuredKeyName, t),
        observedCode: observed.windowsVkCode ?? observed.linuxEvdevCode ?? null,
        configuredCode: result.configuredCode,
      });
    } catch (e) {
      setProbe({ kind: "error", message: String(e) });
    }
  };

  const statusMap: Record<string, HomeStatus> = {
    idle: "idle",
    recording: "recording",
    processing: "processing",
    done: "done",
    stopped: "stopped",
  };

  const mappedStatus = statusMap[status] || "idle";
  const StatusIcon = statusIcon[mappedStatus];

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
              className="btn btn-sm btn-secondary"
              onClick={handleRetryPending}
            >
              <RotateCcw size={13} />
              <span>{t("main.pending_retry")}</span>
            </button>
            <button
              type="button"
              className="btn btn-sm btn-danger"
              onClick={handleDiscardPending}
            >
              <Trash2 size={13} />
              <span>{t("main.pending_discard")}</span>
            </button>
          </div>
        </div>
      )}
      {/* 自检卡常驻在内容流顶部：按键一生效就会进入录音相位，卡片若挂在空态里
          会被整个换掉，用户看不到自检的结论。 */}
      {probe.kind !== "closed" && triggerLabel && (
        <div className="home-probe-area">
          <ProbeCard
            probe={probe}
            triggerLabel={triggerLabel}
            t={t}
            onBegin={() => void runProbe()}
            onRetry={() => void runProbe()}
            onClose={() => setProbe({ kind: "closed" })}
            onSettings={() => navigate("/settings")}
          />
        </div>
      )}
      {!transcription ? (
        <section className="home-state">
          {/* 状态块：统计面范式。标签弱、数值大、右上角一个圆底图标，
              状态色只染图标与数值，不铺底色。 */}
          <div className="stat-card home-state-card">
            <div className="stat-card-head">
              {/* 状态块是这张卡片的标题位，用一条独立短标签，不用产品副标题凑。 */}
              <span className="stat-card-label">{t("main.status_label")}</span>
              <span
                className={`stat-card-icon home-state-icon${
                  mappedStatus === "recording" ? " is-live" : ""
                }`}
                style={{ color: statusColor[mappedStatus] }}
                aria-hidden="true"
              >
                <StatusIcon
                  size={13}
                  className={mappedStatus === "processing" ? "home-spin" : undefined}
                />
              </span>
            </div>
            <div
              className="stat-card-value home-status-text"
              style={{ color: statusColor[mappedStatus] }}
            >
              {t(statusI18n[mappedStatus])}
            </div>
            {mappedStatus === "idle" && triggerLabel && (
              <p className="stat-card-hint home-trigger-hint">
                {renderTriggerPrompt(t("main.trigger_prompt"), triggerLabel)}
              </p>
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
          </div>
          {/* 空态的那个动作：检测触发键是备选动作，用描边按钮，不与状态块抢视线 */}
          {mappedStatus === "idle" && triggerLabel && probe.kind === "closed" && (
            <div className="home-probe-area">
              <button
                type="button"
                className="btn btn-outline"
                onClick={() => void runProbe()}
              >
                <Keyboard size={15} />
                <span>{t("main.probe_open")}</span>
              </button>
            </div>
          )}
          {/* 监听已停止时自检必然收不到按键（旁路没有生产方），这里给的是恢复入口 */}
          {mappedStatus === "stopped" && (
            <div className="home-probe-area">
              <button
                type="button"
                className="btn btn-primary"
                onClick={handleRestartListener}
              >
                <RotateCcw size={15} />
                <span>{t("main.restart_listener")}</span>
              </button>
            </div>
          )}
        </section>
      ) : (
        <section className="home-result">
          <div className="card home-result-card">
            <div className="home-result-head">
              <span className="home-result-label">{t("main.result_label")}</span>
              <span className="home-result-meta">{t("main.result_meta")}</span>
            </div>
            <p className="home-result-text">{transcription}</p>
            <div className="home-result-foot">
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
        </section>
      )}
      <HistoryPanel />
    </div>
  );
}
