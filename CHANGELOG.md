# Changelog

## v2.6.14 (2026-09-22)

### Fixes

- **修复设置页供应商目录加载失败**：v2.6.12 起供应商清单改为设置页打开时从 Oh My Pi 模型目录在线拉取，但请求发在 WebView 里，被应用的 CSP（`connect-src 'self'`）拦截，目录一律显示 `TypeError: Load failed`，无法选择供应商预设。现改为 Rust 侧拉取（与润色测试连接、模型下载同一通道），再交给前端解析，不受 WebView 网络策略限制；拉取失败仍可重试，手填 API 地址照常可用。

### Changed

- **电平轨迹事件稳定为 10 次/秒**：录音期间派发给悬浮窗的音量电平事件（`audio-level`）改为定时器驱动、固定 100ms 间隔（10 次/秒）。此前事件随录音块到达、节奏不稳，电平轨迹的采样密度忽高忽低；#147。

## v2.6.13 (2026-09-22)

### Fixes

- **修复激活键无反应**：键盘设备枚举改为解析 `/proc/bus/input/devices`，监听全部带 `kbd` handler 的输入设备。此前逻辑扫到 `/dev/input/by-id/*-kbd` 即短路，而真键盘（蓝牙、特殊接收器）可能不在 by-id，游戏鼠标的宏键盘接口反而占据该链接——Wayland 下会监听到错误设备，按住激活键无任何反应且启动无报错。同时移除对 `evtest --info` 的依赖（该选项在 evtest 中不存在，备用扫描路径从未生效过）。设置页“按下以设置”复用同一枚举，一并修复。

### Added

- **日志可观测性**：接入 `tracing-subscriber`（此前仅有 tracing 宏、无订阅器，全部日志被静默丢弃）。默认输出 info 级别，`RUST_LOG=debug altgo` 可细化排障。

## v2.6.12 (2026-09-21)

### Added

- **思考层级设置**：润色分区新增“思考层级”（`[polisher] thinking_level`，默认 `off` 维持自动关闭）。选低/中/高后请求按各家参数方言开启思考：OpenAI 兼容端点发 `reasoning_effort`，OpenRouter 发 `reasoning.effort`，通义/SiliconFlow 与智谱/Kimi/DeepSeek 等仅支持开关的服务商统一开启，Anthropic 协议按档给思考预算（1024/4096/16384 token）并自动抬高 `max_tokens`、省略与之不兼容的 `temperature`。

### Changed

- **供应商清单全面依赖 omp 目录**：删除手写的本地供应商预置（原 13 条），设置页打开时自动从 Oh My Pi 模型目录（catalog.stencil.so，与 `omp models` 命令同源，200 余家）拉取供应商与实时模型清单（会话内一次，失败可重试），选择后自动填入地址与协议；离线或目录不可达时可直接手填 API 地址与模型。目录内同端点的不同条目均展示；预置独有的官网 / API Key 链接与图标着色随预置一并移除。
- **移除从未使用的 libnotify 依赖**：deb/rpm/AUR 的依赖清单不再声明通知库（#141），CONTRIBUTING 与 README 同步清理 notify-send / notifications 残留描述。应用行为零变化——系统通知从未实现，结果展示统一走悬浮窗。

## v2.6.11 (2026-09-06)

### Changed

- **改进润色提示词**：明确区分轻度、中度和重度润色，强化对原意、事实、数字、专有名词和不确定性的保护，减少口语转写中的机械套话与 AI 腔。


## v2.6.10 (2026-08-26)

### Changed

- **DeepSeek 预设更新至 V4**：默认模型改为 `deepseek-v4-flash`（轻量快速，适合润色），备选 `deepseek-v4-pro`（重度改写）；旧 ID `deepseek-chat` / `deepseek-reasoner` 已不在官方文档中，OpenRouter 预设同步指向 `deepseek/deepseek-v4-flash`。V4 系列默认开启思考，配合本次的思考抑制补全（请求自动附带关闭参数）使用最佳。

### Fixes

- **润色思考抑制补全与输出兜底**：DeepSeek、Moonshot/Kimi、z.ai 的模型已默认开启思考，润色延迟凭空多出数秒——这些域名加入“自动关闭思考”抑制表，SiliconFlow 国际站（siliconflow.com）一并覆盖（方言表对齐 zotero-pdf-translate#1464）。同时润色出口统一剥掉响应中混入的 `<think>` 思维链残渣，Anthropic 协议响应取第一个文本块，中转端点混入思考块时不再报错或污染结果。

## v2.6.9 (2026-08-26)

### Fixes

- **修复自动更新产物缺失**：`bundle.createUpdaterArtifacts` 未配置（当前 Tauri 版本默认 `false`），此前各版本的 Release 虽注入了签名密钥却未生成签名包与 `latest.json`，自动更新从未真正可用。本版以 `v1Compatible` 模式开启，与 release 工作流已约定的产物命名（`.AppImage.tar.gz`、`.nsis.zip`、`.msi.zip` 及签名文件）匹配。

