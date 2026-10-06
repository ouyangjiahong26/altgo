# 系统架构

本文档是 altgo 的架构速览，面向维护者与贡献者，与 [`testing.md`](testing.md) 配套阅读。数据来自 2026-08-26 对 `src-tauri/src/` 的源码调研。文中引用只用模块与符号名，不标行号（行号会漂移，符号名才是稳定的锚点）。

## 一、系统概览

altgo 是基于 Tauri 的桌面语音转文字工具：Rust 后端承载整条语音流水线，React 前端负责主页、设置页与悬浮窗。两个收敛后的设计前提：

- **转写双后端**：由 `[transcriber] backend` 选择。默认 `"local"` 走本地 SenseVoice（内嵌 sherpa-onnx，模型常驻内存），`"online"` 走小米 MiMo 在线识别（chat/completions + `input_audio`，失败直接报错不回退），分发接缝在 `voice_pipeline/builder.rs` 的 `build_transcriber`。whisper.cpp 与 Whisper API 已随 #121 删除。
- 平台为 Linux（Ubuntu 22.04+，x86_64/aarch64）与 Windows 10+（x86_64/arm64）。旧的 PowerShell 式 Windows 适配曾随 #121 删除，现行实现（`windows.rs`）为原生 API 重写：WH_KEYBOARD_LL 钩子监听按键、cpal/WASAPI 录音、arboard 剪贴板 + SendInput 文本注入（由 `[output] inject_text` 配置控制，默认关闭，ADR 0005）。

核心设计只有一句话：业务核心与框架彻底解耦，平台能力一律收进 trait seam。`voice_pipeline` 模块完全不 import Tauri，只通过 `PipelineSink`、`TranscriptionDispatch`、`OverlaySink` 等 trait seam 与外界交互。按键监听、录音、剪贴板等系统能力也都被收进各自 trait 后面，各平台实现命名为 `linux.rs` / `windows.rs`。

整条流水线跑在独立的 OS 线程上，内部是一个独立的 `current_thread` tokio 运行时，与 Tauri 主运行时隔离（见 `lib.rs` 的 `spawn_pipeline_thread`）。

```text
+-----------------+
|   React 前端     |  设置 / 历史 / 悬浮窗
+-----------------+
         | IPC (21 命令 + 12 事件)
         v
+-----------------+
|  装配层 lib.rs   |  spawn_pipeline_thread：管理状态、组合 seam
+-----------------+
         |
         v
+-----------------------------+
|     voice_pipeline          |  组合根：builder + context + handlers
|  ┌───────────────────────┐  |
|  |   PipelineContext     |  |  tokio::select! 三分支主循环
|  |  ┌─────────────────┐  |  |
|  |  |   state_machine  |  |  |  5 态同步状态机（crate 根部模块）
|  |  └─────────────────┘  |  |
|  |  handlers.rs          |  |  录音→转写→润色→分发
|  |  dispatcher.rs        |  |  TranscriptionDispatch seam
|  |  sink.rs              |  |  PipelineSink / TranscriptionResult
|  └───────────────────────┘  |
+-----------------------------+
         |                  |
         v                  v
+-------------------+  +---------------------------+
| 转写/润色后端      |  |      平台适配层            |
| transcriber       |  | key_listener              |
| sherpa            |  | recorder                  |
| polisher          |  | key_capture               |
| prompt_store      |  | output                    |
| model             |  +---------------------------+
+-------------------+
         |
         v
+----------------------+
| config/error/resource/audio |  公共叶子
+----------------------+
```

### 职责一览

以“按下触发键到结果展示”为主线，各关切的归属：

| 关切 | 负责模块 | 边界说明 |
|------|----------|----------|
| 界面（设置 / 历史 / 悬浮窗内容） | `frontend/`（React） | 只经 IPC 与后端交互，只见 camelCase |
| 按键状态机 | `state_machine.rs` | 纯同步叶子，只返回命令，不执行副作用 |
| 录音 | `recorder`（`Recorder` trait） | Linux 实现为 `parecord` 子进程 |
| 转写 | `transcriber`（`Transcriber` trait） | 本地实现 `sherpa.rs`（SenseVoice），在线实现 `mimo_asr.rs`（小米 MiMo） |
| 润色 | `polisher`（`LLMFormatter`） | 可选，失败降级为原文 |
| 悬浮窗 | `overlay`（`OverlaySink` seam） | 生产实现是 Tauri 窗口 |
| 剪贴板 + 历史 | `dispatcher`（`TranscriptionDispatch` seam）→ `output` + `history` | 失败只 warn，不中断结果返回 |
| 事件发射 | `tauri_sink`（`PipelineEventEmitter` seam） | 流水线事件到前端的通道 |
| 主循环编排 | `voice_pipeline::context` | 驱动状态机，就地执行命令 |

