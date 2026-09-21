import { describe, it, expect } from "vitest";
import { saveRequestBody, normalizeConfig, AppConfig } from "./useConfigForm";

describe("saveRequestBody", () => {
  const base: AppConfig = {
    keyName: "AltRight",
    linuxEvdevCode: 100,
    windowsVkCode: null,
    language: "zh",
    model: "sense-voice",
    polishLevel: "none",
    polishModel: "",
    polishApiBaseUrl: "",
    polishProtocol: "openai",
    polishThinkingLevel: "off",
    guiLanguage: "zh",
    overlayPosition: "bottom_center",
    autoCheckUpdate: true,
    injectText: false,
    polisherApiKey: "",
    hasPolisherApiKey: false,
  };

  it("includes autoCheckUpdate in the request body", () => {
    const result = saveRequestBody({ ...base, autoCheckUpdate: false });
    expect(result).toHaveProperty("autoCheckUpdate", false);
  });

  it("includes injectText in the request body", () => {
    const result = saveRequestBody({ ...base, injectText: true });
    expect(result).toHaveProperty("injectText", true);
  });

  it("includes overlayPosition in the request body", () => {
    const result = saveRequestBody({ ...base, overlayPosition: "top_center" });
    expect(result).toHaveProperty("overlayPosition", "top_center");
  });

  it("includes polishProtocol in the request body", () => {
    const result = saveRequestBody({ ...base, polishProtocol: "anthropic" });
    expect(result).toHaveProperty("polishProtocol", "anthropic");
  });

  it("includes polishThinkingLevel in the request body", () => {
    const result = saveRequestBody({ ...base, polishThinkingLevel: "medium" });
    expect(result).toHaveProperty("polishThinkingLevel", "medium");
  });

  it("includes polishApiKey when polisherApiKey is non-empty", () => {
    const result = saveRequestBody({ ...base, polisherApiKey: "sk-def" });
    expect(result).toHaveProperty("polishApiKey", "sk-def");
  });

  it("excludes polishApiKey when polisherApiKey is empty", () => {
    const result = saveRequestBody({ ...base, polisherApiKey: "" });
    expect(result).not.toHaveProperty("polishApiKey");
  });
});

describe("normalizeConfig", () => {
  it("sets undefined evdev field to null", () => {
    const input = {
      keyName: "AltRight",
      linuxEvdevCode: undefined,
      windowsVkCode: undefined,
      language: "zh",
      model: "sense-voice",
      polishLevel: "none",
      polishModel: "",
      polishApiBaseUrl: "",
      guiLanguage: "zh",
      polisherApiKey: "",
      hasPolisherApiKey: false,
    } as unknown as AppConfig;
    const result = normalizeConfig(input);
    expect(result.linuxEvdevCode).toBeNull();
  });

  it("clears API key in normalized output", () => {
    const input: AppConfig = {
      keyName: "AltRight",
      linuxEvdevCode: 100,
      windowsVkCode: 0xa5,
      language: "zh",
      model: "sense-voice",
      polishLevel: "none",
      polishModel: "",
      polishApiBaseUrl: "",
      polishProtocol: "openai",
      polishThinkingLevel: "off",
      guiLanguage: "zh",
      overlayPosition: "bottom_center",
      autoCheckUpdate: true,
      injectText: false,
      polisherApiKey: "secret",
      hasPolisherApiKey: true,
    };
    const result = normalizeConfig(input);
    expect(result.polisherApiKey).toBe("");
    expect(result.hasPolisherApiKey).toBe(true);
  });
});