### Notes

- **自动更新自本版起生效**（v2.6.8 的启用声明作废，该版实际未携带 updater 产物）。签名公钥已于 v2.6.8 更换：v2.6.4~v2.6.7 客户端内置旧公钥，验签无法通过，这些版本的用户需手动下载安装一次新版；v2.6.8 及本版之后可正常自动更新。

## v2.6.8 (2026-08-26)

### Notes

- **自动更新链路正式启用，签名密钥已更换**：本版起 Release 携带签名的 updater 产物（`latest.json`、`.sig` 等），自动更新功能（v2.6.4 引入）真正可用。因旧私钥遗失，签名公钥已更换——**v2.6.4~v2.6.7 客户端内置旧公钥，验签无法通过，这些版本的用户需手动下载安装本版一次**；v2.6.3 及更早版本无自动更新功能，本就需手动升级。本版之后各版本间可正常自动更新。

## v2.6.7 (2026-08-26)

### Fixes

- **悬浮窗三相位统一为固定尺寸卡片**：录音 / 处理 / 完成三个阶段共用同一张 400×92px、16px 圆角的卡片，相位切换只剩内容 crossfade，窗口不再变来变去。录音阶段的电平轨迹视口固定 300px（轨迹条瘦身至 2px 宽 + 1px 间隔，完整保留最近 10 秒轨迹），录音过程中浮窗不再随轨迹增长变宽；完成阶段的文本区相应收窄，完整文本仍在剪贴板中。

## v2.6.6 (2026-08-26)

### Added

- **录音悬浮窗电平轨迹**：录音期间悬浮窗以滚动电平轨迹（最近 10 秒，每 100ms 一帧）取代原 4 根即时律动条，直观呈现“刚才讲了多久、停顿在哪”；松开触发键后轨迹冻结保留在转写阶段，结果出现后退场（#138）。
- **结果浮窗自动淡出**：转写结果浮窗不再无限挂住等手动关闭——无任何键盘/鼠标输入时一直保留；检测到输入活动（打字、切窗口、动鼠标）后 3 秒自动淡出，粘贴完不用回头点关闭。Windows 基于全局输入检测实现完整语义，Linux 因 Wayland 无统一空闲 API 降级为固定 8 秒超时淡出（#138）。
- **文本注入设置开关**：设置页新增“自动输入到光标位置”开关与 `[output] inject_text` 配置项（#138）。

### Changed

- **⚠️ Windows 文本自动输入默认改为关闭**：此前 Windows 上转写完成后会无条件把文本直接输入到当前焦点窗口的光标处，属侵入性行为且无法关闭。现在默认只写入剪贴板，需要自动输入的用户请在设置页开启“自动输入到光标位置”（#138）。**存量 Windows 用户升级后行为变化：不再自动输入，请手动粘贴或到设置中开启**。注入本身保持一次性整段输入（非流式），Linux 行为不变。

## v2.6.5 (2026-08-24)

### Added

- **Windows arm64 支持**：sherpa-onnx 升级至 1.13.6（上游已发布 Windows arm64 预编译库），CI 与 Release 工作流新增 `aarch64-pc-windows-msvc` 架构构建，发布产物新增 arm64 的 NSIS 安装包与 MSI（#131）。

### Notes

- **winget / Scoop 分发**：winget-pkgs 与 ScoopInstaller/Extras 的提交清单已入库（`packaging/winget/`、`packaging/scoop/`）并提交上游 PR；审核通过后可用 `winget install cislunarspace.altgo` 与 `scoop install extras/altgo` 安装（#130）。Flatpak / Snap 的沙箱可行性验证仍另行跟踪。

## v2.6.4 (2026-08-24)

### Added

- **应用更新检查与自动更新**：支持启动时静默检查更新并在主界面顶部提示，同时在设置页提供手动“检查更新”按钮；发现新版本后支持一键下载安装并重启（#134）。可通过设置项 `gui.auto_check_update` 控制是否在启动时自动检查。

## v2.6.3 (2026-08-24)

### Added

- **录音悬浮窗音频电平律动条**：录音期间悬浮窗以 4 根动态波形律动条替代原静态呼吸圆点，录音器实时计算 PCM 均方根（RMS）并通过感知增益曲线映射到人耳音量电平（20~30 FPS），悬浮窗随声音强弱实时起伏（#132）。

## v2.6.2 (2026-08-23)

### Fixes

- **Windows 修复文本重复输入两次**：两个 altgo 进程并存时（如开机自启后又手动启动），各自安装的键盘钩子会收到同一次按键，导致同一段录音被转写两次、文本重复输入。现在通过单实例保护解决：第二次启动时唤起已有实例的窗口并自动退出。

