# 领域术语表

本文件定义 altgo 代码库中使用的术语。写代码、文档和架构讨论时，请原样使用这些词。

## 核心流水线

**语音流水线（Voice Pipeline）**
端到端处理链：按键 → 录音 → 转写 → 润色 → 输出。由状态机驱动，运行时由 `PipelineController` 管理。

**转写引擎（Transcription Engine）**
把 WAV 音频转成文本的后端。当前唯一实现是本地 `SherpaTranscriber`（内嵌 sherpa-onnx 的 SenseVoice）。

**提供商预设（Provider Preset）**
预配置的 API 提供商模板，包含名称、base URL、API 格式、推荐模型与分类。当前仅用于润色设置。存放在 `frontend/src/config/modelPresets.ts`。

**模型目录（Model Catalog）**
与某提供商预设关联的推荐模型列表。每条含模型 ID、显示名、描述、上下文窗口与输入模态（文本/音频/图像）。用户可从目录中挑选，无需手动输入模型名。

**提供商分类（Provider Category）**
润色 API 提供商的分类：`official`（OpenAI、Anthropic）、`cn_official`（DeepSeek、Kimi、智谱）、`aggregator`（SiliconFlow、OpenRouter）。决定预设选择器 UI 中的显示顺序与分组。

**管道状态（Pipeline Status）**
语音管道任意时刻的生命周期阶段：`Idle`、`Recording`、`Processing`、`Done`、`Stopped`。在 Rust 后端以 `PipelineStatus` 枚举表示，跨 IPC 边界序列化为小写字符串给前端。

**PipelineController**
拥有管道运行句柄与共享的 `PipelineStatus` Arc。负责 start、stop、restart。不知道如何生成管道——调用方注入 spawn 闭包，使本模块不依赖 Tauri 与 sink。位于 `pipeline_controller.rs`。

**PipelineSink**
接收运行中管道事件的 trait：状态变更、进度、错误、转写结果。`tauri_sink.rs` 的 `TauriPipelineSink` 是生产环境唯一的实体适配器。转写结果路径把业务工作（剪贴板写入 + 历史追加）委托给构造时注入的 `TranscriptionDispatch` trait 对象；sink 本身只发 Tauri 事件并切换浮窗状态。

## 配置

**ConfigStore**
在 `Mutex` 后持有内存中的活动配置，连同文件路径。暴露 `snapshot`、`snapshot_blocking`、`apply_patch`。所有配置变更都经 `apply_patch` 校验并写盘。校验失败时内存已部分应用、不落盘（非原子回滚）。位于 `config_store.rs`。

**ConfigPatch**
配置的部分更新：所有字段可选，未提供的字段保持不变。`linux_evdev_code` 字段用三态反序列化器区分缺省（不修改）与 JSON `null`（清除已存代码）。这是 `save_config` 经 IPC 接受的类型。

## 历史

**HistoryStore**
包装历史 JSON 文件，暴露具名操作：`list`、`count`、`append`、`delete`、`clear`、`get`、`update_text`。调用方从不接触文件路径或模块私有辅助函数。位于 `history.rs`。每个实例克隆便宜（只含一个 `PathBuf`）。

**历史条目（HistoryEntry）**
单条转写记录：`id`、`createdAtMs`、`rawText`（转写原文）、`text`（润色后，或与原文相同）。永不存音频。

## 输出

**悬浮窗（Overlay）**
录音、处理与结果显示时出现的悬浮状态窗口。定位在主显示器（Linux 解析 `xrandr` 输出，Windows 用 Tauri `primary_monitor()`），位置可配置（`gui.overlay_position`：`bottom_center` 默认 / `top_center`），对全部阶段生效。Wayland 协议不允许客户端定位窗口（`set_position` 为 no-op），因此启动时检测会话：Wayland 且未显式设置 `GDK_BACKEND` 时切到 X11 后端（XWayland），定位才能生效。状态切换时由 `TauriPipelineSink` 经 `OverlaySink` 抽象管理；`TauriPipelineSink` 只描述意图（"recording" / "processing" / "hidden" / "done"），浮窗管理器把它转成窗口尺寸、位置、显示与隐藏。窗口在全部阶段使用一个固定尺寸——会话中调整透明窗口尺寸会在 Linux 合成器上产生黑色边缘，所以阶段切换只替换前端内容（CSS 交叉淡入）。`hidden` 先发出，实际隐藏延迟约 220ms 以便退出动画可见。转写结果路径上，`transcription-result` 在 `done` 浮窗状态之前发出，前端不会渲染出空 island。island 不使用 `box-shadow`——半透明阴影叠加在透明窗口上会在某些 Linux 合成器上合成出暗色光晕。

