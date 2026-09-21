# AGENTS.md

## 交流语言

始终使用中文与用户交流。代码、commit message、PR 描述等技术输出也用中文。

## 写作要求

所有面向人读的文本（注释、CONTEXT.md、ADR、issue 评论、PR 描述、agent brief、triage notes、Sphinx 文档、Agent 回复），遵守以下原则：

- **善于总结材料**：材料弄全弄准，去粗取精、去伪存真、由此及彼、由表及里，反映事物本质；不堆砌细节、不拼凑清单。
- **真懂才能写好**：反复改都写不清楚，往往是因为对所写的内容还不大懂；真懂了，才有高屋建瓴、势如破竹之势。
- **逻辑清晰**：整篇文章前后次序有逻辑，交代清楚。
- **用词准确**：相邻概念划清界限，不混用、不模糊。概念要抓住事物的本质、全体和内部联系，而非现象、片面和外部联系。
- **观点鲜明**：不堆砌凑数、聚沙成堆。不用夸大的修饰词（”权威””强大””完整””单一事实来源”之类），它们减损力量。
- **废话应当尽量除去**。
- **读得下去是基本要求**：文字通顺，让人读得下去、读后脑中有印象；读完脑中无印象，是极差的文章。
- **通俗、亲切，由小讲到大，由近讲到远，引人入胜**：先讲读者已知／当前的事物，再推到陌生／抽象的；忌一上来就宏大叙事或先搬死人、外国人。
- **与读者完全平等**：靠分析说服，不要装腔作势来吓人；老老实实办事。
- **动笔前想受众**：这篇东西给谁看？谁受益？怎样让更多人受益？

## 编码准则

LLM 写代码时会犯一些可以预见的错误，同样几个，一遍又一遍。以下是规则，需要严格遵守。

### 1. 写代码前先读懂

LLM 产出烂代码最大的根源，就是写新代码之前没有读懂现有代码库。你看到一个任务，匹配到训练数据里的某个模式，就开始生成。这通常导致代码不贴合项目实际。

写任何东西之前：

- 把你要改的文件读一遍。不是略读，是读。
- 看看项目里别处是怎么做类似事情的。有范式就照着来；有工具函数已经做了一半你需要的事，就用它。
- 看文件顶部的 import，它们告诉你这个项目实际在用什么库。项目到处用 fetch，就别引入 axios；项目用原生方法，就别引入 lodash。
- 看测试文件，它们告诉你预期行为到底是什么。

如果你不是 100% 确定某个方法以这个确切签名存在，查文档或看项目里的真实源码。自信地用一个不存在的 API 或已移除的参数，是典型的知识幻觉。

如果你不确定这个项目里某件事是怎么做的，就说出来。“我在代码库里没看到 X 的范式，是该照 Y 的做法来，还是另起炉灶？”永远比瞎猜强。

### 2. 动手前先想清楚

没想清楚到底要做什么之前，别开始写代码。

**把假设说出来。** 用户说“加个鉴权”，可能指 session cookie、JWT、OAuth、basic auth，或其他五种东西。别默默选一个。说“我假设你要的是基于 JWT 的鉴权，带 refresh token，存在 httpOnly cookie 里。如果你想要别的，告诉我。”

**点明取舍。** 几乎每个实现选择都有代价。加缓存就拿内存换速度，还引入了缓存失效这件此后得操心的事。写之前说清楚，用户可能说“其实我不要这个复杂度”。

**做了架构决策，要标出来。** 这些选择难以撤销，用户应当知道。

**存在多种做法时，简要地列出来。** 两种，顶多三种，带上推荐。“A 更简单，但处理不了边界情况 X。B 全 cover，但引入对 Z 的依赖。除非你预期 X 真会发生，否则我选 A。”

**有搞不懂的地方，停下。** 别用听起来像那么回事的代码去填糊涂。直接说哪里搞不懂，问。

### 3. 避免过度工程

