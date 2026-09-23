# 系统架构

本文档是 altgo 的架构速览，面向维护者与贡献者，与 [`testing.md`](testing.md) 配套阅读。数据来自 2026-08-26 对 `src-tauri/src/` 的源码调研。文中引用只用模块与符号名，不标行号——行号会漂移，符号名才是稳定的锚点。

# System Architecture

This document is an architecture overview of altgo, written for maintainers and contributors, meant to be read alongside [`testing.md`](testing.md). The data comes from a survey of the `src-tauri/src/` sources on 2026-08-26. References use only module and symbol names, never line numbers—line numbers drift; symbol names are the stable anchors.

## 一、系统概览

altgo 是基于 Tauri 的桌面语音转文字工具：Rust 后端承载整条语音流水线，React 前端负责设置页、历史页与悬浮窗。两个收敛后的设计前提：

- **转写只有一条路径**：本地 SenseVoice（内嵌 sherpa-onnx），模型常驻内存。whisper.cpp、Whisper API、MiMo ASR 已随 #121 删除。
- **平台为 Linux（Ubuntu 22.04+，x86_64/aarch64）与 Windows 10+（x86_64/arm64）**。旧的 PowerShell 式 Windows 适配曾随 #121 删除，现行实现（`windows.rs`）为原生 API 重写：WH_KEYBOARD_LL 钩子监听按键、cpal/WASAPI 录音、arboard 剪贴板 + SendInput 文本注入（由 `[output] inject_text` 配置控制，默认关闭，ADR 0005）。

核心设计只有一句话：**业务核心与框架彻底解耦，平台能力一律收进 trait seam**。`voice_pipeline` 模块完全不 import Tauri，只通过 `PipelineSink`、`TranscriptionDispatch`、`OverlaySink` 等 trait seam 与外界交互；按键监听、录音、剪贴板等系统能力也都被收进各自 trait 后面，各平台实现命名为 `linux.rs` / `windows.rs`。

整条流水线跑在独立的 OS 线程上，内部是一个独立的 `current_thread` tokio 运行时，与 Tauri 主运行时隔离（见 `lib.rs` 的 `spawn_pipeline_thread`）。

```text
+-----------------+
|   React 前端     |  设置 / 历史 / 浮窗
+-----------------+
         | IPC (17 命令 + 11 事件)
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

## 1. System Overview

altgo is a Tauri-based desktop speech-to-text tool: the Rust backend carries the entire voice pipeline, while the React frontend handles the settings page, history page, and floating overlay window. Two settled design premises:

- **Transcription has exactly one path**: local SenseVoice (with embedded sherpa-onnx), model resident in memory. whisper.cpp, Whisper API, and MiMo ASR were removed along with #121.
- **Platforms are Linux (Ubuntu 22.04+, x86_64/aarch64) and Windows 10+ (x86_64/arm64)**. An earlier PowerShell-style Windows adaptation had been removed along with #121; the current implementation (`windows.rs`) was rewritten against native APIs: a WH_KEYBOARD_LL hook for key listening, cpal/WASAPI recording, arboard clipboard plus SendInput text injection (controlled by the `[output] inject_text` setting, off by default, ADR 0005).

The core design fits in one sentence: **the business core is fully decoupled from frameworks, and every platform capability sits behind a trait seam**. The `voice_pipeline` module never imports Tauri and interacts with the outside world only through trait seams such as `PipelineSink`, `TranscriptionDispatch`, and `OverlaySink`; system capabilities like key listening, recording, and clipboard access are likewise tucked behind their own traits, with per-platform implementations named `linux.rs` / `windows.rs`.

The whole pipeline runs on a dedicated OS thread hosting its own `current_thread` tokio runtime, isolated from the main Tauri runtime (see `spawn_pipeline_thread` in `lib.rs`).

```text
+-----------------+
|   React 前端     |  设置 / 历史 / 浮窗
+-----------------+
         | IPC (17 命令 + 11 事件)
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
| 按键状态机 | `state_machine.rs` | 纯同步叶子；只返回命令，不执行副作用 |
| 录音 | `recorder`（`Recorder` trait） | Linux 实现为 `parecord` 子进程 |
| 转写 | `transcriber`（`Transcriber` trait） | 唯一实现：`sherpa.rs` 的本地 SenseVoice |
| 润色 | `polisher`（`LLMFormatter`） | 可选；失败降级为原文 |
| 悬浮窗 | `overlay`（`OverlaySink` seam） | 生产实现是 Tauri 窗口 |
| 剪贴板 + 历史 | `dispatcher`（`TranscriptionDispatch` seam）→ `output` + `history` | 失败只 warn，不中断结果返回 |
| 事件发射 | `tauri_sink`（`PipelineEventEmitter` seam） | 流水线事件到前端的通道 |
| 主循环编排 | `voice_pipeline::context` | 驱动状态机，就地执行命令 |

