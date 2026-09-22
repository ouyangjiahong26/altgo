# AGENTS.md

altgo：Rust + Tauri 桌面语音转文字工具。**始终用中文交流；代码、commit message、PR 描述等技术输出也用中文。**

## Project Overview

按住右 Alt（可改）录音，松开后本地 sherpa-onnx SenseVoice 转写，可选经 OpenAI 兼容或 Anthropic 协议的 LLM 润色（`none`/`light`/`medium`/`heavy` 四档），结果写剪贴板、悬浮窗展示，文本历史持久化到 `~/.config/altgo/history.json`（不保存音频）。支持 Ubuntu 22.04+ x86_64/aarch64 与 Windows 10+ x86_64/arm64，不支持 macOS。

## Architecture & Data Flow

核心流水线（深模块，业务与框架彻底解耦，平台能力一律收进 trait seam）：

```
Key Listener → State Machine → Recorder → Transcriber → Polisher → Output (+ History)
```

- 按键事件经 `mpsc` 进 `src-tauri/src/voice_pipeline/context.rs` 的 `tokio::select!` 三路主循环（按键 / 状态机超时 `sleep_until(deadline)` / oneshot 停止信号）；`state_machine.rs` 的 5 态状态机（Idle/PotentialPress/Recording/WaitSecondClick/ContinuousRecording）同步返回 `StartRecord`/`StopRecord`。转写与润色在主循环内串行完成——单次转写互斥（ADR-0003）。
- 停止录音后：`handlers::handle_stop_record` → WAV → `SherpaTranscriber`（`spawn_blocking` 推理）→ `LLMFormatter::polish`（失败置 `polish_failed` 并回退原文）→ `TranscriptionResult` 交 `PipelineSink`。`tauri_sink.rs` 由此发前端事件 + 切浮窗相位；`TranscriptionDispatch` 落到 `select_text`（按偏好选文本）→ `Output::write_clipboard` →（Windows 可选 `inject_text`，默认关，ADR-0005）→ `HistoryStore::append`。
- 关键 trait 接缝：`KeyListener`、`Recorder`、`Output`、`Transcriber`、`KeyCapture`（`key_capture/mod.rs`）、`PipelineSink`/`TranscriptionDispatch`（`voice_pipeline/`）、`OverlayWindow`/`OverlaySink`（`overlay/seam.rs`）、`UpdateProvider`（`updater.rs`）、`UserActivityClock`（`overlay/activity.rs`）。
- 平台模块三文件布局：`mod.rs` 定 trait + `#[cfg(target_os)] Platform*` 别名，`linux.rs`/`windows.rs` 各自实现。Linux 走子进程（`xinput`/`evtest`、`parecord`、`xclip`/`xsel`/`wl-copy`、`xrandr`），Windows 走原生 API（WH_KEYBOARD_LL、cpal/WASAPI、arboard + SendInput）。
- 整条管道跑在独立 OS 线程的 current_thread tokio runtime 上（`lib.rs::spawn_pipeline_thread`）；一切阻塞工作（推理、剪贴板、历史 I/O、join 线程）走 `spawn_blocking`。
- 状态管理：Tauri managed state 四件——`ConfigStore`（持锁更新，校验/落盘失败回滚内存）、`HistoryStore`（模块级 I/O 锁，Unix 落盘 0o600）、`PipelineController`（生命周期 + `PipelineStatus` 五态）、`Arc<dyn Output>`。
- IPC：`cmd.rs` 17 个 `#[tauri::command]`；事件 `pipeline-status`/`transcription-result`/`polish-failed`/`history-updated`/`overlay-state` 等，emit 点在 `tauri_sink.rs` 与 `cmd.rs`，前端 `hooks/useTauri.ts` 统一 listen。
- 前端两个窗口：主窗（`index.html`，HashRouter 三个页面）+ 悬浮窗（`overlay.html`，独立样式链，动画只动 transform 防 Linux WM 黑晕）。主题/字体/窗口尺寸存 localStorage，**不进** Tauri 配置。

## Key Directories