写解决问题所需的最少代码，不是理论上能解决问题的最少代码，而是此刻真正解决这个具体问题的最少代码。

过度工程的冲动很强。抵制它。典型表现：

**过早抽象。** 用户要的只是 `sendWelcomeEmail(user)`，你却写了一个带策略模式、支持多家供应商的 EmailService。以后真需要更多，他们会开口。

```python
# 差
class EmailService:
    def __init__(self, provider: EmailProvider, template_engine: TemplateEngine):
        self.provider = provider
        self.template_engine = template_engine

    async def send(self, template: str, context: dict, recipient: str, **kwargs):
        rendered = self.template_engine.render(template, context)
        await self.provider.send(recipient, rendered, **kwargs)

# 好
async def send_welcome_email(user):
    body = f"Welcome {user.name}! Your account is ready."
    await send_email(to=user.email, subject="Welcome", body=body)
```

重复远比错误的抽象便宜。先 copy-paste 两次，再谈抽象。

**投机式的错误处理。** 为不可能发生的错误包 try/catch，对永远不为 null 的值加 null 检查，每一行都是别人得读懂的一行。只处理真正会发生的错误。

**没必要的可配置性。** 你把 batch size 做成参数，把重试次数做成可配置，为永远不会变的东西加环境变量。每个配置项都是某人要做的一个决定、要设对的一个值。在有真正的理由之前，硬编码。

**死灵活性。** 只有一个实现的接口、只有一个子类的抽象基类，有成本（认知开销、间接层），在第二个实现真正出现之前零收益。

检验：不熟项目的人问“这干嘛要这么抽象？”，而答案是“万一我们需要……”，那就是过度工程了。“万一我们需要”不是需求，是对未来的猜测，而对未来的猜测通常是错的。

### 4. 精准改动

改现有代码时，diff 越小越好。你改的每一行都可能引入 bug、都得有人 review、还会永远留在 git blame 里。

**别动没让你动的东西。** 修函数 A 的 bug，注意到函数 B 的变量名很怪，别管。函数 C 的注释有个错别字，别管。import 顺序不合你意，别管。你的活是修函数 A 的 bug。

**贴合现有风格。** 文件用单引号你就用单引号，用 `snake_case` 你就用 `snake_case`，没分号就别加分号。文件内的一致性胜过你的个人偏好。

**收拾自己留下的，不收拾别人的。** 你的改动让某个 import 没用了，就删掉。但仅限你的改动导致的，既存的死代码不归你管。

**别重新格式化。** 别对原本没用 prettier 的文件跑 prettier，别把 4 空格缩进改成 2 空格，别把原本不按字母序的 import 重排。重新格式化制造海量 diff，淹没你真正的改动。

检验：diff 里每一行改动都能直接对应到被要求的事上。有“既然都进来了，顺手……”的，撤掉。

### 5. 验证

“能跑的代码”和“你以为能跑的代码”之间，差的就是测试。

**修 bug 时先写测试。** 先写一个能复现 bug 的测试，看它挂，然后修 bug，看它过。这是唯一能证明你确实修好了、而不是让症状消失的办法。

**按改动范围分层验证。** 先跑受影响模块的测试和必要静态检查；改前能跑的同范围检查改后也应通过。跨模块、共享契约、基础设施、依赖升级，或影响范围无法可靠判断时，再扩大到相关集成测试、全量测试或 CI 指定的回归套件。改前就失败的，说出来，别让你的改动替既存失败背锅。

**测行为，不测实现。** 检查构造函数有没有设好属性的测试一文不值；检查校验是否真的拦住坏输入的测试才有价值。

**想想 happy path 之外的情况。** API 返回 500 时怎样？文件不存在时？用户提交空表单时？

**写不了测试，就说明原因。** “数据库调用跟业务逻辑紧耦合，没法轻松测”，这是个可能需要重构的信号。别默默跳过测试然后指望没事。

### 6. 目标驱动