### Responsibility Map

With “from pressing the trigger key to showing the result” as the main storyline, here is where each concern lives:

| Concern | Owning module | Boundary notes |
|------|----------|----------|
| UI (settings / history / overlay content) | `frontend/` (React) | Talks to the backend only via IPC; sees camelCase only |
| Key state machine | `state_machine.rs` | Pure synchronous leaf; returns commands only, performs no side effects |
| Recording | `recorder` (`Recorder` trait) | Linux implementation spawns a `parecord` subprocess |
| Transcription | `transcriber` (`Transcriber` trait) | Sole implementation: local SenseVoice in `sherpa.rs` |
| Polishing | `polisher` (`LLMFormatter`) | Optional; degrades to raw text on failure |
| Overlay window | `overlay` (`OverlaySink` seam) | Production implementation is a Tauri window |
| Clipboard + history | `dispatcher` (`TranscriptionDispatch` seam) → `output` + `history` | Failures only warn and never interrupt returning the result |
| Event emission | `tauri_sink` (`PipelineEventEmitter` seam) | Channel from pipeline events to the frontend |
| Main loop orchestration | `voice_pipeline::context` | Drives the state machine and executes commands in place |

## 二、依赖方向

依赖整体单向、清晰：

- 装配层 `lib.rs` 唯一负责把 Tauri-managed state 注入流水线。
- `voice_pipeline` 是组合根（composition root），内部再向下组合 `handlers` / `dispatcher` / `sink`。
- `state_machine` 是 crate 根部的纯同步叶子，由 `voice_pipeline::context` 驱动。
- `handlers` 调用 `transcriber` / `polisher` / `recorder`。
- `dispatcher` 调用 `output`（剪贴板）与 `history`（历史记录）。
- `transcriber` 调用 `resource`；`sherpa`（内嵌 sherpa-onnx 的 SenseVoice）是当前唯一的引擎实现。
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
3. **引擎**：`Transcriber`（转写后端），当前唯一实现是 `sherpa.rs` 的本地 SenseVoice。

同 crate 内向下的模块依赖允许直接 import——`handlers` 调 `polisher` 的具体类型、各模块依赖 `config` / `error` 等底层叶子，都不需要 seam。seam 是测试注入 fake 的位置，也是未来加平台或后端时的扩展点。

三个值得注意的依赖关系：

1. **唯一一个环**：`polisher` ↔ `prompt_store`，但二者同处一个 crate 内，可接受。
2. `voice_pipeline::context.rs` 依赖 `pipeline_controller::PipelineStatus`（UI 状态枚举从底层向上泄漏了一点）。
3. `tauri_sink.rs` 通过 `PipelineEventEmitter` seam 发射事件，仅生产实现 `TauriEventEmitter` 持有 `AppHandle`（issue #104 已修）。

## 2. Dependency Direction

Dependencies are overall one-way and clear:

- The assembly layer `lib.rs` alone injects Tauri-managed state into the pipeline.
- `voice_pipeline` is the composition root, composing `handlers` / `dispatcher` / `sink` further down inside itself.
- `state_machine` is a pure synchronous leaf at the crate root, driven by `voice_pipeline::context`.
- `handlers` calls `transcriber` / `polisher` / `recorder`.
- `dispatcher` calls `output` (clipboard) and `history` (transcription history).
- `transcriber` calls `resource`; `sherpa` (SenseVoice with embedded sherpa-onnx) is currently the sole engine implementation.
- `polisher` calls `prompt_store`.
- `model` / `config` / `error` / `resource` / `audio` are low-level leaves (`audio` provides the PCM buffer and WAV encoding/decoding).

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

