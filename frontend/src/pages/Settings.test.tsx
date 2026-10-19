import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Settings from "./Settings";
import type { ProviderPreset } from "../config/modelPresets";

const { invokeMock, loadCatalogMock, savedPatches, state } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  loadCatalogMock: vi.fn(),
  // 后端收到过的 save_config patch，按到达顺序记录（最后一条 = 最终落盘内容）。
  savedPatches: [] as Record<string, unknown>[],
  // get_config 返回体在用例间可变（本地面板 / 在线面板各用一份）。
  state: { config: {} as Record<string, unknown> },
}));

const examplePreset: ProviderPreset = {
  name: "Example Provider",
  websiteUrl: "",
  apiBaseUrl: "https://api.example.com/v1",
  category: "custom",
  modelTypes: ["polisher"],
  apiFormat: "openai",
  defaultModel: "example-model",
  models: [{ model: "example-model", displayName: "Example Model" }],
};

const localConfig = {
  keyName: "Alt_R",
  linuxEvdevCode: null,
  windowsVkCode: null,
  language: "zh",
  model: "",
  transcriberBackend: "local",
  asrModel: "",
  asrApiBaseUrl: "",
  asrProtocol: "mimo_chat",
  asrApiKey: "",
  hasAsrApiKey: false,
  polishLevel: "none",
  polishModel: "",
  polishProtocol: "openai",
  polishApiBaseUrl: "https://api.example.com/v1",
  polishThinkingLevel: "off",
  guiLanguage: "zh",
  overlayPosition: "bottom_center",
  autoCheckUpdate: true,
  hasPolisherApiKey: false,
  polisherApiKey: "",
  injectText: false,
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("0.0.0-test") }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: { getByLabel: vi.fn().mockResolvedValue(null) },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ message: vi.fn() }));

vi.mock("../config/catalog", () => ({ loadCatalog: loadCatalogMock }));

vi.mock("../i18n", () => {
  // t 保持稳定引用：useModelManager 的 refreshModels 以 t 为依赖，identity 变化会让
  // 取模型列表的 effect 每轮渲染重跑，形成无限循环。
  const t = (k: string) => k;
  return { useTranslation: () => ({ t, lang: "zh", setLang: vi.fn() }) };
});

vi.mock("../ThemeContext", () => ({
  useTheme: () => ({ themePref: "system", setTheme: vi.fn() }),
}));

vi.mock("../ui-size", () => ({
  getFontSizePref: () => "medium",
  getWindowSizePref: () => "medium",
  setFontSizePref: vi.fn(),
  setWindowSizePref: vi.fn(),
}));

const lastSavedPatch = () => savedPatches[savedPatches.length - 1];

beforeEach(() => {
  savedPatches.length = 0;
  state.config = { ...localConfig };
  loadCatalogMock.mockReset();
  loadCatalogMock.mockResolvedValue([]);
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    if (cmd === "get_config") return Promise.resolve(state.config);
    if (cmd === "list_models") return Promise.resolve([]);
    if (cmd === "save_config" && args && typeof args === "object" && "patch" in args) {
      const { patch } = args;
      if (patch && typeof patch === "object") savedPatches.push({ ...patch });
    }
    return Promise.resolve(null);
  });
});

describe("Settings 供应商目录自动加载", () => {
  it("挂载即拉取目录一次，并把结果渲染进供应商摘要", async () => {
    loadCatalogMock.mockResolvedValue([examplePreset]);

    render(<Settings />);

    await waitFor(() => {
      expect(loadCatalogMock).toHaveBeenCalledTimes(1);
    });
    // 目录结果按 polishApiBaseUrl 匹配为当前供应商，摘要区显示其名字。
    await waitFor(() => {
      expect(screen.getByText("Example Provider")).toBeTruthy();
    });
  });
});

describe("Settings 在线识别服务商预设与协议", () => {
  it("选中预设后自动填充 base URL、模型与协议，且字段仍可手改", async () => {
    state.config = { ...localConfig, transcriberBackend: "online" };
    render(<Settings />);
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "settings.asr_provider" })).toBeTruthy(),
    );

    // 展开“高级”：模型与地址输入框在折叠区内。
    fireEvent.click(screen.getByRole("button", { name: "settings.advanced_asr" }));

    // 选智谱预设：base URL、模型、协议三项一起填上。
    fireEvent.change(screen.getByRole("combobox", { name: "settings.asr_provider" }), {
      target: { value: "zhipu" },
    });
    expect(screen.getByDisplayValue("https://open.bigmodel.cn/api/paas/v4")).toBeTruthy();
    expect(screen.getByDisplayValue("glm-asr-2512")).toBeTruthy();

    // 协议下拉已随预设填为 transcriptions，并可继续手改。
    const protocol = screen.getByRole("combobox", { name: "settings.asr_protocol" }) as HTMLSelectElement;
    expect(protocol.value).toBe("transcriptions");
    fireEvent.change(protocol, { target: { value: "chat_input_audio" } });
    expect(protocol.value).toBe("chat_input_audio");

    // 保存后协议进 patch。
    fireEvent.click(screen.getByRole("button", { name: "settings.save" }));
    await waitFor(() => expect(lastSavedPatch()).toBeDefined());
    expect(lastSavedPatch()).toMatchObject({
      asrProtocol: "chat_input_audio",
      asrApiBaseUrl: "https://open.bigmodel.cn/api/paas/v4",
      asrModel: "glm-asr-2512",
    });
  });

  it("当前值不匹配任何预设时回显“自定义”，不改动字段", async () => {
    state.config = {
      ...localConfig,
      transcriberBackend: "online",
      asrApiBaseUrl: "https://asr.example.com/v1",
      asrModel: "my-model",
      asrProtocol: "transcriptions",
    };
    render(<Settings />);
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "settings.asr_provider" })).toBeTruthy(),
    );
    const provider = screen.getByRole("combobox", { name: "settings.asr_provider" }) as HTMLSelectElement;
    expect(provider.value).toBe("custom");
  });
});