每个任务在动手前都该有清晰的成功标准。标准模糊，就把它变具体；变不出具体的，就问。

把模糊任务转成可验证的：

- “加校验” → “拦掉邮箱缺失或非法的输入，返回 400 并说明哪里错了，为这两种情况都加测试”
- “修 bug” → “写一个复现上报行为的测试，让它通过，确认现有测试仍通过”
- “提升性能” → “先 profile，定位瓶颈，修那一个具体问题，再测一次”

超过一步的活，执行前先说出计划：

```
计划：
1. 用 migration 加新的数据库列
2. 更新 model 包含新字段
3. 改 API endpoint 以接受并返回该字段
4. 为该字段加校验
5. 为新行为写测试
6. 跑受影响模块的测试和静态检查；若改动跨模块或影响范围不清，再扩大回归范围
```

这让用户能在你浪费时间之前逮到思路失误，也逼你自己把步骤想过一遍。

### 7. 调试

出了问题不工作时，别猜。调查。

**把错误信息读完。** 整条，包括 stack trace。看到错误就立刻基于类型生成“修复”，根本不读它说了什么，这是常见的坏毛病。一个 TypeError 可能指一百种情况，信息和 stack trace 告诉你是哪一种。

**先复现。** 复现不了就没法验证修复。“我觉得这应该能修好”不是调试，是赌博。

**一次只改一处。** 改了三处然后 bug 没了，你不知道是哪一处修好的，也不知道另外两处有没有引入新 bug。改一处，测。再改一处，测。

**没搞懂根因之前，别加 workaround。** 一个值意外为 null，搞清楚它为什么是 null。null 检查也许能防崩溃，但底下的 bug 还在，以后会换个样子冒出来。

**卡住了就说。** “我试了 X 和 Y 都没用，我看到的是这些，觉得问题可能在 Z 但没把握。”这比默默瞎试 20 轮有用得多。

### 8. 依赖

加依赖之前先想想。你加的每一个依赖都是一段你不掌控的代码，却要永久成为项目的一部分，得维护、更新、审计安全问题。代价几乎总比看上去高。

加包之前：

- 项目已有的东西能不能做？有 axios 就别加 node-fetch，有 date-fns 就别加 moment。
- 标准库能不能做？`Array.prototype.map` 不需要 lodash，`crypto.randomUUID()` 存在就不需要 uuid。
- 看最近提交日期和 issue 情况，判断它是否还在维护。
- 它多大？为了格式化日期加个 500KB 的包，多半不值。

真要加时说明原因。默默往 package.json 塞包，不行。

### 9. 沟通

你怎么就代码沟通，跟代码本身一样重要。

**说你做了什么、为什么。** “我把校验逻辑抽到单独的函数里，因为它在三个 endpoint 里重复了。这也让它能独立测试。”用户不用逐行读就懂了这次改动。

**标出顾虑。** “这个能跑，但对列表里每一项都打一次数据库，列表一大就会慢。要不要我改成批量？”这种主动沟通能在以后省下几个小时。

**精确说出你不确定的是什么。** “我不确定这个库支不支持流式响应”，有用。“我觉得这应该能行”，没用。差别在于前者让用户清楚该去验证什么。

**别解释用户已经知道的事。** 把解释的层次对齐到用户展现出来的知识水平。

**commit message 要具体。** “Fix bug”毫无用处。“修好用户查询里的空指针，当邮箱含大写字符时”才能让下一个人清楚发生了什么。

### 10. 常见失败模式

这些是我最常看到的模式。如果你逮住自己在干其中任何一件，停下来重新想想。

**厨房水槽。** 让你加一个功能，你“顺手”重构半个代码库。别。做那一件事。

**错误的抽象。** 你为一个只在一处存在的问题，造了一个漂亮的通用方案。先 copy-paste 两次，再谈抽象。

**隐形决策。** 你做了架构选择，却没有把它作为一项决策标出来。用户应当知道你做了它。