## v2.6.1 (2026-08-23)

### Added

- **Linux AppImage**：发布产物新增 `altgo_<版本>_<arch>.AppImage`（amd64 / arm64），免安装、覆盖 deb/rpm 之外的发行版；AppImage 不自动装依赖，缺库时请参照 README 依赖清单安装。
- **Windows MSI**：发布产物新增 `altgo_<版本>_x64_en-US.msi`，适合需要 MSI 部署方式的企业环境。

### Notes

- Windows arm64 暂不支持：内嵌的 sherpa-onnx 预编译库无 Windows arm64 版本（#131）；Flatpak / Snap / winget / Scoop 分发渠道另行跟踪（#130）。

## v2.6.0 (2026-08-23)

### Added

- **Windows 支持**：Windows 10+（x86_64）原生实现，随版本发布 NSIS 安装包（`altgo_<版本>_x64-setup.exe`）。按键监听用 WH_KEYBOARD_LL 低级键盘钩子，录音用 cpal/WASAPI（16kHz 单声道，设备不支持时自动回退默认格式并重采样），激活键捕获与 Linux“按下以设置”体验一致（配置新增 `windows_vk_code`，与 `linux_evdev_code` 对等）。CI 增加 Windows 测试与打包 job，Release 自动产出安装包。
- **转写文本自动输入**（Windows）：转写完成后除写入剪贴板外，通过 SendInput 把文本直接输入到当前焦点位置（支持中文）；Linux 行为不变（仅剪贴板）。

### Changed

- **Windows 输出全部原生实现**：旧版基于 PowerShell 子进程的剪贴板与悬浮窗方案不再恢复，剪贴板改用 arboard，悬浮窗定位改用系统显示器 API。

## v2.5.12 (2026-08-20)

### Added

- **字体与窗口尺寸调节**：设置页外观区新增字体大小（小 / 标准 / 大）与窗口大小（紧凑 560×520 / 标准 640×600 / 大 760×720）两档选择，保存后即时生效并记忆（#128）。

### Changed

- **设置页区块重排**：按“录音 → 润色 → 转写 → 外观”顺序渲染，润色区块默认展开，常用项无需再展开查找（#128）。
- **默认窗口放大**：由 520×480 提至 640×600，缓解此前窗口过窄、内容拥挤的问题（#128）。
- **供应商选择改弹窗**：从内嵌列表改为独立选择弹窗，顶部展示当前供应商摘要（图标、名称、当前模型），当前使用的预设置顶，搜索可同时匹配供应商与模型名（#128）。

### Fixes

- **供应商弹窗补齐键盘交互**：Esc 关闭、Tab 在弹窗内循环焦点、打开时自动聚焦搜索框（#128）。

## v2.5.11 (2026-08-19)

### Fixes

- **修复润色 API 地址拼接错误**：此前无条件在 base URL 后拼 `/v1/chat/completions`，而设置页大部分供应商预设（Kimi、智谱、通义、OpenAI、SiliconFlow）的地址自带 `/v1` 或 `/v4`，实际请求打到 `/v1/v1/...` 必然 404。现在按协议推导请求地址：地址不带路径时自动补 `/v1/...`，带版本路径（`/v1`、`/api/paas/v4` 等）时只接 `/chat/completions` 或 `/messages`，填写完整 endpoint 亦原样可用。
- **修复 Anthropic 协议无法从设置页生效**：`protocol` 此前不在设置保存链路里，选 Anthropic 预设后仍按 OpenAI 协议请求。现在协议随预设自动带出，也可在设置页手动选择。
- **润色失败不再静默**：实时链路润色失败只写日志并回退原文，用户无从知晓。现在失败时悬浮窗会显示“润色失败，已使用原文”（悬停可看原因）；历史页再润色在级别为“关闭”时给出明确提示。
- **配置校验补全**：润色开启时除密钥外，同时校验 API 地址与模型名非空，错误信息逐项列出缺失字段。

### Added

- **供应商预设清单迁移自 cc-switch**：设置页预设从 13 家扩至 99 家（新增“第三方中转”分组），涵盖国产官方、聚合服务与中转站的 Anthropic / OpenAI 双协议端点，全部端点与默认模型取自 cc-switch 内置清单（过滤了 OAuth、Bedrock 等不适用条目，剥离了推广参数）。
- **Anthropic 协议双鉴权头**：请求同时携带 `x-api-key` 与 `Authorization: Bearer`，官方端点与多数中转端点（用 Bearer token 鉴权）均可直接使用。
- **新增 6 家润色供应商预设**：火山方舟（豆包）、百度千帆、MiniMax、OpenRouter、Google Gemini（OpenAI 兼容端点）与本地 Ollama，预设含各自的推荐模型目录。
- **润色默认关闭“思考”**：语音润色是轻量任务，推理模型先思考再回答会多出数秒延迟与花费。现在对通义、SiliconFlow、智谱、火山方舟、MiniMax、OpenRouter 的请求自动附带各家对应的关闭思考参数（`enable_thinking: false` / `thinking: {"type": "disabled"}` / `reasoning: {"enabled": false}`）；其他服务商的推荐模型默认不思考，不发送任何额外字段。
- **设置页“测试连接”**：润色分区新增按钮，基于表单当前值（密钥留空则用已存密钥）发一次最小请求，立即反馈密钥无效、地址不对、超时等原因，无需先保存或录音验证。
- **清除已存密钥**：设置页可清除已保存的 API 密钥。

