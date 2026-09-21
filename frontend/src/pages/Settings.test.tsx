import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Settings from "./Settings";
import type { ProviderPreset } from "../config/modelPresets";

const { loadCatalogMock } = vi.hoisted(() => ({ loadCatalogMock: vi.fn() }));

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

const configResponse = {
  keyName: "Alt_R",
  linuxEvdevCode: null,
  windowsVkCode: null,
  language: "zh",
  model: "",
  polishLevel: "none",
  polishModel: "",
  polishProtocol: "openai",
  polishApiBaseUrl: "https://api.example.com/v1",
  polishThinkingLevel: "off",
  guiLanguage: "zh",
  overlayPosition: "bottom_center",
  autoCheckUpdate: true,
  hasPolisherApiKey: false,
  injectText: false,
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    switch (cmd) {
      case "get_config":
        return Promise.resolve(configResponse);
      case "list_models":
        return Promise.resolve([]);
      case "resolve_model":
        return Promise.resolve(null);
      default:
        return Promise.resolve(null);
    }
  }),
}));

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("0.0.0-test") }));

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

vi.mock("@tauri-apps/plugin-dialog", () => ({ message: vi.fn() }));

vi.mock("../config/catalog", () => ({ loadCatalog: loadCatalogMock }));

vi.mock("../i18n", () => ({
  useTranslation: () => ({ t: (k: string) => k, lang: "zh", setLang: vi.fn() }),
}));

vi.mock("../ThemeContext", () => ({
  useTheme: () => ({ themePref: "system", setTheme: vi.fn() }),
}));

vi.mock("../ui-size", () => ({
  getFontSizePref: () => "medium",
  getWindowSizePref: () => "medium",
  setFontSizePref: vi.fn(),
  setWindowSizePref: vi.fn(),
}));

describe("Settings 供应商目录自动加载", () => {
  it("挂载即拉取目录一次，并把结果渲染进供应商摘要", async () => {
    loadCatalogMock.mockReset();
    loadCatalogMock.mockResolvedValue([examplePreset]);

    render(<Settings />);

    await waitFor(() => {
      expect(loadCatalogMock).toHaveBeenCalledTimes(1);
    });
    // 目录结果按 polishApiBaseUrl 匹配为当前供应商，摘要区显示其名字。
    // Catalog entries match the saved polishApiBaseUrl and surface in the provider summary.
    await waitFor(() => {
      expect(screen.getByText("Example Provider")).toBeTruthy();
    });
  });
});
