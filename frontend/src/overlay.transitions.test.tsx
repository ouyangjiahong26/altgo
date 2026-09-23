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
vi.mock("./utils/clipboard", () => ({ copyToClipboard: vi.fn() }));

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
    expect(container.textContent).toContain("overlay.transcribing");

    // 生产端契约是 result 先于 done；即使乱序到达，前端也应继续显示
    // processing 视图，而不是渲染没有任何内容的空 pill（闪烁）。
    emitPhase("done");
    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.textContent).toContain("overlay.transcribing");
  });

  it("相位间 crossfade 只做淡出，不带退出位移", () => {
    const { container } = render(<Overlay />);
    emitPhase("recording");
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
    expect(container.textContent).toContain("overlay.transcribing");
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

  it("recording 状态下电平轨迹随 audio-level 累积，processing 冻结", () => {
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
    expect((bars[0] as HTMLElement).style.height).toBe("16.6px");

    // 采样间隔内的后续事件被节流丢弃。
    emitAudioLevel(0.5);
    bars = container.querySelectorAll(".trace-bar");
    expect(bars.length).toBe(1);
    expect((bars[0] as HTMLElement).style.height).toBe("16.6px");

    // 超过采样间隔后追加新帧。
    act(() => {
      vi.advanceTimersByTime(120);
    });
    emitAudioLevel(0.2);
    bars = container.querySelectorAll(".trace-bar");
    expect(bars.length).toBe(2);
    expect((bars[1] as HTMLElement).style.height).toBe("6.4px");

    // 切到 processing：轨迹冻结保留（不再采样新帧），并标记 frozen。
    act(() => {
      vi.advanceTimersByTime(120);
    });
    emitPhase("processing");
    emitAudioLevel(0.9);
    act(() => {
      vi.advanceTimersByTime(250);
    });
    const trace = container.querySelector(".level-trace")!;
    expect(trace.className).toContain("level-trace--frozen");
    expect(trace.querySelectorAll(".trace-bar").length).toBe(2);
  });

  it("result 先于 done 到达时正常显示结果", () => {
    const { container } = render(<Overlay />);
    emitPhase("processing");
    emitResult("你好，世界");
    emitPhase("done");
    act(() => {
      vi.advanceTimersByTime(250);
    });

    expect(container.textContent).toContain("你好，世界");
  });
});