### Changed

- **刷新过期的预设模型目录**：Kimi 的 moonshot-v1 系列（2026-08-31 停服）与 kimi-k2 系列已下线，预设默认改为 `kimi-k2.6`（目录含 `kimi-k3`）；智谱 `glm-4-flash` 系列迁移为免费的 `glm-4.7-flash`；通义默认改为 `qwen-flash`；SiliconFlow 默认改为 `Qwen/Qwen3-8B`。

### CI

- apt 系统依赖换用官方主站镜像，修复 amd64 下载过慢；CI 与 Release 统一 apt 缓存 key（check job 补装 rpm）；新增每日定时预热 apt 缓存的工作流，把冷启动下载挪出 CI 与 Release。

## v2.5.10 (2026-08-19)

### Fixes

- **修复 Wayland 下悬浮窗定位失效**：Wayland 协议不允许客户端定位窗口，悬浮窗被合成器摆到屏幕中央。启动时检测会话，Wayland 且未显式设置 `GDK_BACKEND` 时切到 X11 后端（XWayland），定位恢复生效（#120、#126）。

### Added

- **悬浮窗位置可选**：新增 `gui.overlay_position` 设置（`bottom_center` 默认 / `top_center`），设置页外观区可选择，保存后随流水线重启对全部阶段生效（#126）。

### Docs

- 文档同步与精简：架构文档对齐 SenseVoice 与 Linux-only 收敛后的现状（#125）；测试文档重构为五层策略与派生数字（#124）；README 精简首次使用路径（#123）。

### CI

- 缓存 apt 系统依赖，规避 Azure 镜像慢速下载；Release 的 Tauri CLI 改用 npm 预编译版，免去逐 job 源码编译。

## v2.5.9 (2026-08-19)

### Fixes

- **修正模型下载校验值**：`tokens.txt` 的 SHA-256 校验常量与上游官方文件实际哈希不符，模型下载在校验处必然失败，内置 SenseVoice 模型完全无法启用。已改为官方文件的真实哈希；此前下载失败残留的主模型文件哈希正确，重试时自动复用，只需补下 315KB 的 `tokens.txt`。

## v2.5.8 (2026-08-14)

### Fixes

- **校验 SenseVoice 模型与输入完整性**：模型目录必须同时包含 `model.int8.onnx` 与 `tokens.txt` 才会显示为可用；录音采样率必须为 16kHz，避免把其他采样率的 WAV 错误送入 SenseVoice。
- **恢复 Linux 按键捕获映射**：设置页捕获右 Alt 等常用 evdev 键码后，会保存为可供 X11 监听路径解析的 keysym。
- **修正双架构 AUR 元数据**：生成的 PKGBUILD 同时支持 `x86_64` 与 `aarch64`，使用 SenseVoice 描述及各自的 deb 下载地址和校验和。
- **停止发布 Flatpak**：当前 Linux 运行依赖宿主级 `parecord`、`xinput`/`evtest` 和剪贴板工具，Flatpak 沙盒无法在不额外捆绑整套系统工具的情况下可靠完成核心流程；发布产物收敛为 deb、rpm 和双架构 AUR PKGBUILD。

### Changed

- **移除云端转写与 Windows/macOS 支持**：删除 `WhisperApi`、`MimoAsr`、转写引擎切换、转写 API 密钥/地址配置和全部 Windows 适配器；项目现在只支持 Linux（x86_64 / aarch64），本地 SenseVoice 是唯一转写方式。旧配置中的云端转写字段会被忽略，文本润色的云端 API 配置保留。
- **本地转写引擎换成 SenseVoice（sherpa-onnx 内嵌）**：移除 whisper.cpp 子进程体系（`whisper-cli`/`whisper-server`、`download-deps.sh`、CUDA 打包、whisper-prebuild workflow、`whisper_path`/`beam_size` 配置）。sherpa-onnx 编译进主程序，模型在管道启动时加载一次并常驻内存，之后每句话直接推理，无进程启动与冷载开销；SenseVoice 支持中/英/日/韩/粤自动检测，CPU 实时率远高于 whisper。模型下载体系改为 SenseVoice int8（`model.int8.onnx` + `tokens.txt`，约 230MB）；旧 GGML 模型需在设置页重新下载。打包不再捆绑转写二进制，deb/rpm 体积同步下降。

