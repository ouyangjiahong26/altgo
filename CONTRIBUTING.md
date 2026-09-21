# 贡献指南

感谢你对 altgo 的关注！

本项目支持 **Linux** 的 **x86_64** 与 **aarch64** 架构。CI 和 Release 在 **Ubuntu 22.04** 上完成 Linux 构建验证。合并前请尽量在相关架构上自测。

## 开发环境

- Rust **1.80+**（推荐最新稳定版，需满足 [Tauri 2 前置条件](https://tauri.app/start/prerequisites/)）
- **Node.js 18+**（建议 20+；前端使用 npm）
- Tauri CLI：`cargo install tauri-cli --version "^2" --locked`
- Linux（Ubuntu 22.04+）

### 平台特定依赖

- **Linux**：`xinput`、`xmodmap`、`parecord`、`xclip` 或 `wl-copy`；Wayland 下按键监听还需 `evtest`，且需能读取 `/dev/input/event*`（常见：`sudo usermod -aG input $USER` 后重新登录）。完整 GUI 构建需 GTK/WebKit 等开发库，见 [Tauri 2 前置条件](https://tauri.app/start/prerequisites/)。

## 开发流程

1. Fork 仓库
2. 创建功能分支 (`git checkout -b feat/my-feature`)
3. 编写代码和测试
4. 确保通过检查：
   ```bash
   cargo fmt --manifest-path=src-tauri/Cargo.toml --all -- --check
   cargo clippy --manifest-path=src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path=src-tauri/Cargo.toml
   cd frontend && npm test
   cd frontend && npm run build
   ```

   其中 Rust 的 fmt / clippy / test 也有 Makefile 便捷目标：`make fmt`、`make lint`、`make test`。
5. 提交变更 (`git commit`)
6. 推送分支 (`git push origin feat/my-feature`)
7. 创建 Pull Request

## 提交消息格式

```
type: 简短描述

可选正文说明
```

类型：`feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`

## 代码风格

- 运行 `cargo fmt` 格式化代码
- `cargo clippy -- -D warnings` 零警告
- 公开 API 添加文档注释
- 函数 < 50 行，文件 < 1000 行

## 测试

- 新功能尽量附带单元测试或 HTTP 级模拟测试（与 `transcriber`/`polisher` 类似）
- 使用 `#[cfg(test)]` 模块组织单元测试
- 集成测试放在 `tests/` 目录

## 平台相关开发

- 尽可能使用子进程调用系统工具（Linux），避免 FFI
- 新增系统工具调用时，确保有合理的错误处理和用户提示
- 平台特定代码通过 Linux 模块与 trait 隔离；每个平台模块实现对应 trait（`KeyListener`、`Recorder`、`Output`），使管道可测试

## CI、Release 与 GitHub Pages

- **CI**（`.github/workflows/ci.yml`）：向 `master` 推送或开 PR 时在 `amd64` 与 `arm64` 两个 Linux job 上运行 Rust 测试并构建 **deb**；前端测试、`fmt` 和 `clippy` 只在 `amd64` job 执行一次，因为这些检查不依赖目标架构。
- **Release**（`.github/workflows/release.yml`）：推送符合 `v*` 的 **tag**（例如 `v1.5.0`）时，先校验 tag、Cargo、Tauri 配置、前端版本和 CHANGELOG，再构建 **Linux deb / rpm**（`amd64` 与 `arm64`）及双架构 AUR PKGBUILD，生成 `checksums.txt` 并创建 GitHub Release。发版前请将 `src-tauri/Cargo.toml`、`tauri.conf.json` 和 `frontend/package.json` 中版本与 tag 对齐。
- **文档站**（`.github/workflows/deploy-docs.yml`）：`master` 上的 **CI 成功后**才构建 Docusaurus 并部署到 **GitHub Pages**（`workflow_run` 触发），避免 CI 失败时仍把文档发出去。首次需在仓库 **Settings → Pages** 中将 **Build and deployment** 的 Source 设为 **GitHub Actions**（勿选 branch 静态目录）。文档地址见 `docs-site/docusaurus.config.ts` 中的 `url` / `baseUrl`（例如 `https://<org>.github.io/altgo/`）。也可在 Actions 中手动 **Run workflow** 触发部署。

## 问题反馈

- 使用 GitHub Issues 报告 bug 或提出功能请求
- 包含：平台、版本、复现步骤、日志输出

# Contributing Guide

Thanks for your interest in altgo!

The project supports **Linux** on **x86_64** and **aarch64**. CI and Release verify Linux builds on **Ubuntu 22.04**. Please self-test on the relevant architectures before merging where possible.

## Development Environment

- Rust **1.80+** (latest stable recommended; must satisfy the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/))
- **Node.js 18+** (20+ recommended; the frontend uses npm)
- Tauri CLI: `cargo install tauri-cli --version "^2" --locked`
- Linux (Ubuntu 22.04+)

### Platform-Specific Dependencies

- **Linux**: `xinput`, `xmodmap`, `parecord`, `xclip` or `wl-copy`; key listening on Wayland additionally needs `evtest` and read access to `/dev/input/event*` (typically: `sudo usermod -aG input $USER`, then re-login). Full GUI builds need GTK/WebKit dev libraries — see the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/).

## Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Write code and tests
4. Make sure checks pass:
   ```bash
   cargo fmt --manifest-path=src-tauri/Cargo.toml --all -- --check
   cargo clippy --manifest-path=src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path=src-tauri/Cargo.toml
   cd frontend && npm test
   cd frontend && npm run build
   ```

   The Rust fmt/clippy/test commands also have Makefile shortcuts: `make fmt`, `make lint`, `make test`.
5. Commit (`git commit`)
6. Push the branch (`git push origin feat/my-feature`)
7. Open a Pull Request

## Commit Message Format

```
type: short description

optional body
```

Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`

## Code Style

- Run `cargo fmt` to format code
- `cargo clippy -- -D warnings` with zero warnings
- Document public APIs
- Functions under 50 lines; files under 1000 lines

## Testing

- Pair new features with unit or HTTP-level mock tests where possible (similar to `transcriber`/`polisher`)
- Organize unit tests in `#[cfg(test)]` modules
- Integration tests go in `tests/`

## Platform-Specific Development

- Prefer subprocess calls to system tools on Linux over FFI
- New system-tool invocations need sensible error handling and user-facing messages
- Platform-specific code stays isolated behind per-platform modules and traits; each platform module implements its trait (`KeyListener`, `Recorder`, `Output`) so the pipeline stays testable

## CI, Release, and GitHub Pages

- **CI** (`.github/workflows/ci.yml`): on pushes to `master` or PRs, Rust tests run and a **deb** build runs on both `amd64` and `arm64` Linux jobs; frontend tests plus `fmt`/`clippy` run once on the `amd64` job only, since they don't depend on the target architecture.
- **Release** (`.github/workflows/release.yml`): pushing a **tag** matching `v*` (e.g. `v1.5.0`) first validates tag, Cargo, Tauri config, frontend version, and CHANGELOG, then builds **Linux deb / rpm** (`amd64` and `arm64`) plus dual-architecture AUR PKGBUILDs, generates `checksums.txt`, and creates the GitHub Release. Before releasing, align versions in `src-tauri/Cargo.toml`, `tauri.conf.json`, and `frontend/package.json` with the tag.
- **Docs site** (`.github/workflows/deploy-docs.yml`): Docusaurus builds and deploys to **GitHub Pages** only after CI succeeds on `master` (triggered via `workflow_run`), so docs never go out on a red CI. First time around, set **Settings → Pages → Build and deployment → Source** to **GitHub Actions** (not a branch/static folder). See `url` / `baseUrl` in `docs-site/docusaurus.config.ts` for the site address (e.g. `https://<org>.github.io/altgo/`). You can also deploy manually via **Run workflow** in Actions.

## Reporting Issues

- Use GitHub Issues for bugs and feature requests
- Include: platform, version, reproduction steps, log output
