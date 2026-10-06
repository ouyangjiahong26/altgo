import { createRoot } from "react-dom/client";
import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { X, Check, TriangleAlert } from "lucide-react";
import { tError, useTranslation } from "./i18n";
import type { PipelineErrorPayload } from "./hooks/useTauri";
import { applyThemeToDocument, getThemePref, installThemeListeners } from "./theme";
import "./styles/overlay-base.css";
import "./styles/motion.css";
import "./overlay.css";

/** Rust 侧 OverlayManager 发出的悬浮窗状态。 */
interface OverlayState {
  phase: "recording" | "processing" | "done" | "hidden";
}

export type Phase = "recording" | "processing" | "done" | "hidden" | null;

export const CROSSFADE_DURATION_MS = 180;

/** 电平轨迹：采样间隔与窗口长度（53 帧 × 100 ms ≈ 5.3 秒，恰好填满 158px 视口）。 */
export const TRACE_SAMPLE_INTERVAL_MS = 100;
export const TRACE_MAX_FRAMES = 53;

/** 把 0.0~1.0 的电平映射为轨迹条高度（px，视口高 14px：2 + 1.0 × 12）。 */
export function traceBarHeight(level: number): number {
  const clamped = Math.min(1, Math.max(0, level));
  return 2 + clamped * 12;
}

export interface PhaseTransitionResult {
  action: "show" | "crossfade" | "exit" | "none";
  /** show：要展示的相位。exit→hidden：动画结束后置 null 以清除 */
  phase?: Phase;
  /** crossfade：退出动画期间继续保持的当前相位 */
  exitPhase?: Phase;
  /** crossfade：延迟后进入的相位 */
  enterPhase?: Phase;
  /** crossfade：进入新相位前的延迟（毫秒） */
  delay?: number;
}

/**
 * 纯函数：根据上一相位和新相位，决定下一步动作。
 *
 * - hidden/null → visible: 直接显示（show）
 * - visible → hidden: 播放退出动画（exit）
 * - visible → 不同 visible: crossfade（先 exit，延迟后切换）
 * - visible → 相同 visible: 不动作（none）
 * - hidden → hidden: 清除（show null）
 * - null → hidden: 显示 null（清除状态，show）
 */
export function computePhaseTransition(
  prevPhase: Phase,
  newPhase: "recording" | "processing" | "done" | "hidden"
): PhaseTransitionResult {
  // 进入隐藏状态
  if (newPhase === "hidden") {
    if (prevPhase !== null && prevPhase !== "hidden") {
      return { action: "exit" };
    }
    // hidden→hidden 或 null→hidden：清除
    return { action: "show", phase: null };
  }

  // 进入可见状态
  if (prevPhase === null || prevPhase === "hidden") {
    return { action: "show", phase: newPhase };
  }
  if (prevPhase === newPhase) {
    return { action: "none" };
  }
  // 可见→不同可见：crossfade
  return {
    action: "crossfade",
    exitPhase: prevPhase,
    enterPhase: newPhase,
    delay: CROSSFADE_DURATION_MS,
  };
}

/** 相位与过渡状态：当前相位、退出/交叉渐变标记与相位追踪。 */
function usePhaseVisualState() {
  // 当前视觉相位：完全由 Rust 侧的 overlay-state 事件驱动。
  const [phase, setPhase] = useState<string | null>(null);

  // 是否处于退出过渡中（切换 CSS 类）。
  const [isExiting, setIsExiting] = useState(false);
  // Crossfade（相位间切换）只做淡出，不带退出位移。exit（隐藏）才滑出。
  const [isCrossfading, setIsCrossfading] = useState(false);

  // 记录上一相位以确定过渡方向。
  const prevPhaseRef = useRef<Phase>(null);
  const crossfadeTimerRef = useRef<number | null>(null);

  return {
    phase, setPhase, isExiting, setIsExiting, isCrossfading, setIsCrossfading,
    prevPhaseRef, crossfadeTimerRef,
  };
}