| 路径 | 用途 |
|---|---|
| `src-tauri/src/` | Rust 核心（crate `altgo-tauri`）：`voice_pipeline/` 业务主循环、`key_listener/`、`key_capture/`、`recorder/`、`output/`、`overlay/`（seam/manager/tauri/activity 分层）、`polisher/`，根文件见 “Important Files” |
| `frontend/src/` | React 主窗（`pages/`、`components/`、`hooks/`、`i18n/`）+ `overlay.tsx` 悬浮窗；`styles/` 分层：design-tokens → design-system → global → layout/components/pages |
| `configs/` | 用户 TOML 模板；应用实际读 `~/.config/altgo/altgo.toml` |
| `resources/prompts/` | 润色 prompt：`base.txt` + `light/medium/heavy-suffix.txt`（`none` 档不润色） |
| `docs/` | 维护者文档：`architecture.md`、`testing.md`、`adr/`（ADR-0003~0006）、`agents/`（agent 工作约定） |
| `docs-site/` | Docusaurus 用户文档站，与应用构建无关 |
| `packaging/` | 发版脚本 `scripts/`、AUR/Scoop/winget 清单 |
| `.github/` | `ci.yml`/`release.yml`/`deploy-docs.yml`/`warm-apt-cache.yml` + 复合 action `setup-linux-build` |

根目录另有 `CONTEXT.md`（领域术语表）、`CONTRIBUTING.md`、`Makefile`。**领域概念命名（issue 标题、测试名等）必须用 `CONTEXT.md` 术语表词汇**（`docs/agents/domain.md`）。

## Development Commands

```bash
# Rust（仓库根执行）
cargo build  --release --manifest-path=src-tauri/Cargo.toml
cargo test   --manifest-path=src-tauri/Cargo.toml --lib    # 与 CI 一致；make test 不带 --lib
cargo fmt    --manifest-path=src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path=src-tauri/Cargo.toml --all-targets -- -D warnings

# Tauri GUI（需 cargo install tauri-cli --version "^2" --locked）
cargo tauri dev      # beforeDevCommand 自动起前端 dev server（127.0.0.1:1420）
cargo tauri build    # beforeBuildCommand 自动跑 frontend npm run build

# Makefile 快捷目标（每条单独运行）
make build    # 缺 frontend/node_modules 时自动 npm install；产物 src-tauri/target/release/altgo
make test     # 全目标 cargo test（不带 --lib）
make fmt      # 格式检查
make lint     # clippy -D warnings
make install  # 安装 altgo 到 /usr/local/bin、模板到 /etc/altgo（应用不读取该路径）
make run      # 前台运行

# 前端
cd frontend && npm install && npm test        # vitest run；test:watch 可用
cd frontend && npm run build                  # tsc -b && vite build → frontend/dist

# 文档站
cd docs-site && npm install && npm run build   # 产物 docs-site/build
cd docs-site && npm start                      # dev server（热更新，前台阻塞）
```

提交前检查（`CONTRIBUTING.md`）：fmt + clippy + `cargo test --lib` + `cd frontend && npm test` + `npm run build`。

发版：push tag `v*` 触发 `release.yml`——先 `packaging/scripts/validate-release.sh` 校验 tag 与 `src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、`frontend/package.json` 三处版本及 `CHANGELOG.md` 小节对齐，再双架构构建 deb/rpm/AppImage 与 NSIS/MSI、生成 AUR 与 updater `latest.json`。CI 里 Tauri CLI 走 `npm --prefix frontend exec -- tauri build`。

## Code Conventions & Common Patterns

- **命名**：类型 CamelCase、函数/模块 snake_case；错误枚举 `<域>Error`（集中 `error.rs`，仅 `PromptError`/`OverlayError` 就近定义）；可测核心函数 `*_core`/`*_with_emitter`（如 `copy_text_core`）；生产实现 `…Impl`/`Tauri…`（`TranscriptionDispatcherImpl`、`TauriOverlayWindow`）；平台别名 `Platform*`。
- **错误处理**：thiserror 分层——顶层 `PipelineError::{Fatal, Recoverable}`（构造期失败 = Fatal 停管道，运行期失败 = Recoverable 降级继续）；`#[error]` 文案英文，`message()` 产出中文用户文案；Tauri 命令边界统一 `Result<T, String>`。
- **异步**：主循环 `tokio::select!`；停止用 `oneshot`，按键流用 `mpsc::unbounded`；async trait 手写 `Pin<Box<dyn Future>>` 签名；阻塞工作一律 `spawn_blocking`。
- **依赖注入**：组件在构造期一次性注入 trait object（`Box<dyn Recorder>`、`Arc<dyn Output>`…）；副作用（emit、download、spawn）抽成闭包/trait 参数，使单测不起 Tauri app。
- **serde**：IPC 面向结构体 `#[serde(rename_all = "camelCase")]`；状态/枚举值 `snake_case`；配置全字段 `serde(default)`（部分 TOML 可用，未知值回退默认）。
- **配置补丁**：`ConfigPatch` 三态语义——缺省 = 不改、`null` = 清除、值 = 设置（`config.rs`）。
- **注释**：双语成对（中文在前、英文在后），解释“为什么”而非“是什么”；公开 API 加文档注释；函数 < 50 行、文件 < 1000 行（`CONTRIBUTING.md`）。
- **日志/可见性**：`tracing` 结构化字段（如 `tracing::info!(backend, "key listener active")`）；内部实现收紧 `pub(crate)`。
- **前端**：i18n 自研字典（`frontend/src/i18n/`，key 形如 `settings.save`）；状态色/间距等一律走 `styles/design-tokens.css` 的 CSS 变量，不写死颜色。

