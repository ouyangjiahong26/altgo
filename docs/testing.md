# 测试策略

本文档回答一个问题：一个改动应该补哪层测试、用什么替身、跑什么命令验证。
与 [`architecture.md`](architecture.md) 配套阅读：那篇讲模块结构，这篇讲各层怎么测。

测试分五层，从核心到外壳：

| 层 | 一句话职责 | 判断依据 |
|---|---|---|
| 纯逻辑 | 不碰框架与操作系统的业务规则 | 兜底：不命中其余四层的 Rust 模块 |
| 语音流水线 | 多模块编排契约 | 用 test_doubles 组装核心模块 |
| Tauri 适配 | 核心与 Tauri 运行时、Linux 系统接口的接缝 | 依赖窗口/事件/命令或 X11/evdev/PulseAudio/剪贴板 |
| 前端交互 | React 组件与表单行为 | frontend/src 下的测试 |
| 端到端用户流程 | 真实设备上的完整用户旅程 | 当前无实现，只有缺口清单 |

归属规则：一个模块的测试属于且仅属于一层，按模块归层。同模块内测试层次混杂时，以模块的主要契约为归属。下文清单里模块名后的括号，是该模块下有测试的子模块。无测试的模块不出现在下文清单中，给它们补测试时按上表选层。

## 一、纯逻辑层

验证不依赖 Tauri、不依赖操作系统服务的业务规则。

**模块与行为**：

- `state_machine`：5 态 FSM 的转换、防抖与连发边界
- `audio`：PCM 环形缓冲、WAV 编解码（含截断、非法头等异常分支）
- `config`：TOML 加载、校验（含 SenseVoice 只接受 16 kHz）、环境变量覆盖、文件权限位
- `config_store`：`apply_patch` 的校验、持久化、失败回滚、`linux_evdev_code` 三态语义
- `history`：历史条目的增删改查与计数
- `model`：模型目录结构校验、SHA-256 校验、下载（成功、HTTP 失败、损坏检测、重试、跳过已存在）
- `polisher`：OpenAI 兼容与 Anthropic 两种协议的请求/响应、429 与瞬时失败重试、重试耗尽
- `sherpa`：模型/词表缺失时的报错、语言归一化（不做真实推理）
- `prompt_store`：prompt 模板加载、base + 各档后缀组合、缺文件降级
- `error`：错误的致命/可恢复分类与文案

**可用替身**：临时目录与文件、注入的闭包与参数、mockito 本地 HTTP 假服务器（model 下载与 polisher 协议）。本层不使用语音流水线的 test_doubles。

**执行命令**：本地按模块过滤，如 `cargo test --manifest-path=src-tauri/Cargo.toml --lib state_machine::`。CI 双架构跑全量（见“回归基线”）。

## 二、语音流水线层

验证编排契约：按键事件如何经状态机变成录音、转写、润色、输出动作。

**模块与行为**：

- `voice_pipeline`（handlers / context / builder）：结果文本选择（润色优先、润色失败回退原文、空结果丢弃）、剪贴板失败仍返回结果、PipelineContext 端到端（全替身跑完按键到输出）、构建期配置校验（本地模型缺失、未知润色协议）
- `pipeline_controller` 的 start/stop 生命周期：防重入、重复 start 拒绝、空 stop 空操作、不死锁

**可用替身**：`voice_pipeline/test_doubles.rs` 里的 FakeRecorder、FakeTranscriber、MockSink、FakeListener。本层新测试一律复用这套替身，不自造第二套。

**执行命令**：`cargo test --manifest-path=src-tauri/Cargo.toml --lib voice_pipeline::`（或 `pipeline_controller::`）。CI 双架构跑全量。

## 三、Tauri 适配层（含 Linux 平台适配）

验证两处接缝：核心与 Tauri 运行时之间，以及核心与 Linux 系统接口之间。

**模块与行为**：

Tauri 边界：

- `tauri_sink`：管道状态/结果到 Tauri 事件与悬浮窗状态的映射、事件顺序。脱 Wry 运行，不需要 Tauri 运行时
- `cmd` 的 Tauri 命令核心逻辑：restart_pipeline 的停止/启动编排、copy_text、download_model 的事件序列、polish_history_entry
- `overlay`（manager / tauri）：悬浮窗状态切换与隐藏时序、定位与缩放、各步失败容忍。xrandr 输出解析

Linux 平台接缝：

- `key_capture`：evtest 输出解析、子进程回收（成功/超时/无输出）
- `key_listener`：X11 监听构造、keysym ↔ evdev 映射
- `recorder`：PulseRecorder 构造冒烟、类型化错误
- `output`：剪贴板工具探测（xclip/xsel/wl-copy）

