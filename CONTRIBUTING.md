# 贡献指南

感谢你对 altgo 的关注！

本项目支持 Linux 的 x86_64 与 aarch64 架构。CI 和 Release 在 Ubuntu 22.04 上完成 Linux 构建验证。合并前请尽量在相关架构上自测。

## 开发环境

- Rust 1.80+（推荐最新稳定版，需满足 [Tauri 2 前置条件](https://tauri.app/start/prerequisites/)）
- Node.js 18+（建议 20+，前端使用 npm）
- Tauri CLI：`cargo install tauri-cli --version "^2" --locked`
- Linux（Ubuntu 22.04+）

### 平台特定依赖

- **Linux**：`xinput`、`xmodmap`、`parecord`、`xclip` 或 `wl-copy`。Wayland 下按键监听还需 `evtest`，且需能读取 `/dev/input/event*`（常见：`sudo usermod -aG input $USER` 后重新登录）。完整 GUI 构建需 GTK/WebKit 等开发库，见 [Tauri 2 前置条件](https://tauri.app/start/prerequisites/)。

## 开发流程

1. Fork 仓库
2. 创建功能分支 (`git checkout -b feat/my-feature`)
3. 编写代码和测试
4. 确保通过检查：
   ```bash
   cargo fmt --manifest-path=src-tauri/Cargo.toml --all -- --check
   cargo clippy --manifest-path=src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path=src-tauri/Cargo.toml
   cd frontend && npm test
   cd frontend && npm run build
   ```

   其中 Rust 的 fmt / clippy / test 也有 Makefile 便捷目标：`make fmt`、`make lint`、`make test`。
5. 提交变更 (`git commit`)
6. 推送分支 (`git push origin feat/my-feature`)
7. 创建 Pull Request：标题只写改动内容，正文按[正文写作约定](#正文写作约定)完整填写

## 提交消息格式

```
type: 简短描述

可选正文说明
```

类型：`feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`
正文（可选）写完整段落，讲清楚做了什么和为什么，不用条目罗列。行文约束见[正文写作约定](#正文写作约定)。

## 正文写作约定

Issue、PR、评论、commit message 和 CHANGELOG 都是写给协作者读的，不是写给程序或作者自己读的。默认读者了解本仓库所在的领域，但不了解这条改动的来龙去脉：他应该只读正文就能明白发生了什么、为什么、自己接下来能做什么。标题承担扫读职责，扫读清单的人只看标题就该知道这条工单要做什么，完整叙述放正文。

正文要回答一组固定的问题，而不是堆砌参数和决策名。PR 正文回答：这个改动解决什么问题，为什么值得做。为什么采用这个方案，否决过哪些替代做法。实际改了什么，按协作者阅读代码的顺序讲清楚来龙去脉。怎么验证的，跑了哪些命令和测试，结果是什么。协作者从哪里继续读，关键文件和后续工作在哪。Issue 正文回答：现状是什么，期望是什么，为什么重要，已经做过哪些排查或有过哪些想法。开单模板以提问的形式给出了这组问题，逐段回答即可，不必把问题本身抄进正文。

行文用平实的中文陈述句，一段话说完一个完整的意思，段落之间按逻辑承接。能写成连贯段落的就不要拆成小标题加短句，小标题连篇而每段只有一两句话，读者得到的是碎片而不是理解。内容本身就短时直说其事，不要在开头预告要说什么，也不要在结尾总结说过什么。

少用特殊符号。反引号只用于需要逐字精确的东西，比如符号名、命令、文件路径和代码片段。不用加粗或引号做叙述性强调。括号只保留真正插入语的一处，不连续嵌套，需要展开的意思写成从句。术语第一次出现时用一句话解释，不靠引号或加粗标注。

细节不收进折叠区。人写的文字全部放在正文可见区。折叠区只用于原始机器输出，比如长日志、命令输出和大段数据表。

commit 标题保持 `type: 中文简述` 的单行格式，类型见上文提交消息格式。标题之外写 body 时，body 必须是完整段落，讲清楚做了什么和为什么，不用条目罗列。改动很小时可以只有标题。

AI 生成的评论遵守同样的行文约束。篇幅按内容需要，短事短说。完整性指的是逻辑完整，不是字数。

CHANGELOG 条目保持现有形状：一条变化一个条目，粗体短标题加冒号，随后用完整的中文句子把变化、动因、边界和用法讲成连贯叙述。参数与符号名按上文的符号边界使用反引号，需要展开的意思用从句而不是嵌套括号。issue 引用保持在段尾。已发布条目是不可变历史。Unreleased 节里旧风格的存量条目，在下次发布前统一按本约定回改。

这些约定由评审把关：评审者按问题集核对 PR 正文是否回答完整，按行文约束提出修改意见。存量的 open PR 与 issue 不因本约定回改，但 open PR 在合并前由作者补正。写作约定没有机器检查。AI 贡献标记等原有硬性规则仍按 `docs/agents/issue-tracker.md` 执行。

## 代码风格

- 运行 `cargo fmt` 格式化代码
- `cargo clippy -- -D warnings` 零警告
- 公开 API 添加文档注释
- 函数 < 50 行，文件 < 1000 行

## 界面与样式

- 颜色、间距、圆角、动效一律用 `frontend/src/styles/design-tokens.css` 的 CSS 变量，不写死具体数值。
- `frontend/style-review.html` 是样式审查页：与应用运行无关的单文件静态页，按 `main.tsx` / `overlay.tsx` 的顺序直接加载 `src/styles/*`，另加悬浮窗与更新说明窗口的 `src/overlay.css`、`src/styles/pages/update-notes.css`（这两份不在 `main.tsx` 的链上），逐项列出全部 token 及组件、页面片段，可切换暗亮主题与根字号（15 / 17 / 19 px）。
- 打开方式：`cd frontend && npm run dev`，浏览器访问 `http://127.0.0.1:1420/style-review.html`（端口被占用时加 `-- --port 1430`）。用任意静态服务器指向 `frontend/` 目录也可。
- 改动设计 token、基础层样式或组件样式时同步更新该页。它不在 `vite.config.ts` 的 `rollupOptions.input` 内，不会被打包进应用产物。

## 测试

- 新功能尽量附带单元测试或 HTTP 级模拟测试（与 `transcriber`/`polisher` 类似）
- 使用 `#[cfg(test)]` 模块组织单元测试
- 集成测试放在 `tests/` 目录

## 平台相关开发

- 尽可能使用子进程调用系统工具（Linux），避免 FFI
- 新增系统工具调用时，确保有合理的错误处理和用户提示
- 平台特定代码通过 Linux 模块与 trait 隔离。每个平台模块实现对应 trait（`KeyListener`、`Recorder`、`Output`），使管道可测试

## CI、Release 与 GitHub Pages

- **CI**（`.github/workflows/ci.yml`）：向 `master` 推送或开 PR 时在 `amd64` 与 `arm64` 两个 Linux job 上运行 Rust 测试并构建 deb。前端测试、`fmt` 和 `clippy` 只在 `amd64` job 执行一次，因为这些检查不依赖目标架构。
- **Release**（`.github/workflows/release.yml`）：推送符合 `v*` 的 tag（例如 `v1.5.0`）时，先校验 tag、Cargo、Tauri 配置、前端版本和 CHANGELOG，再构建 Linux deb / rpm（`amd64` 与 `arm64`）及双架构 AUR PKGBUILD，生成 `checksums.txt` 并创建 GitHub Release。发版前请将 `src-tauri/Cargo.toml`、`tauri.conf.json` 和 `frontend/package.json` 中版本与 tag 对齐。
- **文档站**（`.github/workflows/deploy-docs.yml`）：`master` 上的 CI 成功后才构建 Docusaurus 并部署到 GitHub Pages（`workflow_run` 触发），避免 CI 失败时仍把文档发出去。首次需在仓库 Settings 的 Pages 页将 Build and deployment 的 Source 设为 GitHub Actions（勿选 branch 静态目录）。文档地址见 `docs-site/docusaurus.config.ts` 中的 `url` / `baseUrl`（例如 `https://<org>.github.io/altgo/`）。也可在 Actions 中手动 Run workflow 触发部署。

## 问题反馈

使用 GitHub Issues 报告 bug 或提出功能请求，开单模板会以提问的形式引导逐段回答：现状（复现步骤与实际结果）、期望、环境（操作系统与 altgo 版本）、已有排查。正文行文按[正文写作约定](#正文写作约定)。长日志等原始机器输出可收进折叠区，排查命令可用 `RUST_LOG=debug` 提取详细日志。
