import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { ONBOARDING_KEY } from "../onboarding";

const { invokeMock, loadCatalogMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  loadCatalogMock: vi.fn(),
}));

const configResponse = {
  keyName: "Alt_R",
  linuxEvdevCode: null,
  windowsVkCode: null,
  language: "zh",
  model: "",
  transcriberBackend: "local",
  asrModel: "",
  asrApiBaseUrl: "",
  asrApiKey: "",
  hasAsrApiKey: false,
  polishLevel: "none",
  polishModel: "",
  polishProtocol: "openai",
  polishApiBaseUrl: "",
  polishThinkingLevel: "off",
  guiLanguage: "zh",
  overlayPosition: "bottom_center",
  autoCheckUpdate: true,
  injectText: false,
  polisherApiKey: "",
  hasPolisherApiKey: false,
};

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.0.0-test"),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(() => {}),
    startDragging: vi.fn(),
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ message: vi.fn() }));

vi.mock("../config/catalog", () => ({ loadCatalog: loadCatalogMock }));

vi.mock("../i18n", () => {
  // t 保持稳定引用：useModelManager 的 refreshModels 以 t 为依赖，identity 变化会让
  // 取模型列表的 effect 每轮渲染重跑，形成无限循环。
  // Keep t referentially stable: useModelManager's refreshModels depends on t, and a new
  // identity every render would re-run the model-list effect forever.
  const t = (key: string) => key;
  return { useTranslation: () => ({ t, lang: "zh", setLang: vi.fn() }) };
});

vi.mock("../ThemeContext", () => ({
  useTheme: () => ({ themePref: "system", setTheme: vi.fn() }),
}));

describe("首次安装引导", () => {
  beforeEach(() => {
    localStorage.clear();
    loadCatalogMock.mockReset();
    loadCatalogMock.mockResolvedValue([]);
    invokeMock.mockReset();
    invokeMock.mockImplementation((cmd: string) => {
      switch (cmd) {
        case "get_config":
          return Promise.resolve(configResponse);
        case "list_models":
        case "list_history":
          return Promise.resolve([]);
        case "save_config":
        case "resolve_model":
          return Promise.resolve(null);
        default:
          return Promise.resolve(null);
      }
    });
  });

  it("无引导标记时整窗渲染欢迎屏，不渲染主导航", async () => {
    render(<App />);

    // 向导持有全量配置：挂载即读取一次，读到后才渲染欢迎屏。
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_config"));
    await waitFor(() => expect(screen.getByText("onboarding.welcome_title")).toBeTruthy());
    expect(screen.queryByText("nav.settings")).toBeNull();
  });

  it("走完五步点“开始使用”后保存配置并落引导标记", async () => {
    render(<App />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_config"));

    // 欢迎 → 触发键 → 引擎 → 润色（级别为关闭时按钮显示“跳过”）→ 完成。
    for (let i = 0; i < 4; i++) {
      fireEvent.click(
        screen.getByRole("button", { name: /onboarding\.(next|polish_skip)/ }),
      );
    }
    fireEvent.click(screen.getByRole("button", { name: "onboarding.start" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_config", {
        patch: expect.objectContaining({ keyName: "Alt_R", transcriberBackend: "local" }),
      }),
    );
    await waitFor(() => expect(localStorage.getItem(ONBOARDING_KEY)).toBe("1"));
  });
});
