import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PolishingSection } from "./PolishingSection";
import type { AppConfig } from "../../hooks/useConfigForm";
import type { PolishModelEntry } from "../../hooks/usePolishModelManager";
import type { PolishRuntimeStatus } from "../../hooks/usePolishRuntime";

const baseConfig: AppConfig = {
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
  polishLevel: "medium",
  polishModel: "",
  polishApiBaseUrl: "https://api.example.com/v1",
  polishProtocol: "openai",
  polishThinkingLevel: "off",
  polishBackend: "remote",
  polishLocalModel: "qwen3-0.6b",
  guiLanguage: "zh",
  overlayPosition: "bottom_center",
  autoCheckUpdate: true,
  injectText: false,
  polisherApiKey: "",
  hasPolisherApiKey: false,
};

const polishModels: PolishModelEntry[] = [
  { name: "qwen3-0.6b", description: "小体积本地模型", sizeBytes: 400 * 1024 * 1024, downloaded: true },
  { name: "qwen3-1.7b", description: "更高质量的本地模型", sizeBytes: 1100 * 1024 * 1024, downloaded: false },
];

const runtimeStatus: PolishRuntimeStatus = {
  supported: true,
  installed: false,
  running: false,
  build: "",
  archiveSize: 0,
};

function renderSection(
  overrides: Partial<AppConfig> = {},
  onDownload = vi.fn(),
  status: PolishRuntimeStatus = runtimeStatus,
  onRedownload = vi.fn(),
) {
  const onUseLocalModel = vi.fn();
  render(
    <PolishingSection
      t={(k) => k}
      lang="zh"
      config={{ ...baseConfig, ...overrides }}
      update={vi.fn()}
      setPolishBackend={vi.fn()}
      localPolish={{
        saving: false,
        models: polishModels,
        downloading: null,
        getDownloadProgress: () => ({ percent: 0, connecting: true }),
        onUseLocalModel,
        onDownloadAndUse: vi.fn(),
        onDeleteModel: vi.fn(),
        runtime: {
          status,
          downloading: false,
          progress: { downloaded: 0, total: 0 },
          onDownload,
          onRedownload,
        },
      }}
      catalogPresets={[]}
      catalogLoading={false}
      catalogError={null}
      onRetryCatalog={vi.fn()}
      testing={false}
      testResult={null}
      onTestConnection={vi.fn()}
      clearingKey={false}
      onClearApiKey={vi.fn()}
      polishOpen
      setPolishOpen={vi.fn()}
      polishAdvancedOpen={false}
      setPolishAdvancedOpen={vi.fn()}
    />,
  );
  return { onUseLocalModel, onDownload, onRedownload };
}

describe("PolishingSection 润色引擎分支", () => {
  it("本地分支渲染模型卡与运行时行，不渲染云端密钥区", () => {
    renderSection({ polishBackend: "local" });

    expect(screen.getByText("qwen3-0.6b")).toBeTruthy();
    expect(screen.getByText("qwen3-1.7b")).toBeTruthy();
    expect(screen.getByText("settings.polish_runtime")).toBeTruthy();
    expect(screen.getByText("settings.polish_runtime_download")).toBeTruthy();
    expect(screen.queryByText("settings.api_key")).toBeNull();
  });

  it("本地分支点击下载运行时调用回调", async () => {
    const { onDownload } = renderSection({ polishBackend: "local" });

    fireEvent.click(screen.getByText("settings.polish_runtime_download"));

    await waitFor(() => expect(onDownload).toHaveBeenCalled());
  });

  it("运行时已安装时按钮接重新下载", async () => {
    const { onRedownload } = renderSection(
      { polishBackend: "local" },
      vi.fn(),
      { ...runtimeStatus, installed: true },
    );

    expect(screen.getByText("settings.polish_runtime_redownload")).toBeTruthy();
    fireEvent.click(screen.getByText("settings.polish_runtime_redownload"));

    await waitFor(() => expect(onRedownload).toHaveBeenCalled());
  });

  it("云端分支渲染密钥与档位，不渲染本地模型卡与运行时行", () => {
    renderSection({ polishBackend: "remote" });

    expect(screen.getByText("settings.api_key")).toBeTruthy();
    expect(screen.getByText("settings.polish_level")).toBeTruthy();
    expect(screen.queryByText("qwen3-0.6b")).toBeNull();
    expect(screen.queryByText("settings.polish_runtime")).toBeNull();
  });
});
