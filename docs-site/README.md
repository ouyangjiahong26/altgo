# altgo 文档站（Docusaurus）

面向最终用户的说明文档站点，提供英文与简体中文两个语言版本。英文为默认语言，位于 `/`，简体中文位于 `/zh-Hans/`。生产环境部署在 GitHub Pages：

[https://ouyangjiahong26.github.io/altgo/](https://ouyangjiahong26.github.io/altgo/)

`url` / `baseUrl` 与组织名见 [`docusaurus.config.ts`](docusaurus.config.ts)。推送 `master` 时由 [`.github/workflows/deploy-docs.yml`](../.github/workflows/deploy-docs.yml) 构建并发布（详见 [`CONTRIBUTING.md`](../CONTRIBUTING.md)）。

## 本地开发

```bash
cd docs-site
npm install
npm start
```

浏览器默认打开开发服务器，文档变更会热更新。`npm start -- --locale zh-Hans` 直接查看中文站。

## 构建

```bash
npm run build
```

`npm run build` 依次构建所有 locale，默认语言产物在 `build/`，简体中文在 `build/zh-Hans/`，可用任意静态文件服务托管。

## 翻译结构

英文正文是唯一源文件，放在 `docs/` 与 `src/`。中文是译文，放在 `i18n/zh-Hans/`：

```text
i18n/zh-Hans/
├── code.json                                  # 首页与站点级文案
├── docusaurus-plugin-content-docs/
│   ├── current/                               # 中文文档页，文件名与 docs/ 一一对应
│   └── current.json                           # 侧边栏分类与条目标签
└── docusaurus-theme-classic/
    ├── navbar.json                            # 导航栏标签
    └── footer.json                            # 页脚标签
```

修改英文文案后运行以下命令补齐新增的翻译键，再在生成的文件里填中文：

```bash
npx docusaurus write-translations --locale zh-Hans
```

缺少中文文件时对应页面回退到英文，构建不会失败。新增文档页需要同时在 `docs/` 与 `i18n/zh-Hans/docusaurus-plugin-content-docs/current/` 放同名文件，并在 [`sidebars.ts`](sidebars.ts) 中登记。

## 依赖覆盖

`package.json` 的 `overrides` 把四个传递依赖抬到已修复安全公告的版本。Docusaurus 3.10 的依赖树把它们声明在旧的大版本区间里，锁文件刷新升不上去，只有强制覆盖才能装到修复版本：

| 包 | 覆盖到 | 声明旧区间的上游 | 对应公告 |
| --- | --- | --- | --- |
| `tinypool` | `^2.1.2` | `@docusaurus/core` 的 `^1.0.2` | GHSA-5gmw-xhrv-c9v3、GHSA-85c8-ppgw-ccpr |
| `serialize-javascript` | `^7.1.2` | `copy-webpack-plugin` 与 `css-minimizer-webpack-plugin` 的 `^6` | GHSA-5c6j-r48x-rmvq、GHSA-qj8w-gfj5-8c6v |
| `postcss-selector-parser` | `^7.1.6` | cssnano 6 的六个插件声明的 `^6` | GHSA-rj75-hqrm-r3gf |
| `uuid` | `^11.1.1` | `sockjs` 的 `^8.3.2` | GHSA-w5hq-g745-h8pq |

这几个包只参与构建与本地开发服务器，不进站点产物：`tinypool` 供静态生成在页数够多时起线程池，`serialize-javascript`、`postcss-selector-parser`、`uuid` 分别属于 webpack 构建路径、cssnano 与 `sockjs` 开发服务器。升级 Docusaurus 或上述上游后应重新评估这些条目，上游若已声明安全的区间就删掉对应覆盖。

另有一个告警无法通过升级消除：`braces` 停在 3.0.3，是 GHSA-vfj7-8cjw-p6xm 报出的最后一个可用版本，上游没有发布修复版。它只被 `chokidar` 与 `micromatch` 用来展开仓库自己的 glob 模式，等 Docusaurus 换掉这条依赖链后才会消失。

## 内容与侧边栏

- 英文文档页面：`docs/*.mdx`，中文页面：`i18n/zh-Hans/docusaurus-plugin-content-docs/current/*.mdx`（侧边栏由 [`sidebars.ts`](sidebars.ts) 配置，标签译文在 `i18n/zh-Hans/docusaurus-plugin-content-docs/current.json`）
- 营销首页：`src/pages/index.tsx`，文案通过 `translate()` 的 id 提取到 `i18n/zh-Hans/code.json`（与文档首页 `docs/intro.mdx` 不同：前者为站点落地页，后者为文档模块入口）
- 仓库根目录另有维护者用的 [`docs/`](../docs/)（设计/计划归档），请勿与本文档目录混淆