**乐观路径。** 你写的代码把 happy path 处理得完美，对其他一切要么忽略要么崩溃。想想 API 返回 500 时会怎样。文件不存在时。用户提交空表单时。

**知识幻觉。** 你自信地用一个并不存在的 API、一个两个版本前就被移除的参数、或一个想象出来的库特性。如果你不是 100% 确定某个方法以这个确切签名存在，就说出来。查文档。看项目里的真实源码。

**风格漂移。** 你用自己“偏好”的风格写代码，而不是贴合项目。在 OOP 代码库里写函数式。在函数式代码库里写类。在 JavaScript 项目里写 TypeScript 范式。贴合代码库，不是贴合你的偏好。

**失控重构。** 你开始修一处。它碰到另一处。那处又碰到另一处。二十分钟后你改了 15 个文件，不确定自己最初要干什么。如果修复开始级联，停下。告诉用户发生了什么。继续之前先取得同意。

这些准则起作用的标志是：diff 里不必要改动更少、因过度复杂而返工更少、澄清问题发生在实现之前而不是犯错之后。

## 项目概览

**altgo** 是用 Rust 编写的桌面语音转文字工具，支持 Ubuntu 22.04+ 的 **x86_64** 与 **aarch64** 架构，以及 Windows 10+（x86_64 与 arm64）。目前不支持 macOS。按住右 Alt 键录音，松开后使用 **本地 SenseVoice**（内嵌 sherpa-onnx）进行转写。随后可选通过 **OpenAI 兼容或 Anthropic Messages 协议**的 LLM 进行润色。结果写入系统剪贴板，并在悬浮浮窗中显示。成功的转写结果（原始文本 + 显示文本）以纯文本历史记录的形式持久化保存在本地 JSON 文件中；音频不会被保存。历史文件位置：Linux 为 `~/.config/altgo/history.json`。

## 构建与测试命令

```bash
# 仅 Rust（无 GUI）
cargo build --release --manifest-path=src-tauri/Cargo.toml
cargo test --manifest-path=src-tauri/Cargo.toml
cargo fmt --manifest-path=src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path=src-tauri/Cargo.toml -- -D warnings

# Tauri GUI 模式
cargo tauri dev               # 开发模式（前端开发服务器 + 桌面窗口）
cargo tauri build            # 生产环境 GUI 构建

make build
make install                  # 构建后：altgo -> /usr/local/bin，config -> /etc/altgo/
```

## 架构

基于 Tauri 的桌面应用，核心逻辑位于 `src-tauri/src/`：

| 组件 | 路径 |
|-----------|------|
| **Tauri GUI** | `src-tauri/` + `frontend/` |
| **核心模块** | `src-tauri/src/` |
| **文档站点** | `docs-site/`（Docusaurus） |

由键盘事件驱动的核心流水线：

```
Key Listener → State Machine → Recorder → Transcriber → Polisher → Output (+ History JSON)
```

### 模块（位于 `src-tauri/src/`）

