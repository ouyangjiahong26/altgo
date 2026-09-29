# Issue Tracker：GitHub

本仓库的 issue 和 PRD 以 GitHub issue 的形式存在。所有操作都使用 `gh` CLI。

## 约定

- **创建 issue**：`gh issue create --title "..." --body "..."`。多行正文请使用 heredoc。
- **查看 issue**：`gh issue view <number> --comments`，并通过 `jq` 过滤评论，同时拉取 labels。
- **列出 issue**：`gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`，并视情况添加 `--label` 和 `--state` 过滤条件。
- **评论 issue**：`gh issue comment <number> --body "..."`
- **添加 / 移除标签**：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **关闭**：`gh issue close <number> --comment "..."`
- **AI 贡献标记**：AI 提交的 issue 与 PR 标题以 `[AI Generated][<类型>]` 开头（类型是大写标签，如 `[FEAT]`、`[FIX]`、`[DOCS]`、`[CHORE]`、`[ENH]`、`[TASK]`、`[RESEARCH]`）；AI 写的评论首行用 `> **[AI Generated]** 本评论由 AI 完成。` 或 `> **[AI Assisted]** 本评论由 AI 辅助完成。`
- **正文写作风格**：PR 与 Issue 正文、AI 生成的评论、commit body 和 CHANGELOG 条目的写作约定（完整叙述、平实句子、少特殊符号、细节不进折叠区）见 `CONTRIBUTING.md` 的“正文写作约定”一节；开单模板已内联问题集引导。

仓库可从 `git remote -v` 推断 —— 在克隆目录中运行 `gh` 会自动识别。

## Pull request 作为分诊渠道

**PR 作为请求渠道：否。** 本仓库只分诊 issue，外部 PR 不进入请求队列；`/triage` 读取此标记。

设为 `是` 时，PR 与 issue 走相同的标签和状态：`gh pr view <number> --comments` 读、`gh pr diff <number>` 看 diff，`gh pr comment` / `gh pr edit --add-label`/`--remove-label` / `gh pr close` 操作；列出待分诊 PR 时按 `authorAssociation` 过滤掉 `OWNER`/`MEMBER`/`COLLABORATOR`。GitHub 的 issue 与 PR 共用编号空间，`#42` 可能是任一，先 `gh pr view 42` 确认。

## 当技能说 “publish to the issue tracker”

创建一个 GitHub issue。

## 当技能说 “fetch the relevant ticket”

运行 `gh issue view <number> --comments`。

## GitHub Project

看板：[altgo](https://github.com/users/ouyangjiahong26/projects/5)（owner `ouyangjiahong26`，project number `5`）。`/github-project`、`/triage`、`/open-pr`、`/merge-pr` 读取此配置。

| 项 | 值 |
| --- | --- |
| Owner | `ouyangjiahong26` |
| Project 编号 | `5` |
| Project ID | `PVT_kwHOCpw4xM4Bk230` |
| Status 字段 ID | `PVTSSF_lAHOCpw4xM4Bk230zhjl4cg` |

Status 是工作状态（label 只表达分类、领域或分诊角色）。选项与迁移含义：

| 选项 | 选项 ID | 含义 |
| --- | --- | --- |
| `Inbox` | `6cde2d84` | 新到，待分诊 |
| `Backlog` | `dedd3458` | 已确认，未排期 |
| `Ready` | `bad61427` | 已排期，可开工 |
| `In progress` | `241fd9f2` | 实现中 |
| `In review` | `b05b9926` | 等评审 |
| `Done` | `c5a0afd1` | 完成，对应 Issue 关闭原因 Completed |
| `No action` | `0874aa68` | 不处理，对应关闭原因 Not planned |

本仓库没有 `Priority`／`Start Date` 自定义字段，故只登记 Status。状态迁移规则见 `/github-project`。

写入流程（先读 item 现值再写，避免重复添加或覆盖未知字段）：

```bash
gh project item-list 5 --owner ouyangjiahong26 --format json      # 先查已有 item
gh project item-add 5 --owner ouyangjiahong26 --url <issue-or-pr-url>
gh project item-edit --id <item-id> --project-id PVT_kwHOCpw4xM4Bk230 \
  --field-id PVTSSF_lAHOCpw4xM4Bk230zhjl4cg --single-select-option-id b05b9926
gh project item-list 5 --owner ouyangjiahong26 --format json      # 写完复核
```
