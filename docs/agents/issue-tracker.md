# Issue tracker: GitHub

本仓库的 issue 和任务要求存放在 GitHub Issues 中。所有操作使用 `gh` CLI。

## 约定

- 创建 issue：`gh issue create --title "..." --body "..."`。多行正文用 heredoc。
- AI 贡献标记：AI 提交的 issue 与 PR 标题以 `[AI Generated][<类型>]` 开头（类型是大写标签，如 `[FIX]`、`[DOCS]`）。AI 写的评论在最后一行加标记，语言跟正文一致，写明 harness 与模型：中文用 `> 本评论由 <Harness> 的 <模型> 完成。` 或 `> 本评论由 <Harness> 的 <模型> 辅助完成。`，英文用 `> Written by <Harness> using <model>.` 或 `> Drafted by <Harness> using <model>, edited by a human.`
- 读取 issue：`gh issue view <number> --json number,title,body,labels,comments,state,author,createdAt`，用 `jq` 过滤评论，同时获取标签和元数据。
- 列出 issue：`gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`，按需加 `--label`、`--state` 过滤。
- 评论 issue：`gh issue comment <number> --body "..."`
- 添加 / 移除标签：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- 关闭：`gh issue close <number> --comment "..."`

从 `git remote -v` 推断仓库，`gh` 在 clone 内运行时自动识别。

## issue / PR / 评论的格式

文字质量由注入 `AGENTS.md` 的写作要求管，本节管结构与流程。分类、标签与推进面板见下文各节，本节不重复。

### 提案先行

- 动手写代码之前先发 issue 提案，末尾写明是否已有可用实现，得到维护者回应再动手。错别字、修法唯一的明显 bug 等小改动可以直接提 PR。
- 发 issue 前先搜：文档、README、既有 issue 与 PR。已有答案或已有进行中的讨论，评论到既有帖子里，不发新帖。

### issue

- 标题说清对象和目的，不写方案。
- 正文三段：Problem 讲现状与问题，用文档、代码行、真实实例作证据，不写抽象抱怨。Proposal 讲打算怎么改，具体到可判断（关键参数与默认值、保守用户退回现状的开关、为什么安全），有多个选项时给推荐和理由。上下文列出相关的前序 issue、commit、ADR。
- 需要维护者决策的点单独列出（待决策），没写就视为没有。后续在 PR 里落地的，合并前回 issue 评论决策结果。决策记在 issue，不记在 PR 描述里。

### PR

- 标题回应 issue 标题：解决哪个 issue，就用它的词说同一件事，不按实现手段另起名字。
- Closes 置顶，其后五段：Summary 一段话讲做了什么。Motivation 为什么值得做，呼应 issue 的 Problem。Changes 逐个文件讲改动点与动机。Why this is safe 说明不变量未动、退路与边界情况。Test plan 逐项报结果，只报事实（命令、数字、截图），既存失败标注 pre-existing。
- 与 issue 方案不一致的落法单独交代。对不上账的，`Closes` 改 `Refs`。

### 评论

- 按身份说话：维护者评审外部 PR 时给结论（我认可 / 需要改 / 此条撤回），不用商量式的口气，也不替对方解释动机。
- 先立主线再展开：先说这条 issue / PR 在解决什么问题，用文件职责与流程对账作支撑。修了什么，先讲因果再讲改法。
- 收尾简短闭环。靠分析说服，不靠篇幅。不用引号包裹术语，不用破折号，不用装饰性符号。
- 不代改他人提交的内容：别人的 issue 正文、PR 描述、评论有问题，以评论提出请作者自己改。维护者直接改的只有 PR 标题、标签、里程碑这类元数据。
- AI 生成的 issue、PR 与评论按“约定”一节的标记规则标识。

## Pull request 作为分诊渠道

PR 作为请求渠道：否。本仓库只分诊 issue，外部 PR 不进入请求队列。

设为 `是` 时，PR 与 issue 走相同的标签和状态，使用 `gh pr` 等价命令：

- 读取 PR：`gh pr view <number> --json number,title,body,labels,author,comments,state,createdAt`，`gh pr diff <number>` 看 diff。
- 列出待分诊的外部 PR：`gh pr list --state open --json number,title,body,labels,author,comments`，再对每个 PR 运行 `gh api repos/<owner>/<repo>/pulls/<number> --jq .author_association`，只保留 `CONTRIBUTOR`、`FIRST_TIME_CONTRIBUTOR` 或 `NONE` 的（去掉 `OWNER`/`MEMBER`/`COLLABORATOR`）。
- 评论 / 打标签 / 关闭：`gh pr comment`、`gh pr edit --add-label`/`--remove-label`、`gh pr close`。

GitHub 的 issue 和 PR 共享编号空间，所以 `#42` 可能是其中任一，用 `gh pr view 42 --json number,state` 确认，回退到 `gh issue view 42 --json number,state`。

## 当技能说“发布到 issue tracker”时

创建一个 GitHub issue。

## 当技能说“获取相关工单”时

运行 `gh issue view <number> --json number,title,body,labels,comments,state,author,createdAt`。

## GitHub Project

使用 Project：是。

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

`Priority`（`P0`–`P3`）、`Start Date` 等其他字段按同样格式记录字段 ID 与选项 ID。没记录的字段，技能不写。状态迁移规则见 `/github-project`。