## 二、依赖方向

依赖整体单向、清晰：

- 装配层 `lib.rs` 唯一负责把 Tauri-managed state 注入流水线。
- `voice_pipeline` 是组合根（composition root），内部再向下组合 `handlers` / `dispatcher` / `sink`。
- `state_machine` 是 crate 根部的纯同步叶子，由 `voice_pipeline::context` 驱动。
- `handlers` 调用 `transcriber` / `polisher` / `recorder`。
- `dispatcher` 调用 `output`（剪贴板）与 `history`（历史记录）。
- `transcriber` 调用 `resource`，本地实现 `sherpa`（内嵌 sherpa-onnx 的 SenseVoice），在线实现 `mimo_asr`（小米 MiMo 网关），由 `builder.rs` 按 `[transcriber] backend` 分发。
- `polisher` 调用 `prompt_store`。
- `model` / `config` / `error` / `resource` / `audio` 是底层叶子（`audio` 提供 PCM 缓冲与 WAV 编解码）。

```text
lib.rs
  |
  +-- voice_pipeline
  |     |
  |     +-- handlers
  |     |     +-- transcriber
  |     |     |     +-- sherpa
  |     |     |     +-- resource
  |     |     +-- polisher
  |     |     |     +-- prompt_store
  |     |     +-- recorder
  |     +-- dispatcher
  |     |     +-- output
  |     |     +-- history
  |     +-- sink
  |
  +-- state_machine        (纯同步叶子，由 voice_pipeline::context 驱动)
  +-- pipeline_controller  (生命周期 + PipelineStatus)
  +-- tauri_sink           (经 PipelineEventEmitter 发射事件)
  +-- cmd.rs               (IPC 命令；check_update/install_update 调用 updater)
  +-- display_backend      (run() 在 GUI 初始化前探测 Wayland，切 GDK 后端)
```

**Seam 规则。** 三类边界必须经 trait，业务核心只面向 trait：

1. **框架**：`PipelineSink`（状态/错误/结果回调）、`PipelineEventEmitter`（事件发射）、`TranscriptionDispatch`（剪贴板 + 历史分发）、`OverlaySink`（悬浮窗）。
2. **平台**：`Recorder`（录音）、`KeyListener`（按键）、`Output`（剪贴板），当前实现见第五节。
3. **引擎**：`Transcriber`（转写后端），本地实现为 `sherpa.rs` 的本地 SenseVoice，在线实现为 `mimo_asr.rs` 的小米 MiMo。

同 crate 内向下的模块依赖允许直接 import：`handlers` 调 `polisher` 的具体类型、各模块依赖 `config` / `error` 等底层叶子，都不需要 seam。seam 是测试注入 fake 的位置，也是未来加平台或后端时的扩展点。

三个值得注意的依赖关系：

1. **唯一一个环**：`polisher` ↔ `prompt_store`，但二者同处一个 crate 内，可接受。
2. `voice_pipeline::context.rs` 依赖 `pipeline_controller::PipelineStatus`（UI 状态枚举从底层向上泄漏了一点）。
3. `tauri_sink.rs` 通过 `PipelineEventEmitter` seam 发射事件，仅生产实现 `TauriEventEmitter` 持有 `AppHandle`（issue #104 已修）。

## 三、核心流水线

### 主循环

`voice_pipeline/context.rs` 的主循环是 tokio::select! 四分支：

1. **按键事件**：交给 `machine.process(ev)`。
2. **状态机超时**：调用 `machine.poll_timeout()`（仅在 `deadline.is_some()` 时启用）。
3. **重试请求**：取出待重试录音，走与停止录音相同的转写收尾。
4. **停止信号**：`break`。

命令由状态机同步返回，`match cmd` 后就地调用 `handle_start_record` 或 `handle_stop_record`。状态机没有自己的“命令通道”，它只把意图交给调用方。重试请求是唯一的外部注入命令，同样在循环内串行执行。

### 状态机

`state_machine.rs` 是 crate 根部的纯同步叶子，5 个状态：

