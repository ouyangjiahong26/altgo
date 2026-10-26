# 仓库内文档说明

本目录存放架构、测试、签名方案与决策记录，面向维护者与贡献者，不是最终用户手册。

| 路径 | 内容 |
|------|------|
| [`architecture.md`](architecture.md) | 系统架构速览，与 `testing.md` 配套 |
| [`testing.md`](testing.md) | 测试分层、替身选择与 CI 覆盖 |
| [`code-signing.md`](code-signing.md) | Windows 安装包的签名方案、SignPath 申请条件与发布流程改造 |
| [`adr/`](adr/) | 架构决策记录 |

## 按读者选择入口

| 读者 | 入口 |
|------|------|
| 最终用户 | 根目录 [`README.md`](../README.md)，[文档站](https://ouyangjiahong26.github.io/altgo/)的快速开始、配置、使用与 FAQ |
| 贡献者 | [`CONTRIBUTING.md`](../CONTRIBUTING.md)，本目录的架构与测试文档 |
| 维护者 | [`code-signing.md`](code-signing.md)，[`adr/`](adr/) 与根目录 [`CHANGELOG.md`](../CHANGELOG.md) |

文档站源文件在 [`docs-site/docs/`](../docs-site/docs/)，由 Docusaurus 构建。
