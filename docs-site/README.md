# altgo 文档站（Docusaurus）

面向最终用户的说明文档站点，主体为简体中文（`zh-Hans`）。SignPath 申请所需的隐私政策与快速开始页面提供英文版本，英文在前、中文在后。生产环境部署在 GitHub Pages：

[https://ouyangjiahong26.github.io/altgo/](https://ouyangjiahong26.github.io/altgo/)

`url` / `baseUrl` 与组织名见 [`docusaurus.config.ts`](docusaurus.config.ts)。推送 `master` 时由 [`.github/workflows/deploy-docs.yml`](../.github/workflows/deploy-docs.yml) 构建并发布（详见 [`CONTRIBUTING.md`](../CONTRIBUTING.md)）。

## 本地开发

```bash
cd docs-site
npm install
npm start
```

浏览器默认打开开发服务器，文档变更会热更新。

## 构建

```bash
npm run build
```

产物在 `build/` 目录，可用任意静态文件服务托管。

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

- 文档页面：`docs/*.mdx`（侧边栏由 [`sidebars.ts`](sidebars.ts) 配置）
- 营销首页：`src/pages/index.tsx`（与文档首页 `docs/intro.mdx` 不同：前者为站点落地页，后者为文档模块入口）
- 仓库根目录另有维护者用的 [`docs/`](../docs/)（设计/计划归档），请勿与本文档目录混淆

站点只维护简体中文，不提供英文版本：`docusaurus.config.ts` 的 `locales` 仅含 `zh-Hans`，导航栏也不再有语言切换（原 `i18n/en/` 英文站已移除）。