| 状态 | 含义 |
|------|------|
| `Idle` | 空闲 |
| `PotentialPress` | 按下后等待是否达到长按阈值 |
| `Recording` | 长按触发，松开即停 |
| `WaitSecondClick` | 短按松开后等待双击 |
| `ContinuousRecording` | 双击触发，再按一次停止 |

状态机通过 `next_deadline()` 把下一个超时点暴露给外层，由 `tokio::time::sleep_until` 驱动。

### 录音、转写、润色、分发

`handlers.rs` 的 `handle_stop_record` 调用链如下：

| 步骤 | 输入 | 输出/行为 | 出错处理 |
|------|------|-----------|----------|
| 停止录音 | `&dyn Recorder` | WAV 字节 | 报错，回 `Idle` |
| 误触守卫 | WAV 时长（`audio::wav_duration_ms`） | 低于 300 ms 直接丢弃，回 `Idle` | 不转写、不报错 |
| 转写 | WAV + 进度回调 | `TranscribeResult` | `on_error` + 录音本体进待重试槽位，回 `Idle` |
| 空文本过滤 | 转写文本 | 同上按识别失败处理，保留录音 | 同上 |
| 润色 | `raw_text` + `LLMFormatter` | 润色后文本 | 降级为 `raw_text` |
| 结果分发 | `TranscriptionResult` | 悬浮窗 + 剪贴板 + 历史 | 剪贴板/历史失败只 warn |

关键点：

- 润色失败是可恢复降级。转写失败与空结果不是静默降级：录音本体保留进
  `PendingRecordingStore`（内存单槽、不落盘），主窗横幅提供“重新识别”，
  重试请求经 `RetryRequestHandle` 进入主循环串行执行（ADR-0003 不变）。
- 剪贴板失败、历史追加失败只 `tracing::warn!`，不中断结果返回（见 `process_transcription_result`）。

### 本地引擎：内嵌常驻

`SherpaTranscriber`（`sherpa.rs`）内嵌 sherpa-onnx 跑本地 SenseVoice int8 模型。sherpa-onnx 编译进主程序，模型在管道启动时加载一次并常驻内存，之后每句话直接推理（先 `accept_waveform` 再 `decode`），没有进程启动与冷载成本。推理是 CPU 密集同步操作，经 `spawn_blocking` 放入阻塞线程池。模型文件缺失或加载失败在构造期报错（`TranscriberError::ModelLoadFailed`）。

### 在线引擎：MiMo 网关

`MimoAsr`（`mimo_asr.rs`）走小米 MiMo 网关的 `chat/completions`：WAV 以 base64 放进单个 `input_audio` 内容块，`asr_options.language` 传 `[transcriber] language`（空串发 `"auto"`），文本取自 `choices[0].message.content`。纯网络调用，不做重试。失败直接经 `on_error` 报错，不回退本地。端点由 `polisher::build_endpoint` 推导，密钥可经 `ALTGO_TRANSCRIBER_API_KEY` 覆盖。

### 润色 prompt 三级回退

`polisher.rs` 构造 `LLMFormatter` 时，`from_config_with_sources` 统一驱动三级回退（`build_prompt_source_chain`）：

1. `PromptStore` 加载的 `resources/prompts/` 模板（`base.txt` + 档级后缀）。
2. 配置里的 `system_prompt`（非空时）。
3. 内置 hardcoded 默认提示。

启动时加载一次，改文件需重启生效。

### 主循环阻塞是有意设计

`handle_stop_record` 在 select 分支内被 `await`，一次转写/润色会阻塞整个按键循环直到完成。这是有意设计：altgo 的转写是“按一次键录一句”的单发操作，阻塞保证一次只完成一次转写。详见 ADR-0003。

## 四、错误模型

`error.rs` 把错误分为两层：

| 分类 | 用途 | 典型枚举 |
|------|------|----------|
| `FatalError` | 构建期，管道不启动 | `ModelNotFound` / `ApiAuthFailed` / `KeyListenerFailed` / `TranscriberInitFailed` / `PolisherInitFailed` / `RecorderInitFailed` |
| `RecoverableError` | 运行时，降级继续 | `TranscriptionFailed` / `PolishingFailed` / `RecordingFailed` / `EmptyTranscription` |

模块边界一律返回自定义 thiserror 枚举：`TranscriberError` / `PolisherError` / `RecorderError` / `OutputError` / `KeyListenerError` / `ModelError` / `ConfigError` / `HistoryError`。`recorder` 模块有专门测试防止 trait 边界回退到 `anyhow`。