- **`lib.rs`** —— Tauri 应用入口：`run()` 装配 managed state（`ConfigStore`、`HistoryStore`、`PipelineController`、`Arc<dyn Output>`），`spawn_pipeline_thread` 在独立 OS 线程的 tokio 运行时上拉起 `voice_pipeline::run`（从 managed state 取 Output 与 HistoryStore，构造 `TranscriptionDispatcherImpl` 和 `TauriPipelineSink`）。
- **`cmd.rs`** —— 通过 IPC 暴露给前端的 Tauri 命令，共 17 个：配置（`get_config`、`save_config`、`capture_activation_key`、`test_polisher_connection`）、更新（`check_update`、`install_update`）、pipeline（`start_pipeline`）、浮窗（`copy_text`、`hide_overlay`）、模型（`list_models`、`download_model`、`delete_model`、`resolve_model`）、历史记录（`list_history`、`delete_history_entries`、`clear_history`、`polish_history_entry`）。历史追加由 `tauri_sink.rs` 经 `TranscriptionDispatch` 驱动，写入成功后发出 `history-updated` 事件，并优先在润色后文本经 trim 后非空时展示润色文本。
- **`history.rs`** —— `HistoryStore`：对 `history.json` 的追加/列出/删除/清空/更新/计数（camelCase JSON，文件 I/O 使用 `Mutex`）。`HistoryStore` 是唯一对外接口，调用方不直接接触文件路径或内部辅助函数。不保存音频。
- **`config.rs`** —— 使用 `serde(default)` 加载每个字段的 TOML 配置；`ConfigPatch` 补丁逻辑与字段定义共处一处。文本润色 API 密钥可通过环境变量覆盖（`ALTGO_POLISHER_API_KEY`）。
- **`config_store.rs`** —— `Config` 的持久化封装；所有变更经 `apply_patch` 校验并写盘。校验失败时内存已部分应用、不落盘（非原子回滚）。
- **`state_machine.rs`** —— 5 状态枚举（`Idle`、`PotentialPress`、`Recording`、`WaitSecondClick`、`ContinuousRecording`）。长按录音，双击进入连续模式。提供同步接口（`process`、`poll_timeout`、`next_deadline`），由 `voice_pipeline` 的 `tokio::select!` 主循环驱动。
- **`audio.rs`** —— 线程安全的 PCM 缓冲区（`Mutex<Vec<u8>>`），WAV 编码/解码（44 字节头 + PCM）。
- **`error.rs`** —— 类型化错误枚举：顶层 `PipelineError` 分致命（`Fatal(FatalError)`，停管道）与可恢复（`Recoverable(RecoverableError)`，降级继续）；各模块返回自己的 thiserror 枚举（`TranscriberError`、`PolisherError`、`RecorderError`、`OutputError`、`KeyListenerError`、`ModelError`、`ConfigError`、`HistoryError`）。
- **`transcriber.rs`** —— 转写后端 trait；生产实现由 `sherpa.rs` 的本地 SenseVoice 提供。
- **`sherpa.rs`** —— 本地 SenseVoice 后端（`SherpaTranscriber`）：内嵌 sherpa-onnx，模型在管道启动时加载一次并常驻内存；推理是 CPU 密集同步操作，经 `spawn_blocking` 放入阻塞线程池。
- **`polisher.rs`** —— 使用 LLM 对文本进行 4 档润色（`none`/`light`/`medium`/`heavy`），支持 OpenAI 兼容聊天 API 与 Anthropic Messages API 两种协议。指数退避重试（3 次）。`polisher/protocol.rs` 定义 API 协议类型（`ApiProtocol`）。
- **`prompt_store.rs`** —— 润色 prompt 模板管理：从 `resources/prompts/` 组合 `base.txt` + 各档后缀，启动时加载一次，改文件需重启应用生效。
- **`voice_pipeline/`** —— 核心处理流水线（录音→转写→润色）的单一深模块。`sink.rs` 定义 `PipelineSink` 接缝（状态变更、错误、结果、进度、按键后端通知）与 `TranscriptionResult` / `DispatchOutcome`；`dispatcher.rs` 是 sink 注入的业务 seam（剪贴板写入 + 历史追加，归到 `TranscriptionDispatch` trait），生产实现 `TranscriptionDispatcherImpl` 转调 `process_transcription_result`；`handlers.rs` 留有 `dispatch_history_polish` 编排 `store.get + formatter.polish + store.polish_entry`。`context.rs` 主循环是 `tokio::select!` 三分支（按键、状态机超时、停止信号），转写与润色在循环内串行完成（单次转写互斥）。
- **`pipeline_controller.rs`** —— 流水线生命周期与状态跟踪（`PipelineStatus`：Idle/Recording/Processing/Done/Stopped 五态），由 `start_pipeline` 与 `save_config` 内的 `restart_pipeline` 驱动。
- **`tauri_sink.rs`** —— `PipelineSink` 的 Tauri 适配器：把管道事件转成前端事件，并把悬浮窗操作委托给 `OverlayManager`。剪贴板/历史业务由 `TranscriptionDispatch` trait 注入（构造时一次性决定），本模块不再持有 `Output` 或 `HistoryStore`。
- **`model.rs`** —— SenseVoice 模型管理（下载、切换；模型目录存储在 `~/.config/altgo/models/`，每个模型一个子目录，含 `model.int8.onnx` 与 `tokens.txt`）。
- **`tray.rs`** —— 系统托盘配置（显示窗口、退出菜单）。
- **`updater.rs`** —— 应用自动更新（ADR-0004）：静默/手动双检查模式（手动检查带 10 秒超时与分类错误）；按打包方式分级——NSIS 与 AppImage 就地更新，deb/rpm/AUR 外部引导到下载页或包管理器命令。与 `PipelineController` 联动，录音/处理中不重启。
- **`display_backend.rs`** —— 显示后端探测：Wayland 会话且用户未显式设置 `GDK_BACKEND` 时，在 GUI 初始化前切到 X11（XWayland），否则悬浮窗定位不生效。
- **`resource.rs`** —— 路径与线程数工具：`effective_threads`、`expand_tilde`、`which_binary`（PATH 命令查找）。
- **`key_capture/`** —— 设置中的一次性激活键捕获（Linux：通过 `evtest` 监听 `/dev/input/event*`；Windows：键盘钩子）。
- **`key_listener/`** —— 按键检测（Linux：X11 优先 `xinput test-xi2`、失败回退 `evtest`；Wayland 会话优先 `evtest`。Windows：WH_KEYBOARD_LL 钩子）。
- **`recorder/`** —— 音频捕获（Linux：`parecord` PulseAudio 子进程；Windows：cpal/WASAPI），输出 16kHz 单声道 WAV。
- **`output/`** —— 结果输出（Linux：剪贴板经 `xclip`/`xsel`/`wl-copy` 写入；Windows：arboard 剪贴板 + SendInput 文本注入）。结果展示统一由浮窗负责，无系统通知。
- **悬浮窗（`overlay/`）** —— 状态意图与窗口操作分离：`overlay/seam.rs` 定义 `OverlayWindow` seam，`overlay/manager.rs` 按状态意图算尺寸/位置，`overlay/tauri.rs` 是 Tauri 适配器（主显示器几何：Linux 用 `xrandr` 解析，Windows 用 Tauri `primary_monitor()`）。全阶段共用一个固定窗口尺寸，相位切换只换前端内容（见 CONTEXT.md「悬浮窗」条目的平台陷阱说明）。