/** 转写内容状态：结果文本、润色失败、进度与电平轨迹。 */
function useOverlayContentState() {
  // 转写结果文本（done 阶段展示）。
  const [result, setResult] = useState<string | null>(null);

  // 润色失败原因（done 阶段在文本下方提示已回退原文，错误码结构，前端字典翻译）。
  const [polishError, setPolishError] = useState<PipelineErrorPayload | null>(null);

  // processing 阶段的进度信息。
  const [txProgress, setTxProgress] = useState<{
    phase: string;
    fraction: number | null;
  } | null>(null);

  // 电平轨迹：录音期间按 TRACE_SAMPLE_INTERVAL_MS 采样的电平历史（最多
  // TRACE_MAX_FRAMES 帧）。松开后不再采样也不再渲染，结果出现即清空。
  const [levelTrace, setLevelTrace] = useState<number[]>([]);
  const lastSampleAtRef = useRef(0);

  return {
    result, setResult, polishError, setPolishError,
    txProgress, setTxProgress, levelTrace, setLevelTrace, lastSampleAtRef,
  };
}

type PhaseVisual = ReturnType<typeof usePhaseVisualState>;
type OverlayContent = ReturnType<typeof useOverlayContentState>;

/** 悬浮窗的全部可变状态：把 Tauri 事件流折算成渲染所需数据，Overlay 只做组装。 */
function useOverlayState() {
  const visual = usePhaseVisualState();
  const content = useOverlayContentState();

  useEffect(() => {
    applyThemeToDocument(getThemePref());
    return installThemeListeners(() => applyThemeToDocument(getThemePref()));
  }, []);

  usePhaseStateEvents(visual, content);
  useOverlayContentEvents(content, visual.prevPhaseRef);

  // 只响应容器自身的 transitionend，子元素（进度条、按钮等）的
  // transition 结束会冒泡上来，不得因此提前清除内容。
  const handleTransitionEnd = (event: React.TransitionEvent) => {
    if (event.target !== event.currentTarget) return;
    if (visual.isExiting) {
      visual.setIsExiting(false);
      if (visual.prevPhaseRef.current === "hidden") {
        visual.setPhase(null);
      }
    }
  };

  const handleClose = async () => {
    try {
      await invoke("hide_overlay");
    } catch {
      // 悬浮窗 hide 失败，静默忽略
    }
  };

  return {
    phase: visual.phase, result: content.result, polishError: content.polishError,
    txProgress: content.txProgress, levelTrace: content.levelTrace,
    isExiting: visual.isExiting, isCrossfading: visual.isCrossfading,
    handleTransitionEnd, handleClose,
  };
}

/** 订阅 overlay-state 相位事件，折算为过渡动作。 */
function usePhaseStateEvents(visual: PhaseVisual, content: OverlayContent) {
  const { crossfadeTimerRef } = visual;

  // visual/content 的字段全是稳定引用（useState 的 setter 与 ref），空依赖即可。
  useEffect(() => {
    let active = true;

    const clearCrossfadeTimer = () => {
      if (crossfadeTimerRef.current !== null) {
        window.clearTimeout(crossfadeTimerRef.current);
        crossfadeTimerRef.current = null;
      }
    };

    const unlistenState = listen<OverlayState>("overlay-state", (event) => {
      if (!active) return;
      clearCrossfadeTimer();
      applyPhaseEvent(visual, content, event.payload.phase, () => active);
    });

    return () => {
      active = false;
      clearCrossfadeTimer();
      unlistenState.then((fn) => fn());
    };
  }, []);
}

