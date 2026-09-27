// @vitest-environment jsdom
//
// 悬浮窗相位转换的反馈回路：挂载真实 Overlay 组件，mock Tauri 事件通道，
// 回放生产端实际的事件序列，断言用户报告的症状（闪烁/跳变）。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, act, fireEvent } from "@testing-library/react";

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Handler>();

vi.mock("react-dom/client", () => ({ createRoot: () => ({ render: vi.fn() }) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: Handler) => {
    handlers.set(name, cb);
    return Promise.resolve(() => handlers.delete(name));
  }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./i18n", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
vi.mock("./theme", () => ({
  applyThemeToDocument: vi.fn(),
  getThemePref: vi.fn(),
  installThemeListeners: vi.fn(() => () => {}),
}));

import { Overlay } from "./overlay";

function emitPhase(phase: "recording" | "processing" | "done" | "hidden") {
  act(() => handlers.get("overlay-state")!({ payload: { phase } }));
}

function emitResult(text: string) {
  act(() => handlers.get("transcription-result")!({ payload: text }));
}

function emitAudioLevel(level: number) {
  act(() => handlers.get("audio-level")!({ payload: level }));
}

function emitPolishFailed(reason: string) {
  act(() => handlers.get("polish-failed")!({ payload: reason }));
}

describe("Overlay 相位转换", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("done 先于 transcription-result 到达时，不应渲染空 island", () => {
    const { container } = render(<Overlay />);
    emitPhase("processing");
    expect(container.querySelector(".processing-ring")).not.toBeNull();

    // 生产端契约是 result 先于 done；即使乱序到达，前端也应继续显示
    // processing 视图，而不是渲染没有任何内容的空 pill（闪烁）。
    emitPhase("done");
    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.querySelector(".processing-ring")).not.toBeNull();
    expect(container.querySelector(".result-text")).toBeNull();
  });

  it("相位间 crossfade 只做淡出，不带退出位移", () => {
    const { container } = render(<Overlay />);
    emitPhase("recording");
    expect(container.querySelector(".dot")).not.toBeNull();
    expect(container.querySelector(".level-trace")).not.toBeNull();
    emitPhase("processing");

    // crossfade 期间：用 island-crossfade（只淡出），不用 island-exit（带位移）。
    const during = container.querySelector(".island-container")!;
    expect(during.className).toContain("island-crossfade");
    expect(during.className).not.toContain("island-exit");

    act(() => {
      vi.advanceTimersByTime(250);
    });
    expect(container.querySelector(".island-container")!.className).toContain(
      "island-enter"
    );
    expect(container.querySelector(".processing-ring")).not.toBeNull();
    expect(container.querySelector(".overlay-tx-progress-track")).not.toBeNull();
  });

  it("退出动画期间，子元素冒泡的 transitionend 不应提前清除内容", () => {
    const { container } = render(<Overlay />);
    emitPhase("recording");
    emitPhase("hidden");

    const containerEl = container.querySelector(".island-container")!;
    expect(containerEl.className).toContain("island-exit");

    // 子元素（如进度条、按钮）的 transition 结束会向上冒泡。
    // 只有容器自身的 transitionend 才允许结束退出动画。
    const island = container.querySelector(".island")!;
    fireEvent.transitionEnd(island);
    expect(container.querySelector(".island")).not.toBeNull();

    // 容器自身的 transitionend：退出动画结束，内容清除。
    fireEvent.transitionEnd(containerEl);
    expect(container.querySelector(".island")).toBeNull();
  });

  it("recording 相位电平轨迹随 audio-level 节流累积，切到 processing 后随 crossfade 移除", () => {
    const { container } = render(<Overlay />);
    emitPhase("recording");
    // 保证“距上次采样”超过采样间隔（fake timers 的起点可能是 0）。
    act(() => {
      vi.advanceTimersByTime(1000);
    });

    // 第一次事件立即采样为一帧轨迹。
    emitAudioLevel(0.8);
    let bars = container.querySelectorAll(".trace-bar");
    expect(bars.length).toBe(1);
    expect((bars[0] as HTMLElement).style.height).toBe("11.6px");

    // 采样间隔内的后续事件被节流丢弃。
    emitAudioLevel(0.5);
    bars = container.querySelectorAll(".trace-bar");
    expect(bars.length).toBe(1);
    expect((bars[0] as HTMLElement).style.height).toBe("11.6px");

    // 超过采样间隔后追加新帧。
    act(() => {
      vi.advanceTimersByTime(120);
    });
    emitAudioLevel(0.2);
    bars = container.querySelectorAll(".trace-bar");
    expect(bars.length).toBe(2);
    expect((bars[1] as HTMLElement).style.height).toBe("4.4px");

    // 切到 processing：轨迹随 crossfade 移除，换成 spinner + 进度线（无冻结轨迹）。
    act(() => {
      vi.advanceTimersByTime(120);
    });
    emitPhase("processing");
    act(() => {
      vi.advanceTimersByTime(250);
    });
    expect(container.querySelector(".level-trace")).toBeNull();
    expect(container.querySelector(".processing-ring")).not.toBeNull();
    expect(container.querySelector(".overlay-tx-progress-track")).not.toBeNull();
  });

  it("result 先于 done 到达时显示单行结果，无复制按钮，关闭按钮在位", () => {
    const { container } = render(<Overlay />);
    emitPhase("processing");
    emitResult("你好，世界");
    emitPhase("done");
    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.querySelector(".result-text")!.textContent).toBe("你好，世界");
    // 复制职能已删——结果自动写入剪贴板。
    expect(container.querySelector(".btn-copy")).toBeNull();
    // 关闭按钮常驻 DOM，hover 显隐由 CSS 控制；此处断言元素与语义在位。
    const close = container.querySelector(".btn-close") as HTMLElement;
    expect(close).not.toBeNull();
    expect(close.title).toBe("overlay.close");
    expect(close.getAttribute("aria-label")).toBe("overlay.close");
  });

  it("polish-failed 事件后 ✓ 圆盘转失败态并出现 ⚠（title 含错误信息）", () => {
    const { container } = render(<Overlay />);
    emitPhase("processing");
    emitResult("你好，世界");
    emitPolishFailed("连接超时");
    emitPhase("done");
    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.querySelector(".done-indicator--failed")).not.toBeNull();
    const warn = container.querySelector(".result-warn") as HTMLElement;
    expect(warn).not.toBeNull();
    expect(warn.title).toContain("overlay.polish_failed");
    expect(warn.title).toContain("连接超时");
  });
});