## v2.5.7 (2026-08-14)

### Fixes

- **修复转写完成后进度转发卡住**：进度回调此前经 mpsc 通道转发给 sink，并在转写完成后等待转发任务退出；若转写器保留回调、返回结果后才调用（如本地 whisper 完成时），转发任务永不退出，`handle_stop_record` 挂起、流水线停在转写中。改为回调直接同步转发到 sink，不再等待转发任务（#116）。

### Changed

- **CI/release 提速与加固**：固定 runner 版本（ci.yml 与 release 的 Windows job 用 `windows-2025`，release 的 release job 与 docs 部署用 `ubuntu-24.04`）；release 的 flatpak 构建补上 `Swatinem/rust-cache` 增量编译；各 workflow 的 `actions/checkout` 加 `persist-credentials: false`（构建/上传走注入的 GITHUB_TOKEN，不依赖 checkout 持久凭据）。参考 e2m2e#405。

### Docs

- **README 重组**：重组“使用”与“开发”文档结构，修复失效链接（#116）。

## v2.5.6 (2026-08-12)

### Refactor

- **移除 IPC 死代码命令**：`get_status` 与 `stop_pipeline` 已注册但前端零调用（状态由 `tauri_sink` 推送 `pipeline-status` 事件、停止由 `save_config` 内部 `restart_pipeline` 完成），删除命令与注册，IPC 契约 16 → 14；同步清理 architecture.md / CLAUDE.md 中“疑为死代码”标注（#112）。

## v2.5.5 (2026-08-11)

### Fixes

- **回收 evtest 子进程**：Linux 激活键捕获成功或超时后都正确等待 `evtest` 退出，避免长期运行积累僵尸进程（#115）。
- **配置保存与流水线重启一致**：保存配置时复用已校验快照并串行化保存与重启；写盘失败自动回滚内存配置，避免重启读取旧配置或并发保存覆盖新配置（#114）。

## v2.5.4 (2026-08-09)

### Docs

- **README 文本润色**：精简开头与若干冗余表述，统一在线文档链接文本。
- **安装章节对齐实际产物**：release 实际提供 deb / rpm / Flatpak / MSI，README 此前提了从未上架的 AppImage；改为列出三种 Linux 格式并补 rpm / Flatpak 安装命令，Releases 链接改用完整 URL。
- **清理过时的 AppImage / ffmpeg 引用**：docs-site（quick-start、usage、首页）与 CONTRIBUTING 同步更新；CONTRIBUTING 删除指向已不存在的 `appimage.yml` workflow 的条目。
- **移除 README 中已废弃的“桌面通知”描述**：桌面通知在 v2.4.5（#63）随 notify 输出路径删除，README 仍保留该功能描述；本次清除功能列表、设置、架构图、模块表中的过时引用。

### Fixes

- **`build.ps1` 的 ffmpeg 残留检查**：v2.5.1 移除 ffmpeg 后漏改，`ffmpeg.exe` 检查恒为缺失、每次构建白跑一遍 `download-deps-windows.ps1`；改为只检查 `whisper-cli.exe`。

## v2.5.3 (2026-07-22)

### CI

- **新增 Windows Check job**：CI 在 windows-latest 跑 fmt + clippy + test（不打包），`cfg(windows)` 代码结束零覆盖（#98）。
- **修复 Windows 上 `cargo test` 无法运行**：rfd 静态导入 `TaskDialogIndirect` 需要 comctl32 v6 manifest，测试二进制此前启动即 0xc0000139；改为 build.rs 统一向 exe 与测试注入 manifest。
- **tauri_sink 测试平台 gate**：tao 要求 EventLoop 在主线程初始化，libtest 下各平台均无法运行，改为仅 Windows 编译并精确 skip；fixture 重做见 #104（#100）。
- **抽 `setup-linux-build` composite action**：CI 与 release 的 apt 依赖、`download-deps.sh`、tauri-cli 安装收敛到单点维护，CUDA 安装改为 `enable-cuda` 输入（#99）。
- **deploy-docs 改为 CI 成功后再部署**，避免红 CI 也发文档站（#97）。

## v2.5.2 (2026-07-21)

### Fixes

- **首次运行浮窗黑影和重叠**：overlay 窗口初始尺寸 (200×48) 与实际固定尺寸 (520×180) 不一致，首次 `set_state` 时 resize 导致透明窗口暴露新区域，合成器渲染出黑影且小窗口内容重叠。改 tauri.conf.json 初始尺寸对齐 `OVERLAY_SIZE`。

## v2.5.1 (2026-07-21)

