import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProviderPresetSelector } from "./ProviderPresetSelector";
import type { ProviderPreset } from "../config/modelPresets";

const openUrlMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrlMock(url),
}));

const preset: ProviderPreset = {
  name: "Example Provider",
  websiteUrl: "https://example.com",
  apiKeyUrl: "https://example.com/key",
  apiBaseUrl: "https://api.example.com/v1",
  category: "custom",
  modelTypes: ["polisher"],
  apiFormat: "openai",
  defaultModel: "example-model",
  models: [
    {
      model: "example-model",
      displayName: "Example Model",
      recommended: true,
    },
  ],
};

const t = (key: string) => key;
const baseProps = {
  presets: [preset],
  modelType: "polisher" as const,
  currentApiBaseUrl: "",
  currentModel: "",
  lang: "en",
  t,
  onSelect: vi.fn(),
};

describe("ProviderPresetSelector", () => {
  it("keeps the provider list closed until requested", () => {
    const { container } = render(<ProviderPresetSelector {...baseProps} />);

    expect(screen.getByText("settings.add_provider")).toBeTruthy();
    expect(container.querySelector(".provider-preset-dialog")).toBeNull();
  });

  it("opens the picker and selects a recommended model", () => {
    const onSelect = vi.fn();
    render(<ProviderPresetSelector {...baseProps} onSelect={onSelect} />);

    fireEvent.click(screen.getByText("settings.add_provider"));
    fireEvent.click(screen.getByText("Example Provider"));
    fireEvent.click(screen.getByText("Example Model"));

    expect(onSelect).toHaveBeenCalledWith(preset, preset.models[0]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes with Escape and restores focus to the trigger", () => {
    render(<ProviderPresetSelector {...baseProps} />);
    const trigger = screen.getByText("settings.add_provider").closest("button");

    fireEvent.click(trigger!);
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("外链点击经 opener 打开，不交给 webview 原生新窗口导航", () => {
    render(<ProviderPresetSelector {...baseProps} />);

    fireEvent.click(screen.getByText("settings.add_provider"));
    fireEvent.click(screen.getByText("Example Provider"));
    fireEvent.click(screen.getByText("settings.get_api_key"));

    // target=_blank 在 WebView 里走 create 信号被静默吞掉，必须走 opener。
    expect(openUrlMock).toHaveBeenCalledWith("https://example.com/key");
  });

  it("shows catalog loading instead of no-match while the catalog is loading", () => {
    render(<ProviderPresetSelector {...baseProps} presets={[]} catalogLoading />);

    fireEvent.click(screen.getByText("settings.add_provider"));

    expect(screen.getByText("settings.catalog_loading")).toBeTruthy();
    expect(screen.queryByText("settings.no_matching_providers")).toBeNull();
  });

  it("shows catalog error with retry when the fetch failed and no list exists", () => {
    const onRetryCatalog = vi.fn();
    render(
      <ProviderPresetSelector
        {...baseProps}
        presets={[]}
        catalogError="目录拉取失败"
        onRetryCatalog={onRetryCatalog}
      />,
    );

    fireEvent.click(screen.getByText("settings.add_provider"));

    expect(screen.getByText("目录拉取失败")).toBeTruthy();
    expect(screen.queryByText("settings.no_matching_providers")).toBeNull();

    fireEvent.click(screen.getByText("settings.catalog_retry"));
    expect(onRetryCatalog).toHaveBeenCalledTimes(1);
  });

  it("keeps the existing list during a retry after a previous success", () => {
    render(
      <ProviderPresetSelector
        {...baseProps}
        catalogLoading
        catalogError="目录拉取失败"
        onRetryCatalog={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("settings.add_provider"));

    // 已有可用列表时重试进行中保留列表，不进入加载态或错误态。
    expect(screen.getByText("Example Provider")).toBeTruthy();
    expect(screen.queryByText("settings.catalog_loading")).toBeNull();
    expect(screen.queryByText("目录拉取失败")).toBeNull();
  });

  it("shows no-match only when the catalog loaded and search found nothing", () => {
    render(<ProviderPresetSelector {...baseProps} />);

    fireEvent.click(screen.getByText("settings.add_provider"));
    fireEvent.change(screen.getByPlaceholderText("settings.search_providers"), {
      target: { value: "不存在的供应商" },
    });

    expect(screen.getByText("settings.no_matching_providers")).toBeTruthy();
  });
});