### 前端结构（`frontend/src/`）

```
├── App.tsx                 # 应用入口
├── main.tsx                # React 渲染入口
├── ThemeContext.tsx        # 主题 Provider
├── theme.ts                # 主题 token / 持久化
├── overlay.tsx             # 悬浮窗口组件
├── overlay.css             # 浮窗样式（由 overlay.tsx 在 TS 侧 import overlay-base、motion）
├── components/
│   ├── Layout.tsx          # 布局组件
│   └── StatusIndicator.tsx # 状态指示器
├── pages/
│   ├── Home.tsx            # 首页
│   ├── History.tsx         # 转写历史（选择 / 删除 / 清空 / 复制 / 润色单行）
│   └── Settings.tsx        # 设置页
├── hooks/
│   ├── useTauri.ts         # Tauri 集成 hook
│   ├── useConfigForm.ts    # 配置表单 hook
│   └── useModelManager.ts  # 模型管理 hook
├── i18n/                   # 国际化
└── styles/
    ├── global.css
    ├── design-system.css
    ├── design-tokens.css   # 设计 token
    ├── motion.css          # 动效 / 过渡
    ├── layout.css          # 布局组件样式
    ├── overlay-base.css    # 共享浮窗布局
    ├── components/
    │   ├── ui-primitives.css
    │   └── status-indicator.css
    └── pages/
        ├── home.css
        ├── history.css
        └── settings.css
```

