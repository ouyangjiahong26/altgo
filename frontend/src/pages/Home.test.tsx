import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import Home from "./Home";

type EventListener = (event: { payload: unknown }) => void;

const { invokeMock, listeners } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listeners: new Map<string, EventListener[]>(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: EventListener) => {
    const list = listeners.get(event) ?? [];
    list.push(handler);
    listeners.set(event, list);
    return Promise.resolve(() => {
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((h) => h !== handler),
      );
    });
  }),
}));
function emit(event: string, payload: unknown) {
  for (const handler of listeners.get(event) ?? []) {
    handler({ payload });
  }
}

const pendingInfo = {
  durationMs: 12500,
  error: { code: "transcriber.http_error", params: { detail: "timeout" } },
};

beforeEach(() => {
  localStorage.setItem("altgo-lang", "zh");
  listeners.clear();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_config":
        return Promise.resolve({ keyName: "Alt_R" });
      case "get_pending_recording":
        return Promise.resolve(pendingInfo);
      case "list_history":
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  });
});

describe("Home 待重试录音横幅", () => {
  it("有待重试录音时展示时长与失败原因", async () => {
    render(<Home />);
    const banner = await screen.findByRole("status");
    expect(banner.textContent).toContain("上次转写失败，录音已保留");
    expect(banner.textContent).toContain("13 秒");
    expect(banner.textContent).toContain("在线识别请求失败");
    expect(screen.getByRole("button", { name: /重新识别/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /放弃/ })).toBeTruthy();
  });

  it("点击重新识别与放弃分别调用对应命令", async () => {
    render(<Home />);
    await screen.findByRole("status");

    fireEvent.click(screen.getByRole("button", { name: /重新识别/ }));
    expect(invokeMock).toHaveBeenCalledWith("retry_pending_transcription");

    fireEvent.click(screen.getByRole("button", { name: /放弃/ }));
    expect(invokeMock).toHaveBeenCalledWith("discard_pending_recording");
  });

  it("pending-recording-changed 事件送 null 后横幅消失", async () => {
    render(<Home />);
    await screen.findByRole("status");

    act(() => {
      emit("pending-recording-changed", null);
    });

    expect(screen.queryByRole("status")).toBeNull();
  });
});