**自动淡出（Auto-fade）**
结果浮窗（done 阶段）的退出策略：无用户输入活动时无限保留，检测到输入活动后延时数秒自动隐藏。_Avoid_: 自动关闭、auto-hide。

**文本注入（Text Injection）**
转写完成后把最终文本一次性输入到当前焦点窗口光标处的输出动作，仅 Windows 提供；由 `inject_text` 配置控制，默认关闭。_Avoid_: 自动粘贴、流式插入。

**润色器（Polisher）**
可选的 LLM 后处理步骤。由 `PolishLevel`（`none`/`light`/`medium`/`heavy`）控制。与任意 OpenAI 兼容聊天 API 或 Anthropic Messages API 通信。

**PromptStore**
管理润色器的 prompt 模板文件：从 `resources/prompts/` 加载，把 base + 各档后缀组合成完整系统 prompt，首次使用时校验。启动时加载一次，改文件需重启。校验失败时降级（退回自定义或内置 prompt，润色继续用原文）。

**Prompt 模板（Prompt Template）**
`resources/prompts/` 中的文本文件：`base.txt`（共享指令 + 中文写作指导）或 `{level}-suffix.txt`（追加到 base 之后的各档指令）。运行时组合产生发给 LLM 的完整系统 prompt。

**系统 prompt（System Prompt）**
发给 LLM 用于润色的完整 prompt 文本，组合自 `base.txt` + `{level}-suffix.txt`，缓存于内存，启动时加载一次。

## 录音

**录音输出格式（Recorder Output Format）**
语音管道期望录音器以固定的 16kHz、单声道、16 位 PCM WAV 字节返回音频。SenseVoice 只接受这一采样率，其他配置值会在管道启动前被拒绝。

**音频电平（Audio Level）**
录音期间由录音器从实时 PCM 音频块计算出的感知音量大小（范围 0.0 ~ 1.0）。录音线程在读取音频流时计算均方根（RMS）并通过非线性增益映射为感知电平，经 `PipelineSink::on_audio_level` 记录最新电平，录音期间由 Tauri `audio-level` 事件以固定 100ms 间隔（10 次/秒）定时派发给悬浮窗，作为电平轨迹的实时采样来源。

**电平轨迹（Level Trace）**
录音期间由连续音频电平采样累积成的滚动历史，显示最近一段时间的说话痕迹；松开激活键后冻结保留，直到结果浮窗出现。_Avoid_: 声纹、波形图。


## 按键输入

**按键监听器（KeyListener）**
管道运行期间持续监听配置的激活键并发出 `KeyEvent` 的接口。Linux 实现（`X11Listener`）在 X11 用 `xinput test-xi2`、失败回退 `evtest`，Wayland 会话优先 `evtest`；Windows 用 WH_KEYBOARD_LL 低级键盘钩子。管道以 `Box<dyn KeyListener>` 消费它。
_Avoid_: key listener（指概念时小写）、platform listener。

**按键捕获（KeyCapture）**
设置配置期间一次性捕获任意物理键的接口，返回 `KeyListenerConfig` 所需的键标识符（`key_name`、`linux_evdev_code`）。Linux 通过 `evtest` 子进程监听 `/dev/input/event*`；Windows 临时挂 WH_KEYBOARD_LL 钩子等待一次按键。暴露同步阻塞式 API。
_Avoid_: capture mode、key capture mode。

**激活键（Activation Key）**
按住即开始录音的物理键。按设备配置为 X11 keysym 名（`key_name`）或 evdev 扫描码（`linux_evdev_code`）。Wayland 上优先 evdev 路径。

**状态机（State Machine）**
把原始按键事件翻译成管道 `StartRecord` / `StopRecord` 命令的 5 状态 FSM（`Idle → PotentialPress → Recording → WaitSecondClick → ContinuousRecording`）。

## 更新

**应用更新器（App Updater）**
负责检查、下载和安装应用新版本的模块。支持后台静默检查（启动时）与用户手动检查（设置页触发）。

**检查模式（Check Mode）**
检查更新的触发模式：`Silent`（静默模式，启动时触发，失败时不打扰用户，发现新版本时以轻量徽标提示）与 `Manual`（手动模式，用户主动点击触发，带加载状态并在超时或失败时反馈具体原因）。

