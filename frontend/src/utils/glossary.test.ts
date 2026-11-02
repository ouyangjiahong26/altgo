import { describe, expect, it } from "vitest";
import {
  MAX_GLOSSARY_TERMS,
  hasCjk,
  mergeGlossaryTerms,
  splitGlossaryInput,
} from "./glossary";

describe("mergeGlossaryTerms", () => {
  it("按顺序并入新词并保留已有词", () => {
    const result = mergeGlossaryTerms(["地月空间"], ["平动点", "近月空间"]);

    expect(result.terms).toEqual(["地月空间", "平动点", "近月空间"]);
    expect(result.added).toEqual(["平动点", "近月空间"]);
    expect(result.skipped).toEqual([]);
  });

  it("跳过重复、超长与超出条数上限的词并记录原因", () => {
    const existing = Array.from(
      { length: MAX_GLOSSARY_TERMS - 1 },
      (_, index) => `术语${index}`,
    );
    const result = mergeGlossaryTerms(existing, [
      "平动点",
      "术语0",
      "长".repeat(33),
      "新术语",
    ]);

    expect(result.added).toEqual(["平动点"]);
    expect(result.skipped).toEqual([
      { term: "术语0", reason: "duplicate" },
      { term: "长".repeat(33), reason: "too_long" },
      { term: "新术语", reason: "max" },
    ]);
    expect(result.terms).toHaveLength(MAX_GLOSSARY_TERMS);
  });

  it("忽略空白项", () => {
    const result = mergeGlossaryTerms([], ["  ", "地月空间", "\t"]);

    expect(result.terms).toEqual(["地月空间"]);
    expect(result.skipped).toEqual([]);
  });
});

describe("splitGlossaryInput", () => {
  it("按换行与中英文逗号拆分并去掉空白", () => {
    expect(
      splitGlossaryInput(" 地月空间 \n平动点, 近月空间，探测器 "),
    ).toEqual(["地月空间", "平动点", "近月空间", "探测器"]);
  });
});

describe("hasCjk", () => {
  it("识别汉字，纯英文与纯数字返回假", () => {
    expect(hasCjk("地月空间")).toBe(true);
    expect(hasCjk("hello 地月")).toBe(true);
    expect(hasCjk("hello world")).toBe(false);
    expect(hasCjk("12345")).toBe(false);
  });
});
