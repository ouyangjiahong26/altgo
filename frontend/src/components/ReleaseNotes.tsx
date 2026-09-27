import { useMemo, type ReactNode } from "react";

/**
 * 更新说明的受限 Markdown 渲染。
 *
 * 输入是自家发版链路产出的受控文本：`extract-release-notes.sh` 从 CHANGELOG.md
 * 截取当前版本小节，`merge-updater-json.sh` 把它写进 latest.json 的 `notes`，
 * 经 `check_update` 的 `body` 字段到达前端。因此只实现实际出现的语法子集，
 * 未识别的行保守降级为纯文本段落，不解析 HTML、不产生 HTML 字符串。
 */

export interface NotesListItem {
  text: string;
  children: string[];
}

export type NotesBlock =
  | { kind: "heading"; text: string }
  | { kind: "list"; items: NotesListItem[] }
  | { kind: "paragraph"; text: string };

const BULLET_RE = /^(\s*)[-*]\s+(.*)$/;
const HEADING_RE = /^(#{1,6})\s+(.*)$/;
const RULE_RE = /^(?:-{3,}|\*{3,}|_{3,})$/;

/**
 * 纯函数：把更新说明文本切成块。
 */
export function parseReleaseNotes(md: string): NotesBlock[] {
  const blocks: NotesBlock[] = [];
  let list: { kind: "list"; items: NotesListItem[] } | null = null;

  for (const raw of (md ?? "").split(/\r?\n/)) {
    const line = raw.trimEnd();
    const trimmed = line.trim();

    // 空行结束当前列表。
    if (trimmed === "") {
      list = null;
      continue;
    }

    const bullet = BULLET_RE.exec(line);
    if (bullet) {
      const text = bullet[2].trim();
      if (!list) {
        list = { kind: "list", items: [] };
        blocks.push(list);
      }
      if (bullet[1].length >= 2 && list.items.length > 0) {
        list.items[list.items.length - 1].children.push(text);
      } else {
        list.items.push({ text, children: [] });
      }
      continue;
    }

    list = null;

    const heading = HEADING_RE.exec(trimmed);
    if (heading) {
      // 一级/二级标题是版本元信息（`# altgo vX.Y.Z`、`## vX.Y.Z (日期)`），
      // 版本号由窗口头部展示，跳过以免重复。
      if (heading[1].length > 2) {
        blocks.push({ kind: "heading", text: heading[2].trim() });
      }
      continue;
    }

    // 分隔线之后的内容是发版脚本自动追加的样板（与上一 tag 的对比链接等），
    // 不属于版本说明本身，遇到即结束解析。
    if (RULE_RE.test(trimmed)) break;

    blocks.push({ kind: "paragraph", text: trimmed });
  }

  return blocks;
}

const INLINE_RE = /(\*\*[^*]+\*\*|`[^`]+`)/;

/**
 * 纯函数：条目的展示文本——只取“：”之前的小标题。
 *
 * CHANGELOG 的条目是“**小标题**：长描述”，窄窗里把长描述全铺开会盖过标题，
 * 读者也只需知道改了什么；细节仍在 CHANGELOG.md 与 GitHub Release 页。
 * 没有“：”的条目原样保留。
 */
export function entryTitle(text: string): string {
  const colon = text.indexOf("：");
  return colon > 0 ? text.slice(0, colon) : text;
}

/**
 * 纯函数：把一行内联语法切成 React 节点（`**粗体**`、`` `代码` ``），
 * 其余文本原样保留为字符串节点。粗体内部再解析一次——真实 CHANGELOG 里有
 * `**\`build.ps1\` 的检查**` 这类写法，否则反引号会原样显示。
 */
export function renderInline(text: string): ReactNode[] {
  return text
    .split(INLINE_RE)
    .filter((part) => part !== "")
    .map((part, index) => {
      if (part.length > 4 && part.startsWith("**") && part.endsWith("**")) {
        return <strong key={index}>{renderInline(part.slice(2, -2))}</strong>;
      }
      if (part.length > 2 && part.startsWith("`") && part.endsWith("`")) {
        return <code key={index}>{part.slice(1, -1)}</code>;
      }
      return part;
    });
}

export default function ReleaseNotes({ md }: { md?: string | null }) {
  const blocks = useMemo(() => (md ? parseReleaseNotes(md) : []), [md]);

  // 无内容时不渲染任何节点，空态文案由页面负责。
  if (blocks.length === 0) return null;

  return (
    <div className="release-notes">
      {blocks.map((block, index) => {
        if (block.kind === "heading") {
          return (
            <h4 className="release-notes-heading" key={index}>
              {renderInline(block.text)}
            </h4>
          );
        }
        if (block.kind === "list") {
          return (
            <ul className="release-notes-list" key={index}>
              {block.items.map((item, itemIndex) => (
                <li className="release-notes-item" key={itemIndex}>
                  {renderInline(entryTitle(item.text))}
                  {item.children.length > 0 && (
                    <ul className="release-notes-sublist">
                      {item.children.map((child, childIndex) => (
                        <li className="release-notes-subitem" key={childIndex}>
                          {renderInline(entryTitle(child))}
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
          );
        }
        return (
          <p className="release-notes-paragraph" key={index}>
            {renderInline(block.text)}
          </p>
        );
      })}
    </div>
  );
}