### Packaging

- **移除 ffmpeg 依赖**：删除所有 ffmpeg 下载/打包/文档引用，`whisper-cli` 成为唯一的捆绑二进制。
- **CUDA runtime 不再随包分发**：改为用户自行安装 CUDA runtime 启用 GPU，未安装时自动回退 CPU，避免 deb 包膨胀到 1.5G。
- **`.so` 软链精简**：新增 `trim_so_to_soname()` 消除 Tauri 打包时 `.so` 三份复制膨胀（libggml-cuda 单份 131M → 避免 393M）。

### Docs

- **CONTEXT.md 术语扩充**：补充 Transcription Engine、MiMo ASR、Provider Preset、Model Catalog、Provider Category。

## v2.5.0 (2026-07-20)

### Features

- **MiMo ASR 语音识别后端**：新增 `engine="mimo"`，接入小米 MiMo-V2.5-ASR 云端 API，支持 wav/mp3、中英文自动检测；Settings 引擎切换新增“MiMo ASR (小米)”选项，选中自动填充 API 地址。
- **模型预设选择器**：转写和润色设置可按服务商预设快速填充——润色预设含 DeepSeek、Kimi、智谱、通义、OpenAI、Anthropic、SiliconFlow，语音识别预设含 MiMo ASR、OpenAI Whisper、本地 whisper.cpp；每个预设带推荐模型目录，支持搜索和展开详情。
- **whisper GPU 加速构建**：release 工作流安装 CUDA toolkit，本地 whisper 编译启用 GPU。

### Fixes

- **悬浮窗动画全面修复**：相位切换的闪烁、跳变、黑晕。窗口改固定尺寸（520×180），切换不再 resize，消除合成黑边与位移跳变；crossfade 只做淡出、不带位移；transitionend 只响应容器自身事件；先发结果文本再切 done，杜绝空 island 闪烁；hidden 延迟 220ms 再 hide，退出动画真正可见，并用代际计数防止竞态；去掉 box-shadow——半透明阴影叠在透明窗口上会被合成器预乘成黑影。

## v2.4.5 (2026-07-20)

### Refactor

- **架构审计修复（#84-#95）**：统一错误处理路径、类型重命名对齐领域语言、CSS 拆分为独立模块。
- **model 深模块化**（#66-#70）：Output 路径合并、五项架构优化，消除过度工程。
- **OverlaySink 解耦**：提取 trait 接口，TauriPipelineSink 与浮窗实现分离。
- **HistoryStore 注入**：消除对 Tauri 全局状态的依赖，构造时注入。
- **dead code 清理**：删除未使用的 notify 输出路径（#63）、三处死代码（#64）、废止的 PowerShell 脚本。

### Style

- **CSS token 统一**：transition 收敛、中文间距适配，样式系统规范化。

### Tests

- 补齐前端测试覆盖：StatusIndicator、useConfigForm、overlay 模块。
- 补齐 TauriPipelineSink 单元测试。

### Docs

- 更新 CLAUDE.md 反映 error.rs 和 voice_pipeline 新类型名。
- 同步写作规范至 CLAUDE.md 和 AGENTS.md。
- 精简 README，去除冗余结构。

## v2.4.4 (2026-06-19)

### Fixes

- **状态机回归修复**：按键事件路径重构后，`key_events` 通道关闭时管道循环不再永久挂起——现在会打印 `tracing::warn` 并干净退出。
- **PotentialPress 状态残留**：短按被 `min_press_duration` 拒绝后状态未回退到 Idle，后续系统重复 release 事件会让状态机从未真正按下就进入等待双击状态。现回退到 Idle。
- **转写失败状态卡死**：`transcriber.transcribe()` 返回错误时只调用 `on_error` 不恢复状态，前端状态指示器卡在 processing。现补充 `on_status_change("idle")`。
- **windows_vk 三态不一致**：`windows_vk` 与 `linux_evdev_code` 对齐，支持 JSON `null` 清除（之前 `{"windowsVk": null}` 被静默忽略）。
- **失效的 debounce_window 配置**：`debounce_task` 移除后 `debounce_window_ms` 成为死字段，现从配置和模板中彻底移除。

### Refactor — 深化模块（架构审查 5 项全部落地）

- **内联状态机**（#3）：按键事件路径从 4 层（thread → debounce_task → `Machine::run` → select）简化为 2 层（`key_events` → select 内联状态机），净减 139 行。
- **配置镜像结构体消除**（#1）：`config.rs` 直接存储 `Duration`（配合 serde `duration_ms`/`duration_secs` 辅助模块，TOML `_ms`/`_seconds` 字段名通过 alias 保持向后兼容）。移除 4 个模块的镜像 Config 结构体和 53 行机械复制的 `From` 实现。
- **TauriPipelineSink 解耦**（#2）：`HistoryStore` 改为构造时注入，`on_transcription_result` 不再运行时调用 `app.state()`。
- **voice_pipeline 拆分**（#5）：946 行单体文件拆为 5 个聚焦子模块（`sink`/`builder`/`context`/`handlers`/`mod`），公共接口通过 re-export 保持向后兼容。

