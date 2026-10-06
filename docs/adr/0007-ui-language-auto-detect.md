# 界面语言跟随系统 locale：自动检测常驻与错误文案分层

AppImage 目录（appimage.github.io）收录时以截图 OCR 判定界面语言，要求非中文环境默认英文。altgo 界面此前已有 zh/en 双字典、设置页手动切换与跨窗口同步，缺口只在默认语言硬编码中文。决定：`gui.language` 空串语义定为“自动”，且 auto 常驻，即每次启动跟随系统语言（`navigator.language` 前缀 `zh` 取中文，否则回退英文），不检测落盘。用户显式选择后固定。相比检测一次落盘，常驻 auto 让应用跟随系统语言变化，也是目录建议的原样语义。

配套两个决策。其一，悬浮窗错误文案目前是 Rust `message()` 的中文字符串直达前端，第二批按错误码化处理：IPC 传错误枚举标识（kind + 参数），前端字典翻译，Rust 不维护第二套用户文案。文案统一收敛到前端字典。其二，仓库文档与 README 保持中文单语不动：目录的语言判定以截图 OCR 为准，README 中文只影响机器人邀请文案的选择，不构成收录条件。

## 后果

- 识别语言（`transcriber.language`）与界面语言（`gui.language`）同名易混，术语表分立词条（CONTEXT.md “语言”一节），设置页与文档不得混用。
- 错误码化落地时，事件协议与 `voice_pipeline` 测试替身需同步修改。`message()` 仅剩 `describe_test_error`（润色连接测试）与测试消费，事件通道不再传中文文案。
- 自动检测依赖 `navigator.language`，三窗口各自解析（同系统结果一致）。不读 `LANG`/`LC_*`（Windows 无此环境变量，Rust 侧探测徒增平台分叉）。