/** 订阅内容事件：电平采样、结果文本、润色失败与转写进度。 */
function useOverlayContentEvents(
  content: OverlayContent,
  prevPhaseRef: PhaseVisual["prevPhaseRef"],
) {
  const { lastSampleAtRef, setLevelTrace } = content;

  // 闭包内只用到稳定引用（useState 的 setter 与 ref），空依赖即可。
  useEffect(() => {
    let active = true;

    const unlistenAudioLevel = listen<number>("audio-level", (event) => {
      if (!active) return;
      appendLevelSample(event.payload ?? 0, prevPhaseRef, lastSampleAtRef, setLevelTrace);
    });

    const unlistenResult = listen<string>("transcription-result", (event) => {
      if (!active) return;
      content.setResult(event.payload);
    });

    const unlistenPolishFailed = listen<PipelineErrorPayload>("polish-failed", (event) => {
      if (!active) return;
      content.setPolishError(event.payload);
    });

    const unlistenTxProgress = listen<{
      phase: string;
      fraction: number | null;
    }>("transcription-progress", (event) => {
      if (!active) return;
      content.setTxProgress(event.payload);
    });

    return () => {
      active = false;
      unlistenResult.then((fn) => fn());
      unlistenPolishFailed.then((fn) => fn());
      unlistenTxProgress.then((fn) => fn());
      unlistenAudioLevel.then((fn) => fn());
    };
  }, []);
}

/** 把一个相位事件折算为状态更新：show / exit / crossfade / none 四种动作。 */
function applyPhaseEvent(
  visual: PhaseVisual,
  content: OverlayContent,
  newPhase: OverlayState["phase"],
  isActive: () => boolean,
) {
  const transition = computePhaseTransition(visual.prevPhaseRef.current, newPhase);

  switch (transition.action) {
    case "show":
      visual.setIsExiting(false);
      visual.setIsCrossfading(false);
      visual.setPhase(transition.phase!);
      break;
    case "exit":
      visual.setIsCrossfading(false);
      visual.setIsExiting(true);
      break;
    case "crossfade":
      resetContentForPhase(content, newPhase);
      visual.setIsCrossfading(true);
      visual.setIsExiting(true);
      visual.prevPhaseRef.current = transition.enterPhase ?? null;
      scheduleCrossfadeEnter(visual, transition, isActive);
      return;
    case "none":
      return;
  }

  // show/exit 分支也清除转写状态（进入非 done 的可见相位时）
  resetContentForPhase(content, newPhase);
  visual.prevPhaseRef.current = newPhase;
}

/** 相位切换时清理转写内容：非 done 的可见相位清结果与错误，轨迹只在录音与出结果时重置。 */
function resetContentForPhase(content: OverlayContent, newPhase: OverlayState["phase"]) {
  if (newPhase === "hidden") return;
  if (newPhase === "done") {
    // 结果文本已出，轨迹完成使命随 crossfade 退场。
    content.setTxProgress(null);
    content.setLevelTrace([]);
    return;
  }
  content.setTxProgress(null);
  content.setResult(null);
  content.setPolishError(null);
  if (newPhase === "recording") {
    // 新一轮录音：轨迹重新累积（不存在 processing→recording 路径，
    // 进入 recording 即代表上一轮已结束）。
    content.setLevelTrace([]);
  }
}

/** crossfade 延迟进入：先播退出动画，延迟结束后再切换到新相位。 */
function scheduleCrossfadeEnter(
  visual: PhaseVisual,
  transition: PhaseTransitionResult,
  isActive: () => boolean,
) {
  requestAnimationFrame(() => {
    visual.crossfadeTimerRef.current = window.setTimeout(() => {
      if (!isActive()) return;
      visual.crossfadeTimerRef.current = null;
      visual.setIsExiting(false);
      visual.setIsCrossfading(false);
      visual.setPhase(transition.enterPhase!);
    }, transition.delay);
  });
}