### Tests

- 新增 `ConfigStore` 集成测试（7 个），覆盖 patch-validate-save 完整周期、windows_vk/linux_evdev_code 三态清除、无效配置拒绝。
- 测试总数从 146 提升到 156。

## v2.4.3 (2026-06-17)

### Features — Windows 平台支持

- **Windows 正式支持**：altgo 现可通过 MSI 安装运行于 Windows。录音用 cpal（WASAPI），剪贴板用 arboard，按键监听用 `WH_KEYBOARD_LL` 低级钩子，显示器几何用 `GetMonitorInfoW`。
- **MSI 打包与发布流水线**：`build.ps1` 等价于 Linux 的 `make build`；CI 在 tag 推送时自动构建 MSI 并发布到 GitHub Releases。
- **激活键捕获**：Windows 端 `key_capture` 通过 `WH_KEYBOARD_LL` 捕获激活键，与 Linux evdev 路径对齐。

### Refactor — 架构重构

- **Voice Pipeline 模块合并**（#46）：将 pipeline_orchestrator / context / command_handler / event_handler 合并为单一深模块 `voice_pipeline`，保留 `PipelineSink` 接缝。一条录音→转写流程现在集中在一个模块内。
- **HistoryStore 成为唯一接口**（#50）：游离的 `append_entry` 等函数收进 `HistoryStore`，新增 `count()` 领域方法。调用方不再直接处理文件路径。
- **trait 化注入点统一**：`KeyListener`、`Recorder`、`Output`、`Transcriber`、`Polisher` 均定义为 trait，pipeline 通过 `Box<dyn Trait>` 消费，测试可注入 fake。
- **ConfigPatch 移入 config.rs**：补丁应用逻辑与字段定义共处一处，新增配置字段只改一个文件。
- **Settings 拆分 hooks**：配置表单与模型管理逻辑分别抽到 `useConfigForm`、`useModelManager`。
- **错误边界类型化**：移除 `From<anyhow::Error>` 回退，各子系统边界返回类型化错误。

### Docs

- CLAUDE.md 与 `docs/agents` 文档翻译为中文，要求中文交流。
- 新增 Windows 支持的 ADR 与实现计划。

## v2.4.2 (2026-06-12)

### Fixes

- **设置保存修复**：Settings 页面保存配置时发送 patch payload，恢复配置保存 IPC 的参数结构。
- **录音态悬浮窗修复**：开始新录音时清空旧转写结果，并在显示窗口前同步 overlay 状态，避免上一次结果悬浮窗与录音中悬浮窗同时出现。

## v2.4.1 (2026-06-11)

### Fixes

- **悬浮窗黑影修复**：引入 `OverlayWindow` seam，把 Tauri 窗口操作封装到 adapter，显示前先完成尺寸、位置与原生窗口标志准备，减少 Linux 透明窗口首帧黑影。
- **Overlay 动画合成优化**：降低 overlay 阴影半径/透明度，改用不透明 solid surface，并把 opacity 动画从透明顶层窗口移动到内部 island，避免 alpha 叠加产生黑色晕影。
- **状态切换竞态修复**：取消未完成的 crossfade timer，避免 hidden 事件被旧 timer 覆盖后悬浮窗卡住显示。

## v2.4.0 (2026-06-11)

### Features

- **常驻 whisper-server 后端**：`engine="local"` 现在自动拉起常驻 whisper-server 进程，模型只载入一次内存，之后每句通过本地 HTTP 转写。告别每句重载 ~1.5–2.9 GB 模型的冷启动成本。
- **自动降级**：server 启动失败、端口冲突、运行期崩溃时自动回退到一次性 `whisper-cli`，旧安装也能工作。
- **调优旋钮**：新增 `transcriber.threads`（`0` = 自动取满 CPU 核数，whisper 默认仅 4）和 `transcriber.beam_size`（`<=1` = 贪心解码，最快）。

### Performance

- **~10× 转写提速**：medium 模型（1.5 GB）4 秒音频，每句从 ~2.8 s 降到 ~0.24 s（常驻后端 + GPU 加速叠加）。
- **CUDA 自动探测**：构建端检测到 `nvcc` 时自动启用 GPU 后端，捆绑 CUDA runtime 库。无 nvcc 时纯 CPU。
- **悬浮窗动画优化**：提取 `OverlayManager` 模块，单次 IPC 事件替代多次窗口操作；移除 `backdrop-filter` 实时模糊，改用纯色背景；进度条改用 `scaleX` 合成层动画，消除布局重排。

