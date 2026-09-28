# ADR-0004: 应用自动更新架构与分发策略

- **状态**: 已接受（随 v2.6.4 落地）
- **日期**: 2026-08-24
- **受影响模块**: `src-tauri/`, `frontend/`, `.github/workflows/release.yml`, `tauri.conf.json`

## 背景与问题

altgo 是一款常驻后台的桌面语音转文字应用，支持 Linux（x86_64、aarch64）以及 Windows（x86_64）。当前版本分发通过 GitHub Release 上传预编译包，用户需要手动关注并下载更新。为提升版本迭代普及率与用户体验，需支持自动/手动检查更新、自动下载及根据打包类型的安全安装。

## 决策

1. **更新引擎与分发协议**:
   - 采用官方 `tauri-plugin-updater` 插件。
   - 更新源指向 GitHub Release 发布的 `latest.json`。
   - 使用 Ed25519 签名体系进行更新包签名验证（公钥固化在 `tauri.conf.json`，私钥配置于 GitHub Actions Secret）。

2. **多打包格式的分级更新策略（Update Support Tier）**:
   - **就地更新（InPlace）**：适用于 Windows（NSIS）和 Linux（AppImage）。更新器自动下载并调用 Tauri 重启完成就地替换。
   - **外部引导（External）**：适用于 Linux 传统包管理格式（deb、rpm、AUR）。由于需要系统 root 权限或由独立包管理器接管，检测到更新后，更新器展示新版本详情、更新日志，并提供直达 GitHub Release 下载页或包管理器更新命令的引导，不在桌面进程内直接调用特权命令。

3. **双检查模式（Check Mode）与 10 秒超时**:
   - **静默模式（Silent）**：软件启动时在后台静默检查（受配置项 `[gui] auto_check_update` 控制，默认开启）。失败时不打扰用户；发现新版本时在主界面/托盘显示轻量徽标。
   - **手动模式（Manual）**：用户在“关于/设置”页面点击“检查更新”。设置硬性 10 秒超时；失败时返回具体分类原因（`Timeout`、`NetworkError`、`SignatureError`、`RateLimited` 等）。

4. **生命周期与录音安全（Lifecycle Safety）**:
   - 更新器与 `PipelineController` 状态联动。当语音流水线处于 `Recording` 或 `Processing` 状态时，延迟或阻止重启动作，避免在用户转写过程中强行杀死进程导致语音数据丢失。

## 后果与权衡

### 正向收益
- Windows 和 AppImage 用户获得平滑的一键无缝自动更新体验。
- 静默检查不干扰录音与日常使用，手动检查具备精准的错误诊断。
- 引入 Ed25519 签名验证，杜绝更新劫持与篡改风险。

### 代价与局限
- deb/rpm 用户无法做到完全静默后台无感更新（需用户配合提权安装）。
- CI/CD 流水线需在构建产物后生成 `latest.json` 和签名文件。
