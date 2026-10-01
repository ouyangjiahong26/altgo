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
});
