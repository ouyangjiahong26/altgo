import { describe, expect, it } from "vitest";
import { parseCatalog } from "./catalog";
import type { ProviderPreset } from "./modelPresets";

const sampleCatalog = {
  deepseek: {
    id: "deepseek",
    api: "https://api.deepseek.com",
    models: {
      "deepseek-v4-flash": { id: "deepseek-v4-flash", name: "DeepSeek V4 Flash" },
    },
  },
  zhipuai: {
    api: "https://open.bigmodel.cn/api/paas/v4",
    models: {
      "glm-5.2": {
        id: "glm-5.2",
        name: "GLM-5.2",
        limit: { context: 200000 },
      },
    },
  },
  "zai-coding-plan": {
    api: "https://api.z.ai/api/coding/paas/v4",
    models: { "glm-5.3": { id: "glm-5.3" } },
  },
  anthropic: {
    env: ["ANTHROPIC_API_KEY"],
    models: { "claude-fable-5": { id: "claude-fable-5", name: "Claude Fable 5" } },
  },
  "google-vertex": {
    models: { m: { id: "m" } },
  },
};


describe("parseCatalog", () => {
  it("转换目录条目：补官方端点、按 id 定协议、携带上下文窗口", () => {
    const presets = Object.fromEntries(
      parseCatalog(sampleCatalog).map((preset) => [preset.name, preset]),
    ) as Record<string, ProviderPreset>;

    expect(presets.zhipuai.apiBaseUrl).toBe("https://open.bigmodel.cn/api/paas/v4");
    expect(presets.zhipuai.apiFormat).toBe("openai");
    expect(presets.zhipuai.models[0].contextWindow).toBe(200000);
    expect(presets.zhipuai.models[0].displayName).toBe("GLM-5.2");

    expect(presets["zai-coding-plan"].apiFormat).toBe("anthropic");

    expect(presets.anthropic.apiBaseUrl).toBe("https://api.anthropic.com");
    expect(presets.anthropic.apiFormat).toBe("anthropic");
    expect(presets.anthropic.models[0].model).toBe("claude-fable-5");
  });

  it("目录条目全量返回，无端点的条目跳过", () => {
    const presets = Object.fromEntries(
      parseCatalog(sampleCatalog).map((preset) => [preset.name, preset]),
    ) as Record<string, ProviderPreset>;

    expect(presets.deepseek.apiBaseUrl).toBe("https://api.deepseek.com");
    expect(presets["google-vertex"]).toBeUndefined();
  });

  it("模型缺失显示名时回退到模型 ID，缺省上下文窗口为 undefined", () => {
    const presets = Object.fromEntries(
      parseCatalog(sampleCatalog).map((preset) => [preset.name, preset]),
    ) as Record<string, ProviderPreset>;

    expect(presets["zai-coding-plan"].models[0]).toEqual({
      model: "glm-5.3",
      displayName: "glm-5.3",
      contextWindow: undefined,
    });
  });

  it("非对象输入返回空数组", () => {
    expect(parseCatalog(null)).toEqual([]);
    expect(parseCatalog("nope")).toEqual([]);
  });
});
