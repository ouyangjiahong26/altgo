# Issue tracker: 极狐 GitLab

本仓库的代码、issue 与合并请求都在极狐 GitLab 上管理，项目为 `https://jihulab.com/cislunarspace/altgo`。所有操作走 GitLab REST API v4，用 `curl` 加 `PRIVATE-TOKEN`。`gh` 只认 GitHub，本仓库不使用。

## 约定

凭据取个人访问令牌，范围 `api`，存放在 `~/.config/jihulab/token`（权限 600），命令里用 `$(cat ~/.config/jihulab/token)` 读取。令牌不写进仓库、不进聊天记录、不贴进 issue 与 MR 正文。

每个 shell 先定义下面三个变量与一个函数，后文命令都依赖它们：

```bash
API=https://jihulab.com/api/v4
PROJECT=cislunarspace%2Faltgo
TOKEN=$(cat ~/.config/jihulab/token)
api() { curl -sS --header "PRIVATE-TOKEN: $TOKEN" "$@"; }
```

`PROJECT` 用 URL 编码的完整路径，不写数字项目 ID，避免迁移后 ID 变化。列表接口 `per_page` 上限 100，翻页看响应头 `X-Next-Page`，非空就带 `page=<值>` 继续取。

- 创建 issue：`api --request POST --data-urlencode "title=..." --data-urlencode "description=..." "$API/projects/$PROJECT/issues"`。多行正文用 heredoc 传 `description`。
- 读取 issue：`api "$API/projects/$PROJECT/issues/<iid>" | jq '{iid, title, description, labels, state, author: .author.username, created_at}'`。
- 读取评论：`api "$API/projects/$PROJECT/issues/<iid>/notes?per_page=100" | jq '.[] | {author: .author.username, body, created_at}'`。标签与元数据用读取 issue 那一条。
- 列出 issue：`api "$API/projects/$PROJECT/issues?state=opened&per_page=100" | jq '.[] | {iid, title, labels, author: .author.username}'`。按标签筛用 `labels=needs-triage`，多个标签用逗号分隔。
- 评论 issue：`api --request POST --data-urlencode "body=..." "$API/projects/$PROJECT/issues/<iid>/notes"`。
- 添加 / 移除标签：`api --request PUT --data-urlencode "add_labels=needs-triage" --data-urlencode "remove_labels=wontfix" "$API/projects/$PROJECT/issues/<iid>"`。
- 关闭：先按需发评论，再 `api --request PUT --data-urlencode "state_event=close" "$API/projects/$PROJECT/issues/<iid>"`。

项目路径固定为 `cislunarspace/altgo`，与 `git remote -v` 的 origin 一致。GitLab 的 issue 与 merge request 编号各自独立，`#42` 可能同时指向两者，读之前先确定走 `/issues/42` 还是 `/merge_requests/42`。

## issue / MR / 评论的格式

文字质量由注入 `AGENTS.md` 的写作要求管，本节管结构与流程。分类、标签与推进看板见下文各节，本节不重复。

### 提案先行

- 动手写代码之前先发 issue 提案，末尾写明是否已有可用实现，得到维护者回应再动手。错别字、修法唯一的明显 bug 等小改动可以直接提 MR。
- 发 issue 前先搜：文档、README、既有 issue 与 MR。已有答案或已有进行中的讨论，评论到既有帖子里，不发新帖。

### issue

- 标题说清对象和目的，不写方案。
- 正文三段：Problem 讲现状与问题，用文档、代码行、真实实例作证据，不写抽象抱怨。Proposal 讲打算怎么改，具体到可判断（关键参数与默认值、保守用户退回现状的开关、为什么安全），有多个选项时给推荐和理由。上下文列出相关的前序 issue、commit、ADR。
- 需要维护者决策的点单独列出（待决策），没写就视为没有。后续在 MR 里落地的，合并前回 issue 评论决策结果。决策记在 issue，不记在 MR 描述里。

### MR

- 标题回应 issue 标题：解决哪个 issue，就用它的词说同一件事，不按实现手段另起名字。标题只写改动内容，不带决策过程痕迹（如“方案 A/B”），AI 生成的 MR 按约定加前缀。
- Closes 置顶，其后五段：Summary 一段话讲做了什么。Motivation 为什么值得做，呼应 issue 的 Problem。Changes 逐个文件讲改动点与动机。Why this is safe 说明不变量未动、退路与边界情况。Test plan 逐项报结果，只报事实（命令、数字、截图），既存失败标注 pre-existing。
- 与 issue 方案不一致的落法单独交代。对不上账的，`Closes` 改 `Refs`。GitLab 只对同项目 issue 解析 `Closes #<iid>`，跨项目要写全 `Closes cislunarspace/altgo#<iid>`。
- 未完成时用标题前缀 `Draft: ` 或界面上的 Draft 开关，接口返回的 `draft` 字段随之为 `true`。

### 评论

