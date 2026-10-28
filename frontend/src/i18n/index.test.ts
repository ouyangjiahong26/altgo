// @vitest-environment jsdom
//
// 界面语言解析与错误码翻译的纯函数行为：resolveUiLang 的偏好直通与系统语言
// 回退、tError 的双语字典命中、占位符插值与未知 code 回退，
// 以及在线识别文案不绑定具体供应商的约束。
import { describe, it, expect, beforeEach } from "vitest";
import { resolveUiLang, tError, translateStatic } from "./index";

function stubNavigatorLanguage(value: string | undefined) {
  Object.defineProperty(navigator, "language", {
    value,
    configurable: true,
  });
}

describe("resolveUiLang", () => {
  it.each([
    ["zh-CN", "zh"],
    ["en-US", "en"],
    [undefined, "en"],
  ])("空偏好跟随系统语言 %s → %s", (sysLang, expected) => {
    stubNavigatorLanguage(sysLang);
    expect(resolveUiLang("")).toBe(expected);
    expect(resolveUiLang(null)).toBe(expected);
    expect(resolveUiLang(undefined)).toBe(expected);
  });

  it("显式偏好直通", () => {
    expect(resolveUiLang("zh")).toBe("zh");
    expect(resolveUiLang("en")).toBe("en");
  });

  it("显式偏好优先于系统语言", () => {
    stubNavigatorLanguage("zh-CN");
    expect(resolveUiLang("en")).toBe("en");
    stubNavigatorLanguage("en-US");
    expect(resolveUiLang("zh")).toBe("zh");
  });
});

describe("tError", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("按语言偏好命中双语字典", () => {
    localStorage.setItem("altgo-lang", "en");
    expect(tError("polisher.rate_limited")).toBe(
      "API rate limited, please try again later."
    );
    localStorage.setItem("altgo-lang", "zh");
    expect(tError("polisher.rate_limited")).toBe("API 请求频率受限，请稍后重试。");
  });

  it("params 插值 {status} 占位符", () => {
    localStorage.setItem("altgo-lang", "zh");
    expect(tError("transcriber.api_error", { status: "429", body: "busy" })).toBe(
      "在线识别 API 错误（HTTP 429）：busy"
    );
  });

  it("未知 code 原样返回 error.<code>", () => {
    localStorage.setItem("altgo-lang", "en");
    expect(tError("no.such_code")).toBe("error.no.such_code");
  });
});

/** 在线识别已泛化为多供应商：引擎选项文案不得再绑定 MiMo。 */
describe("在线识别文案不绑定具体供应商", () => {
  it.each([
    ["zh", "settings.transcriber_backend_online"],
    ["en", "settings.transcriber_backend_online"],
    ["zh", "onboarding.engine_online"],
    ["en", "onboarding.engine_online"],
  ])("%s/%s 不含 MiMo", (lang, key) => {
    const text = translateStatic(lang, key);
    expect(text).not.toMatch(/mimo/i);
    expect(text).toBeTruthy();
  });

  it("中英文案分别为“在线识别”与 Online", () => {
    expect(translateStatic("zh", "settings.transcriber_backend_online")).toBe("在线识别");
    expect(translateStatic("en", "settings.transcriber_backend_online")).toBe("Online");
    expect(translateStatic("zh", "onboarding.engine_online")).toBe("在线识别");
    expect(translateStatic("en", "onboarding.engine_online")).toBe("Online");
  });
});
