# Issue Tracker：GitHub

本仓库的 issue 和 PRD 以 GitHub issue 的形式存在。所有操作都使用 `gh` CLI。

## 约定

- **创建 issue**：`gh issue create --title "..." --body "..."`。多行正文请使用 heredoc。
- **查看 issue**：`gh issue view <number> --comments`，并通过 `jq` 过滤评论，同时拉取 labels。
- **列出 issue**：`gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`，并视情况添加 `--label` 和 `--state` 过滤条件。
- **评论 issue**：`gh issue comment <number> --body "..."`
- **添加 / 移除标签**：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **关闭**：`gh issue close <number> --comment "..."`
- **AI 贡献标记**：AI 提交的 issue 与 PR 标题以 `[AI Generated][<类型>]` 开头（类型是大写标签，如 `[FIX]`、`[DOCS]`）；AI 写的评论首行用 `> **[AI Generated]** 本评论由 AI 完成。` 或 `> **[AI Assisted]** 本评论由 AI 辅助完成。`

仓库可从 `git remote -v` 推断 —— 在克隆目录中运行 `gh` 会自动识别。

## Pull request 作为分诊渠道

**PR 作为请求渠道：否。** 本仓库不把外部 PR 当作功能请求，`/triage` 只处理 issue。

## GitHub Project

**使用 Project：是。** `/github-project`、`/triage`、`/open-pr`、`/merge-pr` 读取此配置。

| 项 | 值 |
| --- | --- |
| Owner | `ouyangjiahong26` |
| Project 编号 | `5` |
| Project ID | `PVT_kwHOCpw4xM4Bk230` |
| Status 字段 ID | `PVTSSF_lAHOCpw4xM4Bk230zhjl4cg` |

Status 选项 ID：

| 选项 | 选项 ID |
| --- | --- |
| `Inbox` | `6cde2d84` |
| `Backlog` | `dedd3458` |
| `Ready` | `bad61427` |
| `In progress` | `241fd9f2` |
| `In review` | `b05b9926` |
| `Done` | `c5a0afd1` |
| `No action` | `0874aa68` |

状态迁移规则见 `/github-project`。本仓库没有 `Priority`／`Start Date` 自定义字段，故只登记 Status。

## 当技能说 "publish to the issue tracker"

创建一个 GitHub issue。

## 当技能说 "fetch the relevant ticket"

运行 `gh issue view <number> --comments`。

# Issue Tracker: GitHub

This repository keeps its issues and PRDs as GitHub issues. All operations use the `gh` CLI.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **View an issue**: `gh issue view <number> --comments`, filter comments with `jq`, and pull labels too.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`, adding `--label` and `--state` filters as needed.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Add / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`
- **AI contribution marker**: AI-submitted issue and PR titles start with `[AI Generated][<type>]` (an uppercase tag such as `[FIX]`, `[DOCS]`); AI-written comments open with `> **[AI Generated]** 本评论由 AI 完成。` or `> **[AI Assisted]** 本评论由 AI 辅助完成。`

The repo is inferred from `git remote -v` — running `gh` inside the clone just works.

## Pull Requests as a Triage Channel

**PRs as a request channel: no.** This repository does not treat external PRs as feature requests; `/triage` only handles issues.

## GitHub Project

**Use Project: yes.** `/github-project`, `/triage`, `/open-pr` and `/merge-pr` read this configuration.

| Item | Value |
| --- | --- |
| Owner | `ouyangjiahong26` |
| Project number | `5` |
| Project ID | `PVT_kwHOCpw4xM4Bk230` |
| Status field ID | `PVTSSF_lAHOCpw4xM4Bk230zhjl4cg` |

Status option IDs:

| Option | Option ID |
| --- | --- |
| `Inbox` | `6cde2d84` |
| `Backlog` | `dedd3458` |
| `Ready` | `bad61427` |
| `In progress` | `241fd9f2` |
| `In review` | `b05b9926` |
| `Done` | `c5a0afd1` |
| `No action` | `0874aa68` |

Transition rules live in `/github-project`. This repository has no `Priority` or `Start Date` custom fields, so only Status is recorded.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.
