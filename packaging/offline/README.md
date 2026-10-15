# 离线资产包

面向完全没有外网的 Windows x64 机器：把应用、转写模型、润色模型与润色运行时一次搬过去。决策与取舍见 `docs/adr/0012-offline-asset-pack.md`。

## 包内结构

```
altgo-<版本>-offline-win-x64/
├── altgo_<版本>_x64-setup.exe     来自该版本的 Release（checksums.txt 里那一行的哈希）
├── install-offline-assets.ps1     校验并放置模型与运行时
├── SHA256SUMS                     除自身外每个文件的 SHA-256
├── THIRD-PARTY-NOTICES.md         模型与运行时的来源与许可证
├── models/sense-voice/            转写模型，来自 HuggingFace
├── polish-models/qwen3-0.6b/      本地润色模型，来自 HuggingFace
├── runtime/llama-server/b11612/   llama.cpp 的 Windows x64 产物
└── webview2/                      仅 with-webview2 变体含此目录，供 Windows 10 使用
```

模型与运行时的来源地址、SHA-256 与构建号都固定在代码里，取自 `src-tauri/src/model.rs`、`src-tauri/src/polish_model.rs` 与 `src-tauri/src/polish_runtime/manifest.rs`，不要从别处取数。

## 出包步骤

1. 从版本 Release 下载 `altgo_<版本>_x64-setup.exe`，核对它在该版本 `checksums.txt` 中的哈希。
2. 复制 `models`、`polish-models`、`runtime` 三个目录，内容取自应用配置目录（`%APPDATA%\altgo`），逐文件核对代码内固定的 SHA-256。
3. 复制本目录的 `install-offline-assets.ps1`、`THIRD-PARTY-NOTICES.md`，为 with-webview2 变体另取 WebView2 独立离线安装器放到 `webview2/`（`https://go.microsoft.com/fwlink/?linkid=2124701`，记录实际解析到的地址、大小与哈希）。
4. 生成 `SHA256SUMS`（不包含它自己），再压成 zip，顶层目录名与 zip 同名（`altgo-<版本>-offline-win-x64` 或带 `-with-webview2` 后缀的变体），两个变体解压到同一处也不会互相混淆。
5. 解压回验：重新计算每个文件的哈希，并在 Windows 上跑一次 `install-offline-assets.ps1` 的校验分支。
6. 建 tag `offline-assets-v<版本>`（指向该版本的发布提交），创建 Release，把目标环境、内容清单、两个 zip 的 SHA-256 与 WebView2 安装器的版本信息写进正文，上传 zip。

## 注意

- 脚本必须带 UTF-8 BOM。Windows PowerShell 5.1 对无 BOM 的脚本按系统 ANSI 代码页解析，中文提示会变成乱码。
- WebView2 安装器只允许随应用一起分发，永不单独挂成发布产物，也不得改名或重打包。包内清单要带该许可快照与 §9 的 SmartScreen 通知，详见 `docs/adr/0012-offline-asset-pack.md` 的后果一节。
- 安装器与 WebView2 离线安装器都不进版本 Release 的产物清单，版本 Release 的产物校验与 `checksums.txt` 范围（deb、rpm、AppImage、`*-setup.exe`）保持不变。
