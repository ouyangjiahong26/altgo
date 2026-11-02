import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AppConfig } from "../../hooks/useConfigForm";
import { GlossarySection } from "./GlossarySection";

const baseConfig: AppConfig = {
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
  polishLevel: "medium",
  polishModel: "",
  polishApiBaseUrl: "https://api.example.com/v1",
  polishProtocol: "openai",
  polishThinkingLevel: "off",
  polishBackend: "remote",
  polishLocalModel: "qwen3-0.6b",
  polishGlossary: [],
  guiLanguage: "zh",
  overlayPosition: "bottom_center",
  autoCheckUpdate: true,
  injectText: false,
  polisherApiKey: "",
  hasPolisherApiKey: false,
};

function renderSection(
  overrides: Partial<AppConfig> = {},
  update = vi.fn(),
) {
  render(
    <GlossarySection
      t={(key) => key}
      config={{ ...baseConfig, ...overrides }}
      update={update}
    />,
  );
  return { update };
}

describe("GlossarySection", () => {
  it("renders terms as chips and removes one", () => {
    const { update } = renderSection({ polishGlossary: ["地月空间", "平动点"] });

    fireEvent.click(
      screen.getByLabelText("settings.polish_glossary_remove 地月空间"),
    );

    expect(update).toHaveBeenCalledWith("polishGlossary", ["平动点"]);
  });

  it("adds a typed term on Enter", () => {
    const { update } = renderSection({ polishGlossary: ["地月空间"] });
    const input = screen.getByPlaceholderText(
      "settings.polish_glossary_add_placeholder",
    );

    fireEvent.change(input, { target: { value: "平动点" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(update).toHaveBeenCalledWith("polishGlossary", [
      "地月空间",
      "平动点",
    ]);
  });

  it("rejects a duplicate term", () => {
    const { update } = renderSection({ polishGlossary: ["地月空间"] });
    const input = screen.getByPlaceholderText(
      "settings.polish_glossary_add_placeholder",
    );

    fireEvent.change(input, { target: { value: "地月空间" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(update).not.toHaveBeenCalled();
    expect(
      screen.getByText("settings.polish_glossary_duplicate"),
    ).toBeTruthy();
  });

  it("pastes multiple lines at once", () => {
    const { update } = renderSection({ polishGlossary: [] });
    const input = screen.getByPlaceholderText(
      "settings.polish_glossary_add_placeholder",
    );

    fireEvent.paste(input, {
      clipboardData: { getData: () => "地月空间\n平动点" },
    });

    expect(update).toHaveBeenCalledWith("polishGlossary", [
      "地月空间",
      "平动点",
    ]);
  });

  it("rejects a term longer than 32 characters", () => {
    const { update } = renderSection({ polishGlossary: [] });
    const input = screen.getByPlaceholderText(
      "settings.polish_glossary_add_placeholder",
    );

    fireEvent.change(input, { target: { value: "长".repeat(33) } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(update).not.toHaveBeenCalled();
    expect(
      screen.getByText("settings.polish_glossary_too_long"),
    ).toBeTruthy();
  });

  it("stops at 50 terms", () => {
    const terms = Array.from({ length: 50 }, (_, index) => `术语${index}`);
    const { update } = renderSection({ polishGlossary: terms });
    const input = screen.getByPlaceholderText(
      "settings.polish_glossary_add_placeholder",
    );

    fireEvent.change(input, { target: { value: "新术语" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(update).not.toHaveBeenCalled();
    expect(screen.getByText("settings.polish_glossary_max")).toBeTruthy();
  });
});