**更新支持级别（Update Support Tier）**
不同平台及打包分发方式下的更新能力分级：
- `InPlace`（就地更新）：Windows（NSIS）与 Linux（AppImage），支持由更新器自动下载差量/全量包并就地替换重启。
- `External`（外部引导）：Linux 传统包管理分发（deb、rpm、AUR），因需要系统提权或由系统包管理器托管，更新器提示新版本变更并提供一键打开下载页或包管理器更新命令。

# Domain Glossary

This file defines the terms used across the altgo codebase. Use them verbatim in code, docs, and architecture discussions.

## Core Pipeline

**Voice Pipeline**
The end-to-end processing chain: key press → record → transcribe → polish → output. Driven by the state machine; managed at runtime by `PipelineController`.

**Transcription Engine**
The backend that turns WAV audio into text. Currently a single implementation: the local `SherpaTranscriber` (SenseVoice via embedded sherpa-onnx).

**Provider Preset**
A preconfigured API provider template: name, base URL, API format, recommended models, and category. Currently used only by polisher settings. Lives in `frontend/src/config/modelPresets.ts`.

**Model Catalog**
The list of recommended models associated with a provider preset. Each entry carries a model ID, display name, description, context window, and input modalities (text/audio/image). Users pick from the catalog instead of typing model names.

**Provider Category**
Classification of polisher API providers: `official` (OpenAI, Anthropic), `cn_official` (DeepSeek, Kimi, Zhipu), `aggregator` (SiliconFlow, OpenRouter). Determines ordering and grouping in the preset picker UI.

**Pipeline Status**
The lifecycle phase of the voice pipeline at any moment: `Idle`, `Recording`, `Processing`, `Done`, `Stopped`. Represented in the Rust backend as the `PipelineStatus` enum and serialized as lowercase strings to the frontend across the IPC boundary.

**PipelineController**
Owns the pipeline run handle plus a shared `PipelineStatus` Arc. Handles start, stop, restart. It never knows how to spawn the pipeline—callers inject the spawn closure, keeping this module free of Tauri and sink dependencies. Located in `pipeline_controller.rs`.

**PipelineSink**
Trait receiving events from a running pipeline: status changes, progress, errors, transcription results. The concrete production adapter is `TauriPipelineSink` in `tauri_sink.rs`. On the transcription-result path it delegates business work (clipboard write + history append) to the `TranscriptionDispatch` trait object injected at construction; the sink itself only emits Tauri events and switches overlay states.

## Configuration

**ConfigStore**
Holds the in-memory live config behind a `Mutex`, along with its file path. Exposes `snapshot`, `snapshot_blocking`, and `apply_patch`. Every config change goes through `apply_patch` for validation and persistence. When validation fails, memory may be partially applied while nothing is written to disk (non-atomic rollback). Located in `config_store.rs`.

**ConfigPatch**
A partial config update: all fields optional; unprovided fields stay unchanged. The `linux_evdev_code` field uses a three-state deserializer distinguishing absent (no change) from JSON `null` (clear stored code). This is the type accepted over IPC by `save_config`.

## History

**HistoryStore**
Wraps the history JSON file with named operations: `list`, `count`, `append`, `delete`, `clear`, `get`, `update_text`. Callers never touch file paths or module-private helpers. Located in `history.rs`. Cloning an instance is cheap (a single `PathBuf` inside).

**HistoryEntry**
One transcription record: `id`, `createdAtMs`, `rawText` (original transcript), `text` (polished, or equal to raw when unpolished). Audio is never stored.

## Output

