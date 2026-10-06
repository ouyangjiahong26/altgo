# altgo

![altgo](assets/banner.png)

[![CI](https://github.com/ouyangjiahong26/altgo/actions/workflows/ci.yml/badge.svg)](https://github.com/ouyangjiahong26/altgo/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-online-2f6feb)](https://ouyangjiahong26.github.io/altgo/)
[![Release](https://img.shields.io/github/v/release/ouyangjiahong26/altgo)](https://github.com/ouyangjiahong26/altgo/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

altgo 是桌面语音转文字工具。按住触发键说话，松开即自动完成录音、转写与可选润色，结果写入系统剪贴板，并在屏幕底部的悬浮窗中单行展示。

支持 Linux（Ubuntu 22.04+，x86_64 / aarch64）与 Windows 10+（x86_64 / arm64），暂不支持 macOS。

- [在线文档](https://ouyangjiahong26.github.io/altgo/)
- [Releases 下载](https://github.com/ouyangjiahong26/altgo/releases)
- [问题反馈](https://github.com/ouyangjiahong26/altgo/issues)

## 功能

- 按住右 Alt 说话，松开自动转写

  ![录音时的悬浮窗：红点与实时电平轨迹](assets/overlay-phase-recording.svg)

- **转写引擎二选一**：本地 SenseVoice（内嵌 sherpa-onnx，离线可用，模型常驻加载，可在设置页下载与管理）或在线 MiMo ASR（无需下载模型，需填 API Key）
- 转写中悬浮窗显示旋转环与进度线

  ![转写中的悬浮窗：旋转环与进度线](assets/overlay-phase-transcribing.svg)

- **LLM 润色**（可选）：四档强度，支持 OpenAI 兼容协议与 Anthropic Messages API，内置多家提供商预设
- **结果写入剪贴板**，并在屏幕底部悬浮窗以单行悬浮条扫一眼确认

  ![完成后的悬浮窗：对勾与单行结果，悬停显示关闭按钮](assets/overlay-phase-result.svg)

- 本地转写历史：查看、复制、复制原文、删除、清空、重新润色
- 自动输入到光标位置（仅 Windows，默认关闭）
- 自动检查更新：启动时静默检查，设置页可手动检查并在独立窗口查看更新说明
- 托盘图标：显示主窗口或退出应用
- 外观可调：深浅配色跟随系统或手动指定，字体大小与窗口尺寸各三档
- 只保存文本，从不保存音频

## 安装

### Linux

安装前把当前用户加入 `input` 组，否则无法读取键盘设备。随后注销重新登录：

```bash
sudo usermod -aG input "$USER"
```

然后：

1. 从 [Releases](https://github.com/ouyangjiahong26/altgo/releases) 下载对应架构的 `.deb`、`.rpm` 或 `.AppImage`。
2. 安装下载的包，例如：

   ```bash
   sudo apt install ./altgo_*.deb
   # 或
   sudo dnf install ./altgo-*.rpm
   ```

   `.AppImage` 免安装，加执行权限直接运行：

   ```bash
   chmod +x altgo_*.AppImage && ./altgo_*.AppImage
   ```

   与 `.deb`/`.rpm` 不同，AppImage 不自动解析依赖。缺库时参照下方依赖说明自行安装。

3. 重新登录后启动 altgo，在设置页完成转写配置。

<details>
<summary>依赖说明</summary>

`.deb` 与 `.rpm` 已声明覆盖桌面集成、音频、剪贴板与 `evtest` 的依赖。Wayland 会话需确保已安装 `evtest`，且当前用户可读 `/dev/input/event*`。

</details>

### Windows

从 [Releases](https://github.com/ouyangjiahong26/altgo/releases) 下载安装包：

- `*-setup.exe`（NSIS 安装器）：双击安装，适合多数用户。

x64 与 arm64 按设备架构选择对应包。安装后从开始菜单启动 altgo，在设置页完成转写配置。

## 快速开始

启动应用后，在设置页完成：

1. 选择转写引擎：本地 SenseVoice 需先下载模型，在线 MiMo ASR 需填 API Key。
2. 按需设置润色档位与润色服务。
3. 确认触发键（默认右 Alt）。
4. 点击保存。

一次完整的转写：

```text
按下右 Alt，开始录音；松开右 Alt，依次转写、润色（可选）、写入剪贴板并弹出悬浮窗
```

转写历史默认保存在：

```text
~/.config/altgo/history.json
```

## 文档

- [在线文档](https://ouyangjiahong26.github.io/altgo/)：快速上手、使用与架构
- [配置指南](https://ouyangjiahong26.github.io/altgo/docs/configuration)：配置文件字段、环境变量与日志级别
- [FAQ](https://ouyangjiahong26.github.io/altgo/docs/faq)：按键、录音、转写、润色与剪贴板问题排查
- [`CONTRIBUTING.md`](CONTRIBUTING.md)：开发环境、构建、测试、CI 与发版
- [`docs/architecture.md`](docs/architecture.md) 与 [`AGENTS.md`](AGENTS.md)：系统架构与核心模块
- [`docs/README.md`](docs/README.md)：设计与规划文档索引
- [`CHANGELOG.md`](CHANGELOG.md)：版本历史

## 许可证

[MIT License](LICENSE)