## Important Files

- `src-tauri/src/lib.rs` — 装配入口 `run()`、`spawn_pipeline_thread`、17 个命令注册。
- `src-tauri/src/voice_pipeline/context.rs` — select! 主循环；`handlers.rs` — 录音/转写/润色/分发编排。
- `src-tauri/src/cmd.rs` — 全部 IPC 命令；`tauri_sink.rs` — 事件映射 + 浮窗状态切换。
- `src-tauri/src/error.rs` — 全部错误枚举；`state_machine.rs` — 按键状态机。
- `src-tauri/src/config.rs` + `config_store.rs` — 配置模型、补丁与持久化；`history.rs` — 历史 JSON。
- `src-tauri/tauri.conf.json` — 双窗口、bundle targets（deb/rpm/appimage/nsis/msi）、updater 端点。
- `configs/altgo.toml` — 全部配置字段模板（`[key_listener]`/`[recorder]`/`[transcriber]`/`[polisher]`/`[output]`/`[logging]`）。
- `CONTEXT.md`、`docs/architecture.md`、`docs/testing.md`、`docs/adr/` — 术语、架构、测试策略、决策记录。

**已知文档与现状出入（防止照抄旧说法）**：

- `resources/prompts/` 未随安装包分发（`tauri.conf.json` 无 `bundle.resources`）；打包安装后 PromptStore 不加载，走 `system_prompt`/内置回退。从仓库根运行开发版才会命中 `./resources/prompts`。
- `make install` 装到 `/etc/altgo/altgo.toml` 的模板不被应用读取；应用只读 `~/.config/altgo/altgo.toml`。
- 仓库没有 `tests/` 目录（`CONTRIBUTING.md` 的集成测试约定暂无实例）；`docs/testing.md` 模块清单漏 `updater.rs`、`display_backend.rs`。
- Loop 三件套不在 `.claude/`：builder/checker 实为 `docs/agents/builder.md`、`docs/agents/checker.md`，`loop-go` 规则见下节。
- Node 下界三处不一（CONTRIBUTING 18+ / `docs-site` engines ≥20 / CI 22）；Rust MSRV 未在 `Cargo.toml` 强制（CI 用 stable）。
- 文档统一写仓库全名 `cislunarspace/altgo`；fork 克隆里 `gh` 会解析到 fork（`git remote -v` 现查）。

## Runtime/Tooling Preferences

- **Rust**：edition 2021，Tauri 2.10.x，sherpa-onnx 1.13.6 静态链接（`SHERPA_ONNX_LIB_DIR`/`SHERPA_ONNX_ARCHIVE_DIR` 可覆盖，否则自动下载预编译包缓存到 `src-tauri/target/sherpa-onnx-prebuilt/`）。无 `rustfmt.toml`/`clippy.toml`——默认规则 + `-D warnings`。
- **Node**：一律 **npm**（`frontend/`、`docs-site/` 均 `package-lock.json` v3；不用 pnpm/yarn）；CI 用 Node 22；`tsconfig` strict + `noUnusedLocals`/`noUnusedParameters`。
- **Linux 运行时外部工具**：`xinput`、`xmodmap`、`evtest`、`parecord`、`xclip`/`xsel`/`wl-copy`、`xrandr`。Wayland 会话在 GUI 初始化前自动切 XWayland（`display_backend.rs`），否则浮窗定位不生效。
- **环境变量**：`ALTGO_POLISHER_API_KEY` 覆盖 `[polisher] api_key`；`ALTGO_MODEL_BASE_URL` 覆盖模型下载镜像。
- **Issue tracker**：issue/PRD 全部是 GitHub issue，一律用 `gh` CLI（命令约定见 `docs/agents/issue-tracker.md`）。
- **Triage 标签**（`docs/agents/triage-labels.md`）：`needs-triage`、`needs-info`、`ready-for-agent`、`ready-for-human`、`wontfix`。
- **领域文档**：单上下文布局 = 根 `CONTEXT.md` + `docs/adr/`；与 ADR 矛盾的输出必须显式标注，不得静默覆盖（`docs/agents/domain.md`）。
- **Loop 工程**：`/loop-go <任务>` 循环 builder 与 checker 直到检查全绿。停止规则：最多 5 轮（每轮声明 "Cycle N/5"）；同一失败连续两次 → 停止报告；修复使原本通过的检查失败 → 停止；到上限 → 停止报告现状。