### 关键模式

**基于子进程的系统交互（Linux）** —— Linux 上的平台集成通过调用 CLI 工具（`xinput`/`evtest`、`parecord`、`xclip`、`xrandr`）完成。这简化了构建，避免了原生依赖的复杂性。Windows 走原生 API（键盘钩子、cpal/WASAPI、arboard），见各模块 `windows.rs`。

**平台模块 + trait 抽象** —— `key_listener/`、`recorder/`、`output/`、`key_capture/` 各含 `linux.rs` 与 `windows.rs` 实现。`Platform*` 类型别名提供默认实现，每个模块暴露一个 trait（`KeyListener`、`Recorder`、`Output`），以便流水线可以使用 `Box<dyn Trait>`，提升可测试性。

**异步流水线** —— 按键监听器经无界 `tokio::sync::mpsc` 通道把按键事件送入主循环；状态机同步返回命令，由 `tokio::select!` 分支就地执行，没有独立的命令通道（单次转写互斥，见 ADR-0003）。整条管道运行在独立 OS 线程的 tokio 运行时上（`spawn_pipeline_thread`）。

**配置** —— 位于 `~/.config/altgo/altgo.toml`。模板在 `configs/altgo.toml`。所有字段都有 serde 默认值，因此部分配置也能工作。

**转写历史** —— `~/.config/altgo/history.json`（与配置同目录）。条目：`id`、`createdAtMs`、`rawText`、`text`。浮窗和前端监听 **`history-updated`** 事件以刷新列表。

### 系统要求

**Linux**：`xinput`、`evtest`、`xmodmap`、`parecord`、`xclip`/`xsel`/`wl-copy`、`xrandr`

### Tauri GUI 开发

首次运行前，安装前端依赖：
```bash
cd frontend && npm install
```

## 测试说明

- 单元测试位于每个源文件的 `#[cfg(test)]` 模块内。
- `config.rs`、`audio.rs`、`model.rs` 和 `history.rs` 有全面的测试。
- `polisher.rs` 使用 `mockito` 进行 HTTP 级别的模拟；本地 SenseVoice 的模型存在性与加载失败由 `sherpa.rs` 单元测试覆盖。
- 平台特定模块只有少量测试（仅构造/冒烟测试）。
- CI 共三个 job：Linux `amd64` 与 `arm64` 都跑 `cargo test --lib`；Windows job 在 x64 上跑 `cargo test --lib`，arm64 仅 `cargo check`。release 发版前由独立 `test` job 再跑一遍测试。
- 完整测试套件画像与维护提示见 `docs/testing.md`。

## Agent 技能

### Issue tracker

Issue 位于 GitHub Issues（`cislunarspace/altgo`）。使用 `gh` CLI。参见 `docs/agents/issue-tracker.md`。

### Triage labels

五个标准 triage 标签，使用默认名称。参见 `docs/agents/triage-labels.md`。

### Domain docs

单上下文布局（仓库根目录的 `CONTEXT.md` + `docs/adr/`）。参见 `docs/agents/domain.md`。

### Loop Engineering

本仓库配有 Loop 三件套：`/loop-go <任务>` 循环运行 builder 与 checker，直到全部检查通过。定义见 `.claude/agents/builder.md`、`.claude/agents/checker.md`、`.claude/commands/loop-go.md`。三件套为项目级副本，覆盖用户级同名 agent（仅本仓库生效）。

#### Loop 停止规则

- 最多 5 轮。每轮开始时公开声明 "Cycle N/5"。
- 同一失败连续出现两次 → 停止循环，向用户报告。
- 修复导致之前通过的检查失败 → 停止循环（拆东墙补西墙）。
- 达到轮次上限仍未全部通过 → 停止，报告当前状态。