### Build

- **可移植 CPU 基线**：始终关闭 `-march=native`，避免旧机器 `SIGILL`。
- **whisper-server 捆绑**：`download-deps.sh` 现在同时捆绑 `whisper-server` 与 `whisper-cli`。

### Refactor

- **OverlayManager**：悬浮窗物理管理（尺寸/位置/显隐）从 `TauriPipelineSink` 中剥离为独立模块，接口从 3 个窗口方法缩减为 `set_state(OverlayState)` 单一意图调用。
- **CI 修复**：ffmpeg 下载源 `johnvansickle.com` 对 GitHub Actions 返回 415，新增 BtbN GitHub 镜像回退。

## v2.3.1 (2026-04-23)

### Packaging / CI

- Unified release pipeline: tag push now auto-builds **deb**, **rpm**, **AppImage**, **Flatpak**, and **AUR PKGBUILD**
- Added **RPM** bundle target for Fedora/RHEL/openSUSE
- Added **Flatpak** manifest (`.flatpak` artifact on GitHub Release)
- Added **AUR** PKGBUILD template and generator script
- Integrated AppImage build into `release.yml` (removed standalone `appimage.yml`)
- Unified version constants in `packaging/scripts/versions.sh` (fixed whisper.cpp v1.7.5 → v1.8.4 skew)

## v2.2.4 (2026-04-22)

### Packaging / CI

- Linux **deb** 在 **ubuntu-22.04** 上构建，链接 **glibc 2.35**，可在 **Ubuntu 22.04 (Jammy)** 等环境运行（避免在更新 runner 上出现 GLIBC_2.39+ 仅新系统可用的问题）
- Tauri 可执行文件统一为 **`altgo`**（与文档、桌面项、`make install` 一致；不再使用 `altgo-tauri` 作为安装名）

### Release

- GitHub Release 正文本版本起从 **CHANGELOG** 自动生成，并附与上一 tag 的对比链接

## v2.1.0 (2026-04-21)

### Features

- 转写历史：本地 `history.json` 持久化；History 页面列表、删除、清空、复制与单条再润色；管道成功后写入并广播 `history-updated`

### Improvements

- 更新应用图标与依赖；设置页与界面样式打磨

### Fixes

- 代码审查：并发、错误处理与资源管理等修复
- 构建与综合稳定性改进

## v2.0.1 (2026-04-21)

### Bug Fixes

- Windows：修复 `key_capture` 中 VK 字母显示名的类型错误（`i32` / `u8` / `char`），恢复 Release 构建

## v2.0.0 (2026-04-21)

### Documentation

- 全面更新 README：安装（Release、Makefile/deps、从源码轻量路径）、开发流程与 Makefile 目标说明、`key_listener` 可选字段与相关文档链接
- 更新 CLAUDE.md（`make build` 行为、`capture_activation_key`、`key_capture`、前端主题与样式结构）
- 更新 CONTRIBUTING.md（工具链版本、平台依赖、`frontend` 构建校验）
- 修复 docs-site 快速开始（移除不存在的安装脚本、修正构建命令与 MDX）；docs-site README 改用 npm
- README / docs-site：Linux 优先；标明 Ubuntu 20.04 测试环境；强调 **input 组** 必做；文档口径为仅本地 whisper.cpp、LLM 润色走 OpenAI 兼容 API、结果以**悬浮窗**为主；删除过时“平台支持”表；开发者以 **`make build`** 为主流程；最终用户用 deb/AppImage/MSI
- README / docs-site：默认配置方式改为**应用内设置**；手写 `altgo.toml` 标为高级；强调**预编译包捆绑** ffmpeg / whisper-cli，减少用户侧依赖清单

### CI / Release

- GitHub Actions：修复 CI 使用 `src-tauri/Cargo.toml`；CI/Release 在 Tauri 构建前执行 `download-deps.sh`；Release 增加 Windows MSI 与合并产物；Pages 在每次推送 `master` 时部署；AppImage 工作流注入 `VERSION`

## v1.0.0 (2026-04-14)

### Features

- Cross-platform desktop voice-to-text tool
- Hold right Alt key to record, release to transcribe
- Dual transcription backends: Whisper API (OpenAI-compatible) and local whisper.cpp
- LLM text polishing with 4 levels (none/light/medium/heavy)
- Automatic clipboard output
- Platform-native notifications
- GUI settings panel with real-time config reload

### Platform Support

- **Linux**: x86_64 and ARM64 builds with DEB packages
- **Windows**: x86_64 with MSI installer

### Bug Fixes

- Fix WiX v5 ComponentGroup configuration for MSI builds
- Fix release workflow runner compatibility
- Fix CJK font rendering in GUI panel
- Fix GUI config save guards
- Resolve multiple code quality and safety issues
