import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { ONBOARDING_KEY } from "../onboarding";

const { invokeMock, loadCatalogMock, listenerHandlers, savedPatches } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  loadCatalogMock: vi.fn(),
  listenerHandlers: new Map<string, (event: { payload: unknown }) => void>(),
  // 后端收到过的 save_config patch，按到达顺序记录（最后一条 = 最终落盘内容）。
  // save_config patches in arrival order; the last one is what lands on disk.
  savedPatches: [] as Record<string, unknown>[],
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

const modelsResponse = [
  { name: "sense-voice", filename: "model.int8.onnx", sizeBytes: 175000000, description: "", downloaded: true },
  { name: "sense-voice-small", filename: "model.int8.onnx", sizeBytes: 93000000, description: "", downloaded: false },
];

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.0.0-test"),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (e: { payload: unknown }) => void) => {
    listenerHandlers.set(event, handler);
    return Promise.resolve(() => {
      listenerHandlers.delete(event);
    });
  },
}));

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

const lastSavedPatch = () => savedPatches[savedPatches.length - 1];

describe("首次安装引导", () => {
  beforeEach(() => {
    localStorage.clear();
    listenerHandlers.clear();
    savedPatches.length = 0;
    loadCatalogMock.mockReset();
    loadCatalogMock.mockResolvedValue([]);
    invokeMock.mockReset();
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "get_config") return Promise.resolve(configResponse);
      if (cmd === "list_models") return Promise.resolve(modelsResponse);
      if (cmd === "list_history") return Promise.resolve([]);
      if (cmd === "save_config" && args && typeof args === "object" && "patch" in args) {
        const { patch } = args;
        if (patch && typeof patch === "object") savedPatches.push({ ...patch });
      }
      return Promise.resolve(null);
    });
  });

  it("无引导标记时整窗渲染欢迎屏，不渲染主导航", async () => {
    render(<App />);

    // 向导持有全量配置：挂载即读取一次，读到后才渲染欢迎屏。
    // The wizard owns the whole config: one read on mount, and the welcome screen appears after it.
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_config"));
    await waitFor(() => expect(screen.getByText("onboarding.welcome_title")).toBeTruthy());
    expect(screen.queryByText("nav.settings")).toBeNull();
  });

  it("走完五步点“开始使用”后保存配置并落引导标记", async () => {
    render(<App />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_config"));

    // 欢迎 → 触发键 → 引擎 → 润色（级别为关闭时按钮显示“跳过”）→ 完成。
    // welcome → key → engine → polish (the button reads “skip” while the level is off) → done.
    for (let i = 0; i < 4; i++) {
      fireEvent.click(
        screen.getByRole("button", { name: /onboarding\.(next|polish_skip)/ }),
      );
    }
    fireEvent.click(screen.getByRole("button", { name: "onboarding.start" }));

    await waitFor(() => expect(localStorage.getItem(ONBOARDING_KEY)).toBe("1"));
    expect(lastSavedPatch()).toMatchObject({
      keyName: "Alt_R",
      transcriberBackend: "local",
    });
  });

  it("模型下载完成后按最新表单保存，不覆盖向导里填的润色配置", async () => {
    render(<App />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_config"));

    // 引擎步点击“下载并启用”：下载要等 model-download-finished，耗时可超过向导本身。
    // Click Download & enable on the engine step: it resolves only when
    // model-download-finished arrives, which can outlive the whole wizard.
    fireEvent.click(screen.getByRole("button", { name: "onboarding.next" }));
    fireEvent.click(screen.getByRole("button", { name: "onboarding.next" }));
    fireEvent.click(screen.getByRole("button", { name: "settings.download_and_use" }));
    fireEvent.click(screen.getByRole("button", { name: "onboarding.next" }));

    // 润色步：填级别、API 地址与模型（两个 textbox 依次是地址与模型）。
    // Polish step: level, API URL and model (the two textboxes are URL then model).
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "medium" } });
    const [apiUrl, model] = screen.getAllByRole("textbox");
    fireEvent.change(apiUrl, { target: { value: "https://api.deepseek.com/v1" } });
    fireEvent.change(model, { target: { value: "deepseek-chat" } });

    fireEvent.click(screen.getByRole("button", { name: "onboarding.next" }));
    fireEvent.click(screen.getByRole("button", { name: "onboarding.start" }));
    await waitFor(() => expect(localStorage.getItem(ONBOARDING_KEY)).toBe("1"));

    // 下载结束：写入的必须是“最新表单 + 新模型”，不能是点击时的旧快照。
    // The download finishes: the save must carry the latest form plus the new model, never the
    // click-time snapshot.
    listenerHandlers.get("model-download-finished")?.({
      payload: { name: "sense-voice-small", success: true, path: "/tmp/sense-voice-small" },
    });

    await waitFor(() =>
      expect(lastSavedPatch()).toMatchObject({
        model: "sense-voice-small",
        polishLevel: "medium",
        polishModel: "deepseek-chat",
        polishApiBaseUrl: "https://api.deepseek.com/v1",
      }),
    );
  });
});