/** 电平采样：仅录音相位、按采样间隔节流，超出窗口长度丢弃最旧帧。 */
function appendLevelSample(
  level: number,
  prevPhaseRef: PhaseVisual["prevPhaseRef"],
  lastSampleAtRef: OverlayContent["lastSampleAtRef"],
  setLevelTrace: OverlayContent["setLevelTrace"],
) {
  // 仅录音相位采样，processing/done/hidden 不采。
  if (prevPhaseRef.current !== "recording") return;
  const now = Date.now();
  if (now - lastSampleAtRef.current < TRACE_SAMPLE_INTERVAL_MS) return;
  lastSampleAtRef.current = now;
  setLevelTrace((prev) => {
    const next = [...prev, level];
    return next.length > TRACE_MAX_FRAMES
      ? next.slice(next.length - TRACE_MAX_FRAMES)
      : next;
  });
}

/** done 相位内容：结果圆盘、单行文本、润色失败提示与关闭按钮。 */
function DoneContent({
  t,
  result,
  polishError,
  onClose,
}: {
  t: (key: string) => string;
  result: string;
  polishError: PipelineErrorPayload | null;
  onClose: () => void;
}) {
  return (
    <>
      <div className={`done-indicator ${polishError ? "done-indicator--failed" : ""}`}>
        <Check size={11} strokeWidth={2.5} aria-hidden />
      </div>
      <span className="result-text">{result}</span>
      {polishError && (
        <span
          className="result-warn"
          title={`${t("overlay.polish_failed")}：${tError(polishError.code, polishError.params)}`}
        >
          <TriangleAlert size={12} strokeWidth={2} aria-hidden />
        </span>
      )}
      <button
        type="button"
        className="btn-close"
        onClick={onClose}
        title={t("overlay.close")}
        aria-label={t("overlay.close")}
      >
        <X size={14} strokeWidth={2.5} aria-hidden />
      </button>
    </>
  );
}

/** recording 相位内容：状态圆点与电平轨迹。 */
function RecordingContent({ levelTrace }: { levelTrace: number[] }) {
  return (
    <>
      <span className="dot" aria-hidden />
      <div className="level-trace" aria-hidden>
        {levelTrace.map((level, i) => (
          <div
            key={i}
            className="trace-bar"
            style={{ height: `${traceBarHeight(level).toFixed(1)}px` }}
          />
        ))}
      </div>
    </>
  );
}

/** processing 相位内容：转写环形动画与进度线（无进度时为不定态动画）。 */
function ProcessingContent({
  txProgress,
}: {
  txProgress: { phase: string; fraction: number | null } | null;
}) {
  return (
    <>
      <div className="processing-ring" />
      <div className="overlay-tx-progress-track">
        <div
          className={`overlay-tx-progress-fill ${
            txProgress?.fraction == null ? "indeterminate" : ""
          }`}
          style={
            txProgress?.fraction != null
              ? {
                  transform: `scaleX(${Math.min(
                    1,
                    Math.max(0, txProgress.fraction)
                  )})`,
                }
              : undefined
          }
        />
      </div>
    </>
  );
}

export function Overlay() {
  const { t } = useTranslation();
  const s = useOverlayState();

  if (s.phase === null && !s.isExiting) return null;

  const containerClass = `island-container ${
    s.isExiting ? (s.isCrossfading ? "island-crossfade" : "island-exit") : "island-enter"
  }`;

  // 生产端保证 transcription-result 先于 done 到达，若乱序先收到 done 且
  // 结果未到，继续显示 processing 视图，避免渲染出没有内容的空 island（闪烁）。
  const effectivePhase = s.phase === "done" && !s.result ? "processing" : s.phase;

  if (s.phase === "done" && s.result) {
    return (
      <div className={containerClass} onTransitionEnd={s.handleTransitionEnd}>
        <div className="island">
          <DoneContent
            t={t}
            result={s.result}
            polishError={s.polishError}
            onClose={s.handleClose}
          />
        </div>
      </div>
    );
  }

  return (
    <div className={containerClass} onTransitionEnd={s.handleTransitionEnd}>
      <div className="island">
        {effectivePhase === "recording" && (
          <RecordingContent levelTrace={s.levelTrace} />
        )}
        {effectivePhase === "processing" && (
          <ProcessingContent txProgress={s.txProgress} />
        )}
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<Overlay />);
