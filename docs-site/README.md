# altgo 文档站（Docusaurus）

面向最终用户的说明文档站点，只有简体中文（`zh-Hans`）一个语言版本。生产环境部署在 GitHub Pages：

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

## 内容与侧边栏

- 文档页面：`docs/*.mdx`（侧边栏由 [`sidebars.ts`](sidebars.ts) 配置）
- 营销首页：`src/pages/index.tsx`（与文档首页 `docs/intro.mdx` 不同：前者为站点落地页，后者为文档模块入口）
- 仓库根目录另有维护者用的 [`docs/`](../docs/)（设计/计划归档），请勿与本文档目录混淆

站点只维护简体中文，不提供英文版本：`docusaurus.config.ts` 的 `locales` 仅含 `zh-Hans`，导航栏也不再有语言切换（原 `i18n/en/` 英文站已移除）。