**可用替身**：MockDispatch、MockOverlay、RecordingOverlayWindow（记录悬浮窗调用的假窗口）、注入的 spawn 闭包（key_capture 的 evtest）、mockito 本地 HTTP 假服务器（cmd 的 download_model）。不开真实窗口，不连真实显示/音频服务。依赖系统工具的测试写成环境容忍：有 xinput 断言 Ok、无则断言 Err。探测不到剪贴板工具也算通过。这样无显示服务器的 CI 容器与开发者机器跑出同样结果。环境过不去的测试不允许用 `#[ignore]` 藏起来。

**执行命令**：按模块过滤同上，如 `cargo test --manifest-path=src-tauri/Cargo.toml --lib tauri_sink::`。CI 双架构跑全量。

## 四、前端交互层

验证 React 侧的组件与表单行为。

**模块与行为**：

- `overlay`：悬浮窗相位转换（show/replace/hide）与渲染（前端组件，与同名的 Rust `overlay` 模块不同物）
- `useConfigForm`：设置表单的配置归一化与保存请求体构造
- `StatusIndicator`：状态指示组件渲染

**可用替身**：vitest + jsdom + Testing Library。Tauri IPC 一律 `vi.mock("@tauri-apps/api/core")` / `vi.mock("@tauri-apps/api/event")` 替换，不触真实后端。

**执行命令**：本地 `cd frontend && npm test`。CI 只在 amd64 job 跑一次（前端测试与目标架构无关）。

## 五、端到端用户流程层

当前没有任何端到端测试。以下用户流程缺少自动化保护：

1. 按住激活键开始录音，SenseVoice 本地转写，文本进剪贴板，悬浮窗各相位切换
2. 首次使用：从设置页下载 SenseVoice 模型，完成 SHA-256 校验，转写可用
3. 修改设置并保存，重启应用后生效（真实 IPC、真实配置文件）
4. 主页历史列表浏览、删除、对旧条目重新润色（真实 LLM API）
5. deb / rpm 在 x86_64 与 aarch64 真机安装后启动并完成一次转写
6. NSIS 安装包在 Windows 10+ 真机安装后启动并完成一次转写（按键钩子、WASAPI 录音、SendInput 注入均需真实会话）

补这一层之前先评估代价：真实模型体积大、LLM 调用要花钱、需要显示服务器与音频设备。再决定自动化（如真实模型 + 合成 WAV 的冒烟）还是列为发版前手动回归项。决定记录在本节。

## 回归基线

**产品支持范围**：Linux x86_64 与 aarch64。Windows x86_64 与 arm64（原生 API 实现：WH_KEYBOARD_LL、cpal/WASAPI、arboard/SendInput）。SenseVoice 本地转写（仅接受 16 kHz 单声道 16 位 PCM WAV）或小米 MiMo 在线转写（`[transcriber] backend = "online"`），可选 LLM 润色（OpenAI 兼容或 Anthropic 协议）。在线转写以 MiMo 后端回归，测试见 `src-tauri/src/mimo_asr.rs` 模块（mockito）。更早的云端转写实现及其测试已随 #121 删除，不要回填。

**CI 保护范围**：

- `ci.yml` 的 check job 在 ubuntu-22.04（amd64）与 ubuntu-22.04-arm（arm64）各跑全量 `cargo test --manifest-path=src-tauri/Cargo.toml --lib`
- 前端测试、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`（测试代码也过编译检查）只在 amd64 job 跑一次
- `ci.yml` 的 check-windows job 在 windows-latest 上按架构分步：x64 跑 `cargo test --lib`。arm64 交叉编译，只能 `cargo check`
- `release.yml` 的 test job 同样双架构跑 `cargo test --lib`。deb/rpm 打包（build-linux）`needs: test`，发版前必须过两种架构的测试。NSIS 打包（build-windows）`needs: validate`，覆盖 Windows x64 与 arm64，打包前 x64 跑测试、arm64 只做编译检查

**基线要求**：全量全绿，无 `#[ignore]`，无静默跳过。改动落在哪层，测试随代码补在哪层。若改动的风险只有端到端层能覆盖（如安装包真实启动），在 PR 里说明手动验证方式。

## 派生信息：数字现查，不进文档

测试数量与模块分布会随代码漂移，本文档不维护任何数字。需要时用命令现查：

```bash
# Rust 测试总数
cargo test --manifest-path=src-tauri/Cargo.toml --lib -- --list | grep -c ': test$'

# 按模块分布
cargo test --manifest-path=src-tauri/Cargo.toml --lib -- --list | grep ': test$' | awk -F'::' '{print $1}' | sort | uniq -c

# 前端（看输出末尾的 Tests 计数）
cd frontend && npm test

# 真实通过率：跑一次全量，看 test result 行
cargo test --manifest-path=src-tauri/Cargo.toml --lib
```

覆盖率目前未统计。若日后引入覆盖率工具，把生成命令记在本节，数字本身仍不进文档。