**Seam rules.** Three categories of boundaries must go through traits; the business core faces traits only:

1. **Frameworks**: `PipelineSink` (state/error/result callbacks), `PipelineEventEmitter` (event emission), `TranscriptionDispatch` (clipboard + history dispatch), `OverlaySink` (overlay window).
2. **Platforms**: `Recorder` (recording), `KeyListener` (keys), `Output` (clipboard); current implementations are listed in Section 5.
3. **Engines**: `Transcriber` (transcription backend); its sole implementation today is local SenseVoice in `sherpa.rs`.

Downward module dependencies within the same crate may import directly—`handlers` calling concrete types of `polisher`, or modules depending on low-level leaves like `config` / `error`, all need no seam. Seams are where tests inject fakes, and where future platforms or backends plug in.

Three dependency relationships worth noting:

1. **The only cycle**: `polisher` ↔ `prompt_store`; both live in the same crate, which is acceptable.
2. `voice_pipeline::context.rs` depends on `pipeline_controller::PipelineStatus` (a UI state enum leaks slightly upward from the bottom).
3. `tauri_sink.rs` emits events through the `PipelineEventEmitter` seam; only the production implementation `TauriEventEmitter` holds the `AppHandle` (fixed in issue #104).

## 三、核心流水线

## 3. Core Pipeline

### 主循环

`voice_pipeline/context.rs` 的主循环是 tokio::select! 三分支：

1. **按键事件** → `machine.process(ev)`。
2. **状态机超时** → `machine.poll_timeout()`（仅在 `deadline.is_some()` 时启用）。
3. **停止信号** → `break`。

命令由状态机同步返回，`match cmd` 后**就地**调用 `handle_start_record` 或 `handle_stop_record`。**没有独立的“命令通道”**——状态机不自己执行副作用，只是把意图交给调用方。

### Main Loop

The main loop in `voice_pipeline/context.rs` is a three-branch tokio::select!:

1. **Key events** → `machine.process(ev)`.
2. **State machine timeout** → `machine.poll_timeout()` (enabled only when `deadline.is_some()`).
3. **Stop signal** → `break`.

Commands return synchronously from the state machine; after `match cmd`, `handle_start_record` or `handle_stop_record` is invoked **in place**. There is **no separate "command channel"**—the state machine does not perform side effects itself; it merely hands intent to the caller.

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

### State Machine

`state_machine.rs` is a pure synchronous leaf at the crate root with 5 states:

| State | Meaning |
|------|------|
| `Idle` | Idle |
| `PotentialPress` | After key-down, waiting on whether the long-press threshold is reached |
| `Recording` | Triggered by long press; stops on release |
| `WaitSecondClick` | After a short press is released, waiting for a double click |
| `ContinuousRecording` | Triggered by double click; press once more to stop |

Via `next_deadline()`, the state machine exposes the next timeout point to the outer layer, driven by `tokio::time::sleep_until`.

### 录音 → 转写 → 润色 → 分发

`handlers.rs` 的 `handle_stop_record` 调用链如下：

| 步骤 | 输入 | 输出/行为 | 出错处理 |
|------|------|-----------|----------|
| 停止录音 | `&dyn Recorder` | WAV 字节 | 报错，回 `Idle` |
| 转写 | WAV + 进度回调 | `TranscribeResult` | `on_error` + 回 `Idle` |
| 空文本过滤 | 转写文本 | 直接 `done` | 仅跳过 |
| 润色 | `raw_text` + `LLMFormatter` | 润色后文本 | 降级为 `raw_text` |
| 结果分发 | `TranscriptionResult` | 浮窗 + 剪贴板 + 历史 | 剪贴板/历史失败只 warn |

关键点：

- 转写失败、润色失败都是**可恢复降级**。
- 剪贴板失败、历史追加失败只 `tracing::warn!`，不中断结果返回（见 `process_transcription_result`）。

### Recording → Transcription → Polishing → Dispatch

The call chain of `handle_stop_record` in `handlers.rs`:

| Step | Input | Output/behavior | Error handling |
|------|------|-----------|----------|
| Stop recording | `&dyn Recorder` | WAV bytes | Error out, return to `Idle` |
| Transcribe | WAV + progress callback | `TranscribeResult` | `on_error` + return to `Idle` |
| Empty-text filter | Transcribed text | Straight to `done` | Skip only |
| Polish | `raw_text` + `LLMFormatter` | Polished text | Degrade to `raw_text` |
| Result dispatch | `TranscriptionResult` | Overlay + clipboard + history | Clipboard/history failures only warn |

Key points:

- Both transcription failure and polishing failure are **recoverable degradations**.
- Clipboard failure and history-append failure only emit `tracing::warn!` and never interrupt returning the result (see `process_transcription_result`).

### 本地引擎：内嵌常驻

`SherpaTranscriber`（`sherpa.rs`）内嵌 sherpa-onnx 跑本地 SenseVoice int8 模型。sherpa-onnx 编译进主程序，模型在管道启动时加载一次并常驻内存，之后每句话直接推理（`accept_waveform` → `decode`），没有进程启动与冷载成本。推理是 CPU 密集同步操作，经 `spawn_blocking` 放入阻塞线程池。模型文件缺失或加载失败在构造期报错（`TranscriberError::ModelLoadFailed`）。

### Local Engine: Embedded and Resident

`SherpaTranscriber` (`sherpa.rs`) embeds sherpa-onnx to run the local SenseVoice int8 model. sherpa-onnx is compiled into the main program; the model loads once at pipeline start and stays resident in memory, after which every utterance is inferred directly (`accept_waveform` → `decode`) with no process startup or cold-load cost. Inference is a CPU-intensive synchronous operation, dispatched to the blocking thread pool via `spawn_blocking`. Missing model files or load failure error out at construction time (`TranscriberError::ModelLoadFailed`).

### 润色 prompt 三级回退

`polisher.rs` 构造 `LLMFormatter` 时，`from_config_with_sources` 统一驱动三级回退（`build_prompt_source_chain`）：

1. `PromptStore` 加载的 `resources/prompts/` 模板（`base.txt` + 档级后缀）。
2. 配置里的 `system_prompt`（非空时）。
3. 内置 hardcoded 默认提示。

启动时加载一次，改文件需重启生效。

### Three-Level Prompt Fallback for Polishing

When `polisher.rs` constructs `LLMFormatter`, `from_config_with_sources` drives a unified three-level fallback (`build_prompt_source_chain`):

1. Templates loaded by `PromptStore` from `resources/prompts/` (`base.txt` + per-level suffixes).
2. The `system_prompt` from configuration (when non-empty).
3. Built-in hardcoded default prompts.

Templates load once at startup; changing the files requires an app restart to take effect.

### 主循环阻塞是有意设计

`handle_stop_record` 在 select 分支内被 `await`，一次转写/润色会阻塞整个按键循环直到完成。这是**有意设计**——altgo 的转写是“按一次键录一句”的单发操作，阻塞保证一次只完成一次转写。详见 ADR-0003。

### Blocking the Main Loop Is Intentional

`handle_stop_record` is awaited inside the select branch, so one transcription/polish blocks the entire key loop until it completes. This is **by design**—altgo transcription is a one-shot “press once, dictate one sentence” operation, and blocking guarantees at most one transcription completes at a time. See ADR-0003 for details.

## 四、错误模型

`error.rs` 把错误分为两层：

| 分类 | 用途 | 典型枚举 |
|------|------|----------|
| `FatalError` | 构建期，管道不启动 | `ModelNotFound` / `ApiAuthFailed` / `KeyListenerFailed` / `TranscriberInitFailed` / `PolisherInitFailed` / `RecorderInitFailed` |
| `RecoverableError` | 运行时，降级继续 | `TranscriptionFailed` / `PolishingFailed` / `RecordingFailed` / `EmptyTranscription` |

模块边界一律返回自定义 thiserror 枚举：`TranscriberError` / `PolisherError` / `RecorderError` / `OutputError` / `KeyListenerError` / `ModelError` / `ConfigError` / `HistoryError`。`recorder` 模块有专门测试防止 trait 边界回退到 `anyhow`。

一个小不一致：运行时 handler 里的错误实际走 `to_string()` + `sink.on_error`，结构化 `PipelineError` 主要用于构建期——两套机制并行存在。

## 4. Error Model

`error.rs` splits errors into two tiers:

| Category | Purpose | Typical variants |
|------|------|----------|
| `FatalError` | Build time; the pipeline does not start | `ModelNotFound` / `ApiAuthFailed` / `KeyListenerFailed` / `TranscriberInitFailed` / `PolisherInitFailed` / `RecorderInitFailed` |
| `RecoverableError` | Runtime; degrade and continue | `TranscriptionFailed` / `PolishingFailed` / `RecordingFailed` / `EmptyTranscription` |

Module boundaries always return their own thiserror enums: `TranscriberError` / `PolisherError` / `RecorderError` / `OutputError` / `KeyListenerError` / `ModelError` / `ConfigError` / `HistoryError`. The `recorder` module has dedicated tests that keep the trait boundary from slipping back to `anyhow`.

One small inconsistency: errors in runtime handlers actually flow through `to_string()` + `sink.on_error`, while structured `PipelineError` serves mainly at build time—the two mechanisms coexist.

## 五、平台抽象

支持范围：Linux（Ubuntu 22.04+）的 x86_64 与 aarch64，Windows 10+ 的 x86_64 与 arm64。平台服务全部收在 trait 后面，各模块的实现文件按平台命名为 `linux.rs` / `windows.rs`——这是加新平台时的扩展点，也是测试注入 fake 的接缝：

| 模块 | Trait | Linux 实现 | Windows 实现 |
|------|-------|------------|--------------|
| `key_listener` | `KeyListener::start()` | X11 用 `xinput test-xi2`、失败回退 `evtest`；Wayland 会话优先 `evtest` | WH_KEYBOARD_LL 低级键盘钩子 |
| `recorder` | `Recorder::start_recording/stop_recording/is_recording` | `parecord` 子进程 | cpal/WASAPI |
| `output` | `Output::write_clipboard` + `clone_box` | `xclip`/`xsel`/`wl-copy` 探测一次 | arboard 剪贴板 + SendInput 文本注入 |
| `key_capture` | 无（自由函数） | `evtest` 监听 `/dev/input/event*` 等一次按键 | 临时 WH_KEYBOARD_LL 钩子等一次按键 |

另有两处平台相关但不走 trait 的分支：`display_backend.rs` 在 GUI 初始化前探测 Wayland 会话并切 GDK 后端；悬浮窗的主显示器几何在 `overlay/tauri.rs` 内按平台取值——Linux 解析 `xrandr` 输出，Windows 用 Tauri `primary_monitor()`。

各平台录音统一输出 16kHz 单声道 16 位 PCM，`audio.rs` 在录音停止时编码为 WAV——这是 SenseVoice 唯一接受的输入格式。

`Box<dyn Trait>` 用于 `PipelineContext` 字段与 builder 返回值；`Arc<dyn Trait>` 用于 Tauri 侧注入。

## 5. Platform Abstraction

Supported platforms: x86_64 and aarch64 on Linux (Ubuntu 22.04+); x86_64 and arm64 on Windows 10+. All platform services sit behind traits, with per-module implementation files named after the platform as `linux.rs` / `windows.rs`—this is the extension point for adding platforms and the seam where tests inject fakes:

| Module | Trait | Linux implementation | Windows implementation |
|------|-------|------------|--------------|
| `key_listener` | `KeyListener::start()` | X11 uses `xinput test-xi2`, falling back to `evtest`; Wayland sessions prefer `evtest` | WH_KEYBOARD_LL low-level keyboard hook |
| `recorder` | `Recorder::start_recording/stop_recording/is_recording` | `parecord` subprocess | cpal/WASAPI |
| `output` | `Output::write_clipboard` + `clone_box` | Probes `xclip`/`xsel`/`wl-copy` once | arboard clipboard + SendInput text injection |
| `key_capture` | None (free functions) | `evtest` watching `/dev/input/event*` for a single keystroke | Temporary WH_KEYBOARD_LL hook waiting for one keystroke |

Two platform-specific branches bypass traits: `display_backend.rs` probes for a Wayland session before GUI init and switches the GDK backend; the overlay's primary-monitor geometry is fetched per platform inside `overlay/tauri.rs`—Linux parses `xrandr` output, Windows uses Tauri `primary_monitor()`.

Recording on every platform uniformly outputs 16kHz mono 16-bit PCM, encoded to WAV by `audio.rs` when recording stops—the only input format SenseVoice accepts.

`Box<dyn Trait>` is used for `PipelineContext` fields and builder return values; `Arc<dyn Trait>` for injection on the Tauri side.

## 六、IPC 契约面

## 6. IPC Contract Surface

### 命令

`cmd.rs` 暴露 17 个命令：

- 配置 4：`get_config`、`save_config`、`capture_activation_key`、`test_polisher_connection`
- 更新 2：`check_update`、`install_update`
- 流水线 1：`start_pipeline`
- 浮窗 2：`copy_text`、`hide_overlay`
- 模型 4：`list_models`、`download_model`、`delete_model`、`resolve_model`
- 历史 4：`list_history`、`delete_history_entries`、`clear_history`、`polish_history_entry`

### Commands

`cmd.rs` exposes 17 commands:

- Config, 4: `get_config`, `save_config`, `capture_activation_key`, `test_polisher_connection`
- Updates, 2: `check_update`, `install_update`
- Pipeline, 1: `start_pipeline`
- Overlay, 2: `copy_text`, `hide_overlay`
- Models, 4: `list_models`, `download_model`, `delete_model`, `resolve_model`
- History, 4: `list_history`, `delete_history_entries`, `clear_history`, `polish_history_entry`

### 事件

共 11 个 Tauri 事件：

`pipeline-status`、`pipeline-error`、`transcription-result`、`polish-failed`、`transcription-progress`、`audio-level`、`key-listener-backend`、`history-updated`、`overlay-state`、`model-download-progress`、`model-download-finished`。

`polish-failed` 携带润色失败原因字符串，在 `transcription-result` 之前发出；悬浮窗 done 阶段据此显示“润色失败，已使用原文”。`audio-level` 在录音期间以固定 100ms 间隔（10 次/秒）定时派发感知音量给悬浮窗，驱动录音阶段的实时波形。

### Events

There are 11 Tauri events in total:

`pipeline-status`, `pipeline-error`, `transcription-result`, `polish-failed`, `transcription-progress`, `audio-level`, `key-listener-backend`, `history-updated`, `overlay-state`, `model-download-progress`, `model-download-finished`.

`polish-failed` carries the polishing failure reason string and is emitted before `transcription-result`; the overlay shows “Polishing failed, raw text used” during its done phase accordingly.`audio-level` dispatches perceived loudness to the overlay on a fixed 100ms timer (10 events per second) while recording, driving the live waveform.

### 序列化契约

- IPC 与 `history.json` 使用 `camelCase`。
- `config.toml` 使用 `snake_case`。
- 前端永远只见 camelCase；Rust 内部 snake_case，靠 `serde(rename_all)` 在边界转换。

### Serialization Contract

- IPC and `history.json` use `camelCase`.
- `config.toml` uses `snake_case`.
- The frontend sees camelCase only; Rust internals stay snake_case, converted at the boundary via `serde(rename_all)`.

## 七、质量 / 可维护性观察点

## 7. Quality / Maintainability Observations

### 结构性

- `voice_pipeline::context.rs` 依赖 `pipeline_controller::PipelineStatus`，UI 状态枚举从底层向上泄漏了一点。

### Structural

- `voice_pipeline::context.rs` depends on `pipeline_controller::PipelineStatus`; a UI state enum leaks slightly upward from the bottom.

### 文档漂移 / 死代码

- `notify-send` 从未实现，结果展示统一走 Tauri overlay。
- `prompt_store` 没有热重载，改文件需重启。

### Doc Drift / Dead Code

- `notify-send` was never implemented; result display goes through the Tauri overlay uniformly.
- `prompt_store` has no hot reload; changing files requires a restart.

### 架构资产

- `voice_pipeline` 完全不 import Tauri，全靠 trait seam。
- 状态机、`overlay/manager` 都是干净的叶子/纯逻辑，测试覆盖好。
- 回退链设计成熟：`prompt` 三级、模型下载官方→镜像。
- 错误分类致命/可恢复边界清晰。

### Architecture Assets

- `voice_pipeline` imports no Tauri at all, relying entirely on trait seams.
- The state machine and `overlay/manager` are clean leaves/pure logic with good test coverage.
- Fallback chains are mature designs: three-level `prompt`, official → mirror for model downloads.
- The fatal/recoverable error classification has crisp boundaries.
