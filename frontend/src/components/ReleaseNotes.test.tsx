import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import ReleaseNotes, { parseReleaseNotes, renderInline, entryTitle } from "./ReleaseNotes";

// 近似 release_notes.md 的真实形态：`# altgo vX` + `## vX (日期)` + 分类 + 条目 + 分隔线 + 对比链接。
const SAMPLE = [
  "# altgo v2.6.18",
  "",
  "## v2.6.18 (2026-09-27)",
  "",
  "### Changed",
  "",
  "- **悬浮窗精简为单行极简胶囊**：胶囊缩小为 280×64（#168）。",
  "- **启用模型改为窄补丁 `save_config {model}`**：主窗改为 440×680 窄长两页布局（#170）。",
  "  - 子条目：历史页并入主页。",
  "",
  "### Fixes",
  "",
  "- **更新渠道指向新仓库名**：仓库改名 `ouyangjiahong26/altgo`（#169）。",
  "",
  "这是一行未识别文本，降级为段落。",
  "",
  "---",
  "",
  "**与 v2.6.17 以来的提交对比：** https://github.com/ouyangjiahong26/altgo/compare/v2.6.17...v2.6.18",
].join("\n");

describe("parseReleaseNotes", () => {
  it("把 ### 分类渲染为标题块，跳过一二级版本标题与分隔线", () => {
    const blocks = parseReleaseNotes(SAMPLE);
    const headings = blocks.filter((b) => b.kind === "heading");

    expect(headings.map((h) => (h.kind === "heading" ? h.text : ""))).toEqual([
      "Changed",
      "Fixes",
    ]);
    // 版本行（`# altgo v2.6.18`、`## v2.6.18 (2026-09-27)`）不产生任何块。
    expect(JSON.stringify(blocks)).not.toContain("altgo v2.6.18");
    expect(JSON.stringify(blocks)).not.toContain("(2026-09-27)");
  });

  it("解析顶层条目与两个空格缩进的子条目", () => {
    const blocks = parseReleaseNotes(SAMPLE);
    const changed = blocks.find((b) => b.kind === "list");
    if (changed?.kind !== "list") throw new Error("expected a list block");

    expect(changed.items).toHaveLength(2);
    expect(changed.items[0].text).toContain("悬浮窗精简为单行极简胶囊");
    expect(changed.items[0].children).toEqual([]);
    expect(changed.items[1].children).toEqual(["子条目：历史页并入主页。"]);

    // 空行分隔的两个分类各自成块（中间隔着 `### Fixes` 标题块）。
    const fixes = blocks.filter((b) => b.kind === "list")[1];
    if (fixes.kind !== "list") throw new Error("expected a second list block");
    expect(fixes.items).toHaveLength(1);
  });

  it("未识别的行降级为纯文本段落", () => {
    const blocks = parseReleaseNotes(SAMPLE);
    const paragraphs = blocks.filter((b) => b.kind === "paragraph");
    expect(paragraphs).toHaveLength(1);
    expect(paragraphs[0].kind === "paragraph" && paragraphs[0].text).toBe(
      "这是一行未识别文本，降级为段落。"
    );
  });

  it("分隔线及其后的样板行不进入结果", () => {
    const blocks = parseReleaseNotes(SAMPLE);
    expect(JSON.stringify(blocks)).not.toContain("提交对比");
    expect(JSON.stringify(blocks)).not.toContain("compare/v2.6.17");
  });

  it("空字符串与空白串产出空块列表", () => {
    expect(parseReleaseNotes("")).toEqual([]);
    expect(parseReleaseNotes("   \n\n  ")).toEqual([]);
  });
});

describe("entryTitle", () => {
  it("只保留冒号前的小标题，没有冒号或冒号开头时原样返回", () => {
    expect(entryTitle("**标题**：很长的描述（#1）。")).toBe("**标题**");
    expect(entryTitle("没有冒号的条目")).toBe("没有冒号的条目");
    expect(entryTitle("：开头就是冒号")).toBe("：开头就是冒号");
  });
});

describe("renderInline", () => {
  it("区分粗体、行内代码与普通文本", () => {
    const rendered = render(
      <div>{renderInline("**标题**：改用 `altgo.toml` 配置。")}</div>
    ).container;

    expect(rendered.querySelector("strong")?.textContent).toBe("标题");
    expect(rendered.querySelector("code")?.textContent).toBe("altgo.toml");
    expect(rendered.textContent).toBe("标题：改用 altgo.toml 配置。");
  });

  it("粗体内部的代码标记仍然生效", () => {
    const rendered = render(
      <div>{renderInline("**`build.ps1` 的残留检查**")}</div>
    ).container;

    expect(rendered.querySelector("strong")?.textContent).toBe("build.ps1 的残留检查");
    expect(rendered.querySelector("strong code")?.textContent).toBe("build.ps1");
  });

  it("不成对的行内标记保持原文", () => {
    const rendered = render(<div>{renderInline("**未闭合 `混合")}</div>).container;
    expect(rendered.querySelector("strong")).toBeNull();
    expect(rendered.querySelector("code")).toBeNull();
    expect(rendered.textContent).toBe("**未闭合 `混合");
  });
});

describe("ReleaseNotes", () => {
  it("每个条目只渲染冒号前的小标题，分类标题与嵌套条目同样处理", () => {
    const container = render(<ReleaseNotes md={SAMPLE} />).container;

    expect(
      Array.from(container.querySelectorAll(".release-notes-heading")).map((h) => h.textContent)
    ).toEqual(["Changed", "Fixes"]);

    const itemTitles = Array.from(container.querySelectorAll(".release-notes-item")).map(
      (li) => li.querySelector("strong")?.textContent
    );
    expect(itemTitles).toEqual([
      "悬浮窗精简为单行极简胶囊",
      "启用模型改为窄补丁 save_config {model}",
      "更新渠道指向新仓库名",
    ]);
    expect(container.querySelector(".release-notes-subitem")?.textContent).toBe("子条目");
    expect(container.querySelectorAll(".release-notes-item strong code")).toHaveLength(1);

    // 冒号后的描述不进入渲染，样板尾巴与版本行同样被过滤。
    expect(container.textContent).not.toContain("主窗改为 440×680");
    expect(container.textContent).not.toContain("ouyangjiahong26/altgo");
    expect(container.textContent).not.toContain("altgo v2.6.18");
    expect(container.textContent).not.toContain("提交对比");
    expect(container.querySelector(".release-notes-paragraph")?.textContent).toBe(
      "这是一行未识别文本，降级为段落。"
    );
  });

  it("body 缺失或为空时不渲染任何节点", () => {
    expect(render(<ReleaseNotes md="" />).container.querySelector(".release-notes")).toBeNull();
    expect(render(<ReleaseNotes />).container.innerHTML).toBe("");
    expect(
      render(<ReleaseNotes md={undefined} />).container.querySelector(".release-notes")
    ).toBeNull();
  });
});
