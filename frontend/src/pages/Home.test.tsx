import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
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

/** 主页用了 useNavigate，测试里也要给一个路由上下文。 */
function renderHome() {
  return render(
    <MemoryRouter>
      <Home />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  localStorage.setItem("altgo-lang", "zh");
  localStorage.removeItem("altgo-hotkey-hint-seen");
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
    renderHome();
    const banner = await screen.findByRole("status");
    expect(banner.textContent).toContain("上次转写失败，录音已保留");
    expect(banner.textContent).toContain("13 秒");
    expect(banner.textContent).toContain("在线识别请求失败");
    expect(screen.getByRole("button", { name: /重新识别/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /放弃/ })).toBeTruthy();
  });

  it("点击重新识别与放弃分别调用对应命令", async () => {
    renderHome();
    await screen.findByRole("status");

    fireEvent.click(screen.getByRole("button", { name: /重新识别/ }));
    expect(invokeMock).toHaveBeenCalledWith("retry_pending_transcription");

    fireEvent.click(screen.getByRole("button", { name: /放弃/ }));
    expect(invokeMock).toHaveBeenCalledWith("discard_pending_recording");
  });

  it("pending-recording-changed 事件送 null 后横幅消失", async () => {
    renderHome();
    await screen.findByRole("status");

    act(() => {
      emit("pending-recording-changed", null);
    });

    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("Home 触发键自检卡", () => {
  /** 让自检命令返回给定的观测结果。 */
  function stubProbe(result: unknown) {
    invokeMock.mockImplementation((cmd: string) => {
      switch (cmd) {
        case "get_config":
          return Promise.resolve({ keyName: "Alt_R" });
        case "probe_activation_key":
          return Promise.resolve(result);
        case "get_pending_recording":
          return Promise.resolve(null);
        case "list_history":
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });
  }

  it("首次进主页展开自检卡，跳过之后只留入口按钮", async () => {
    renderHome();

    expect(await screen.findByText("先确认触发键能用")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "以后再说" }));

    expect(screen.queryByText("先确认触发键能用")).toBeNull();
    expect(screen.getByRole("button", { name: /检测触发键/ })).toBeTruthy();
  });

  it("已看过提示时只显示入口按钮", async () => {
    localStorage.setItem("altgo-hotkey-hint-seen", "1");
    renderHome();

    await screen.findByRole("button", { name: /检测触发键/ });
    expect(screen.queryByText("先确认触发键能用")).toBeNull();
  });

  it("抓到与配置一致的键时报告可用", async () => {
    stubProbe({
      observed: { keyName: "Alt_R", linuxEvdevCode: null, windowsVkCode: 0xa5 },
      configuredKeyName: "Alt_R",
      configuredCode: 0xa5,
      matched: true,
    });
    renderHome();

    fireEvent.click(await screen.findByRole("button", { name: /开始检测/ }));

    expect(await screen.findByText("已检测到 右Alt，触发键可用")).toBeTruthy();
  });

  it("键不一致时给出两边键名与诊断键码", async () => {
    stubProbe({
      observed: { keyName: "vk_0xA4", linuxEvdevCode: null, windowsVkCode: 0xa4 },
      configuredKeyName: "Alt_R",
      configuredCode: 0xa5,
      matched: false,
    });
    renderHome();

    fireEvent.click(await screen.findByRole("button", { name: /开始检测/ }));

    expect(
      await screen.findByText("你按下的是 vk_0xA4，当前触发键是 右Alt"),
    ).toBeTruthy();
    expect(screen.getByText("诊断信息：抓到 0xA4，当前触发键 0xA5")).toBeTruthy();
    expect(screen.getByRole("button", { name: /去设置/ })).toBeTruthy();
  });

  it("没有收到按键时报超时并给重试", async () => {
    stubProbe({
      observed: null,
      configuredKeyName: "Alt_R",
      configuredCode: 0xa5,
      matched: false,
    });
    renderHome();

    fireEvent.click(await screen.findByRole("button", { name: /开始检测/ }));

    expect(await screen.findByText("没有收到任何按键")).toBeTruthy();
    expect(screen.getByRole("button", { name: /重试/ })).toBeTruthy();
  });

  it("检测命令失败时显示原因", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      switch (cmd) {
        case "get_config":
          return Promise.resolve({ keyName: "Alt_R" });
        case "probe_activation_key":
          return Promise.reject("键盘钩子启动失败: 5");
        case "list_history":
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });
    renderHome();

    fireEvent.click(await screen.findByRole("button", { name: /开始检测/ }));

    expect(
      await screen.findByText("检测失败：键盘钩子启动失败: 5"),
    ).toBeTruthy();
  });
});