## Testing & QA

- **五层模型**（`docs/testing.md`）：纯逻辑 → 语音流水线 → Tauri 适配（含 Linux 平台接缝）→ 前端交互 → 端到端（当前 0 个，缺口清单 6 条）。一个模块的测试属于且仅属于一层。
- **组织**：每个源文件末尾 `#[cfg(test)] mod tests`（`use super::*`）；无集成测试目录；前端测试与源码同目录（vitest + jsdom + Testing Library，共 9 个 `*.test.ts(x)`）。
- **替身四类**：tempfile（文件 I/O）、mockito（HTTP 假服务器）、共享 `src-tauri/src/voice_pipeline/test_doubles.rs`（流水线层唯一替身来源，不自造第二套）、闭包注入（emit/download/spawn）。`tauri_sink.rs`、`overlay/manager.rs`、`updater.rs` 各有模块私有 mock。
- **环境容忍**：依赖系统工具的测试对两种环境都断言（有 `xinput` 断 Ok、无则断 Err），保证无显示服务器的 CI 与开发机一致；**禁止用 `#[ignore]` 藏测试**。
- **回归基线**：`cargo test --manifest-path=src-tauri/Cargo.toml --lib` 全绿 + `cd frontend && npm test`，无静默跳过；只有端到端层能覆盖的风险，在 PR 里写明手动验证方式。测试数量/分布**现查不进文档**（`cargo test … --lib -- --list`）；覆盖率未统计；云端转写已移除，其测试不要回填。
- **CI 矩阵**：Linux amd64 + arm64 全跑 `cargo test --lib`（fmt/clippy/前端测试仅 amd64 各一次）；Windows x64 跑测试、arm64 交叉编译仅 `cargo check --target`。

## 写作要求

所有面向人读的文本（注释、CONTEXT.md、ADR、issue 评论、PR 描述、agent brief、triage notes、Sphinx 文档、Agent 回复）应当：

- 准确、清楚、简洁；先理解材料，再提炼结论。
- 按逻辑组织，区分相近概念；不用空泛、夸大的修饰语。
- 面向实际读者，从已知事实推到陌生结论；用分析说服，不装腔或堆砌。
- 全仓库文档不得使用直角引号「」，引号用弯引号（“”）。

## 编码准则

- **先理解再改动**：完整阅读目标文件、相似实现和相关测试；不确定 API 或惯例时查源码或文档，不猜。
- **明确目标与决策**：需求或验收条件不明确时先澄清；架构选择、假设和关键取舍要说明。
- **保持简单**：只实现当前需求。复用已有模式；不为单一用例过早抽象、配置化或引入依赖。
- **精准修改**：只改与任务直接相关的代码，贴合既有风格；删掉本次修改产生的废弃代码，不重格式化无关内容。
- **完整迁移**：变更接口或行为时更新所有调用方、测试和文档；不保留无需求的兼容层。
- **按根因修复**：先复现并读完整错误信息；一次处理一个原因，不用吞异常或特判掩盖问题。
- **验证行为**：按影响范围运行相关检查；测试可观察行为、边界和错误路径，不测试实现细节。无法测试时说明原因并做可行的烟雾验证。
- **审慎依赖**：优先现有依赖和标准库；新增依赖前确认必要性、维护状态和成本，并说明理由。
- **清楚沟通**：说明做了什么、为什么、验证结果和已知风险；对不确定性给出具体事实，提交信息描述实际改动。