一个小不一致：运行时 handler 里的错误不经结构化 `PipelineError`，而是取该错误的用户文案（如 `TranscriberError::message()`）交 `sink.on_error`。`PipelineError` 主要用于构建期，两套机制并行存在。

## 五、平台抽象

支持范围：Linux（Ubuntu 22.04+）的 x86_64 与 aarch64，Windows 10+ 的 x86_64 与 arm64。平台服务全部收在 trait 后面，各模块的实现文件按平台命名为 `linux.rs` / `windows.rs`，这是加新平台时的扩展点，也是测试注入 fake 的接缝：

| 模块 | Trait | Linux 实现 | Windows 实现 |
|------|-------|------------|--------------|
| `key_listener` | `KeyListener::start()` | X11 用 `xinput test-xi2`、失败回退 `evtest`，Wayland 会话优先 `evtest` | WH_KEYBOARD_LL 低级键盘钩子 |
| `recorder` | `Recorder::start_recording/stop_recording/is_recording` | `parecord` 子进程 | cpal/WASAPI |
| `output` | `Output::write_clipboard` + `clone_box` | `xclip`/`xsel`/`wl-copy` 探测一次 | arboard 剪贴板 + SendInput 文本注入 |
| `key_capture` | 无（自由函数） | `evtest` 监听 `/dev/input/event*` 等一次按键 | 临时 WH_KEYBOARD_LL 钩子等一次按键 |

另有两处平台相关但不走 trait 的分支：`display_backend.rs` 在 GUI 初始化前探测 Wayland 会话并切 GDK 后端。悬浮窗的主显示器几何在 `overlay/tauri.rs` 内按平台取值：Linux 解析 `xrandr` 输出，Windows 用 Tauri `primary_monitor()`。

各平台录音统一输出 16 kHz 单声道 16 位 PCM，`audio.rs` 在录音停止时编码为 WAV。这是 SenseVoice 唯一接受的输入格式。

`Box<dyn Trait>` 用于 `PipelineContext` 字段与 builder 返回值，`Arc<dyn Trait>` 用于 Tauri 侧注入。

## 六、IPC 契约面

### 命令

`cmd.rs` 暴露 21 个命令：

- 配置 5：`get_config`、`save_config`、`capture_activation_key`、`test_polisher_connection`、`fetch_provider_catalog`
- 更新 2：`check_update`、`install_update`
- 流水线 1：`start_pipeline`
- 悬浮窗 2：`copy_text`、`hide_overlay`
- 模型 4：`list_models`、`download_model`、`delete_model`、`resolve_model`
- 历史 4：`list_history`、`delete_history_entries`、`clear_history`、`polish_history_entry`
- 待重试录音 3：`get_pending_recording`、`retry_pending_transcription`、`discard_pending_recording`

### 事件

共 12 个 Tauri 事件：

`pipeline-status`、`pipeline-error`、`transcription-result`、`polish-failed`、`transcription-progress`、`audio-level`、`key-listener-backend`、`history-updated`、`overlay-state`、`model-download-progress`、`model-download-finished`、`pending-recording-changed`。

`pending-recording-changed` 载荷为待重试录音元信息（`durationMs` + 错误码结构）或 null。保留与清空（重试成功、用户放弃）都会发出，主窗横幅据此显隐。

`polish-failed` 携带润色失败原因字符串，在 `transcription-result` 之前发出，悬浮窗 done 相位据此显示“润色失败，已使用原文”。`audio-level` 在录音期间以固定 100 ms 间隔（10 次/秒）定时派发感知音量给悬浮窗，驱动录音相位的实时波形。

### 序列化契约

- IPC 与 `history.json` 使用 `camelCase`。
- `config.toml` 使用 `snake_case`。
- 前端永远只见 camelCase，Rust 内部 snake_case，靠 `serde(rename_all)` 在边界转换。

## 七、质量 / 可维护性观察点

### 结构性

- `voice_pipeline::context.rs` 依赖 `pipeline_controller::PipelineStatus`，UI 状态枚举从底层向上泄漏了一点。

### 文档漂移 / 死代码

- `notify-send` 从未实现，结果展示统一走 Tauri overlay。
- `prompt_store` 没有热重载，改文件需重启。

### 架构资产

- `voice_pipeline` 完全不 import Tauri，全靠 trait seam。
- 状态机、`overlay/manager` 都是干净的叶子/纯逻辑，测试覆盖好。
- 回退链设计成熟：`prompt` 三级，模型下载由官方回退到镜像。
- 错误分类致命/可恢复边界清晰。