**Overlay**
The floating state window shown during recording, processing, and result display. Positioned on the primary monitor (Linux parses `xrandr` output; Windows uses Tauri's `primary_monitor()`); placement is configurable (`gui.overlay_position`: `bottom_center` default / `top_center`) and applies to all phases. The Wayland protocol forbids client-side window positioning (`set_position` is a no-op), so the session is probed at startup: on Wayland without an explicit `GDK_BACKEND` we switch to the X11 backend (XWayland) so positioning works. State switching is driven by `TauriPipelineSink` through the `OverlaySink` abstraction; the sink only states intent ("recording" / "processing" / "hidden" / "done") and the overlay manager translates that into window size, position, show, and hide. The window uses one fixed size across all phases—resizing a transparent window mid-session produces black edges on Linux compositors, so phase changes merely swap frontend content (CSS crossfade). `hidden` is emitted first; actual hiding is delayed ~220ms so the exit animation stays visible. On the transcription-result path, `transcription-result` is emitted before the `done` overlay state, sparing the frontend an empty island. The island avoids `box-shadow`—a translucent shadow layered on a transparent window renders dark halos on some Linux compositors.

**Auto-fade**
Exit policy of the result overlay (done phase): stays indefinitely without user input activity, hides automatically seconds after input activity is detected. _Avoid_: auto-close, auto-hide.

**Text Injection**
Output action typing the final text once at the cursor of the focused window after transcription; Windows only, governed by the `inject_text` setting, off by default. _Avoid_: auto-paste, streaming insert.

**Polisher**
Optional LLM post-processing step, controlled by `PolishLevel` (`none`/`light`/`medium`/`heavy`). Talks to any OpenAI-compatible chat API or the Anthropic Messages API.

**PromptStore**
Manages the polisher's prompt template files: loaded from `resources/prompts/`, combining base + per-level suffixes into complete system prompts, validated at first use. Loaded once at startup; file edits need a restart. Validation failures degrade gracefully (fall back to custom or built-in prompts; polishing continues with raw text).

**Prompt Template**
Text files under `resources/prompts/`: `base.txt` (shared instructions + Chinese writing guidance) or `{level}-suffix.txt` (per-level instructions appended to base). Composed at runtime into the full system prompt sent to the LLM.

**System Prompt**
The full prompt text sent to the LLM for polishing, composed from `base.txt` + `{level}-suffix.txt`, cached in memory, loaded once at startup.

## Recording

**Recorder Output Format**
The voice pipeline expects recorders to return audio as fixed 16kHz mono 16-bit PCM WAV bytes. SenseVoice accepts only this sample rate; other settings are rejected before the pipeline starts.

**Audio Level**
Perceptual loudness (0.0–1.0) computed by the recorder from realtime PCM chunks during recording. The recording thread computes RMS while reading the stream and maps it through nonlinear gain onto a perceptual level; recorded via `PipelineSink::on_audio_level` and dispatched to the overlay as the Tauri `audio-level` event on a fixed 100ms timer (10 events per second) while recording, feeding the overlay's level trace.

**Level Trace**
A rolling history built from continuous audio-level samples during recording, showing recent speech activity; frozen once the activation key is released, kept until the result overlay appears. _Avoid_: voiceprint, waveform.


## Key Input

**KeyListener**
Interface that continuously watches the configured activation key while the pipeline runs, emitting `KeyEvent`s. The Linux implementation (`X11Listener`) uses `xinput test-xi2` on X11 with an `evtest` fallback, preferring `evtest` on Wayland sessions; Windows uses a WH_KEYBOARD_LL low-level hook. Consumed by the pipeline as `Box<dyn KeyListener>`.
_Avoid_: key listener (lowercase when referring to the concept), platform listener.

**KeyCapture**
One-shot interface capturing any physical key during configuration, returning the identifiers `KeyListenerConfig` needs (`key_name`, `linux_evdev_code`). Linux listens on `/dev/input/event*` through an `evtest` subprocess; Windows temporarily installs a WH_KEYBOARD_LL hook awaiting one press. Exposes a synchronous blocking API.
_Avoid_: capture mode, key capture mode.

**Activation Key**
The physical key held down to start recording. Configured per device either as an X11 keysym name (`key_name`) or an evdev scancode (`linux_evdev_code`). The evdev path is preferred on Wayland.

**State Machine**
The 5-state FSM translating raw key events into pipeline `StartRecord` / `StopRecord` commands (`Idle → PotentialPress → Recording → WaitSecondClick → ContinuousRecording`).

## Updates

**App Updater**
Module checking for, downloading, and installing new app versions. Supports background silent checks (at startup) and user-initiated manual checks (from the Settings page).

**Check Mode**
Trigger pattern of an update check: `Silent` (at startup; failures never disturb the user; a lightweight badge appears when a new version is found) versus `Manual` (user-initiated click; shows a loading state and reports the concrete reason on timeout/failure).

**Update Support Tier**
How update capability is tiered across platforms and packaging/distribution formats:
- `InPlace`: Windows (NSIS) and Linux (AppImage); the updater downloads delta/full packages and replaces + restarts in place.
- `External`: traditional Linux package-manager distributions (deb, rpm, AUR), which require root privileges or are owned by the system package manager; the updater announces new versions and offers one-click access to the download page or the package-manager update command.