- 按身份说话：维护者评审外部 MR 时给结论（我认可 / 需要改 / 此条撤回），不用商量式的口气，也不替对方解释动机。
- 先立主线再展开：先说这条 issue / MR 在解决什么问题，用文件职责与流程对账作支撑。修了什么，先讲因果再讲改法。
- 收尾简短闭环。靠分析说服，不靠篇幅。不用引号包裹术语，不用破折号，不用装饰性符号。
- 不代改他人提交的内容：别人的 issue 正文、MR 描述、评论有问题，以评论提出请作者自己改。维护者直接改的只有 MR 标题、标签、里程碑这类元数据。
- AI 生成的 issue、MR 与评论按“约定”一节的标记规则标识。

## 合并请求的状态与合并

- 读 MR：`api "$API/projects/$PROJECT/merge_requests/<iid>" | jq '{iid, title, state, draft, has_conflicts, detailed_merge_status, target_branch, source_branch, sha, merged_at, author: .author.username}'`。
- 看 diff：`api "$API/projects/$PROJECT/merge_requests/<iid>/changes"`。
- 合并：`api --request PUT --data-urlencode "squash=true" --data-urlencode "squash_commit_message=<提交标题与正文>" --data-urlencode "should_remove_source_branch=true" --data-urlencode "sha=<当前 head>" "$API/projects/$PROJECT/merge_requests/<iid>/merge"`。`sha` 用于防止合并期间分支被推新提交。`squash_commit_message` 必须显式给出，否则 GitLab 用 MR 标题自动生成，AI 标记会进历史。
- 合并后复核：重新读 MR，`state` 为 `merged` 且 `merged_at` 非空才算落地。失败时读响应体原文，不做清理。
- 删除远端源分支：合并时 `should_remove_source_branch=true` 已覆盖。平台未删时用 `api --request DELETE "$API/projects/$PROJECT/repository/branches/<分支名>"`。
- 分支新鲜度：`git fetch origin <base>` 后 `git rev-list --count origin/<branch>..origin/<base>` 应为 0。落后时本地 rebase 后 `git push --force-with-lease`，再等 `detailed_merge_status` 刷新为 `mergeable`。刚 rebase 或刚推送时它可能短暂为 `checking` 或 `unchecked`，不要据此判定失败。
- 评审：GitLab 的批准用 `api "$API/projects/$PROJECT/merge_requests/<iid>/approvals"` 读，评论意见读 `/notes`。

## 检查

项目有 `.gitlab-ci.yml`，MR 与 master 推送各触发一条流水线，两个 job 并行：

- `check`：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test --lib`，外加 xvfb 下的 `x11_backend_is_xinput_when_xmodmap_resolves`。
- `frontend`：`npm ci`、`npm test`、`npm run build`。

读状态用 `api "$API/projects/$PROJECT/merge_requests/<iid>/pipelines"`，合并前 `status` 必须是 `success`。`failed`、`canceled`、`running` 都不能合并，`running` 时等它结束再判断。流水线未完成时 MR 的 `detailed_merge_status` 会是 `ci_still_running`，失败时是 `ci_must_pass`。

Windows x64 测试、Windows arm64 交叉编译检查与 Linux arm64 测试在极狐共享 runner 上没有对应环境，GitLab 流水线不覆盖。这些检查只能把提交同步到 GitHub 仓库后在 GitHub Actions 触发，jihulab 的 MR 不等待它们，发版前单独确认。Release 与 GitHub Pages 也仍在 GitHub 侧。

## 合并请求作为分诊渠道

MR 作为请求渠道：否。本仓库只分诊 issue，外部 MR 不进入请求队列。

设为 `是` 时，MR 与 issue 走相同的标签和状态，使用下列等价调用：

- 读取 MR：`api "$API/projects/$PROJECT/merge_requests/<iid>"`，diff 用 `/merge_requests/<iid>/changes`。
- 列出待分诊的外部 MR：`api "$API/projects/$PROJECT/merge_requests?state=opened&per_page=100"`，再对每个 `author.id` 查 `api "$API/projects/$PROJECT/members/all/<user_id>"`，返回 404 视为非项目成员。
- 评论 / 打标签 / 关闭：`/merge_requests/<iid>/notes`、PUT `labels`、PUT `state_event=close`。

## 当技能说“发布到 issue tracker”时

在 `cislunarspace/altgo` 创建一个 issue。

## 当技能说“获取相关工单”时

运行 `api "$API/projects/$PROJECT/issues/<iid>"`，需要评论时再取 `/notes`。

## Issue Board

使用看板：是。看板在项目的 Issue Boards 中，列由标签驱动，状态迁移就是增删对应标签。标签名区分大小写，与列名逐字一致。标签不存在时先在项目 Labels 建好再挂列。

| 列 | 标签 |
| --- | --- |
| Inbox | `Inbox` |
| Backlog | `Backlog` |
| Ready | `Ready` |
| In progress | `In progress` |
| In review | `In review` |
| Done | `Done` |
| No action | `No action` |

- 查看板：`api "$API/projects/$PROJECT/boards" | jq '.[] | {id, name}'`。
- 查列：`api "$API/projects/$PROJECT/boards/<board_id>/lists" | jq '.[] | {id, label: .label.name, position}'`。
- 改状态：给 issue PUT `add_labels=<目标列>` 与 `remove_labels=<原状态标签>`。每个 issue 只保留一个状态标签，否则会同时出现在多列。
- 优先级用 `P0`–`P3` 标签，其他字段没记录的不写。
