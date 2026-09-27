# altgo 文档站（Docusaurus）

面向最终用户的说明文档站点，提供**简体中文**（`zh-Hans`，默认）与**英文**（`en`）两个语言版本。生产环境部署在 **GitHub Pages**：

**[https://ouyangjiahong26.github.io/altgo/](https://ouyangjiahong26.github.io/altgo/)**

`url` / `baseUrl` 与组织名见 [`docusaurus.config.ts`](docusaurus.config.ts)。推送 `master` 时由 [`.github/workflows/deploy-docs.yml`](../.github/workflows/deploy-docs.yml) 构建并发布（详见 [`CONTRIBUTING.md`](../CONTRIBUTING.md)）。

# altgo Docs Site (Docusaurus)

A documentation site for end users, available in **Simplified Chinese** (`zh-Hans`, the default) and **English** (`en`). In production it is hosted on **GitHub Pages**:

**[https://ouyangjiahong26.github.io/altgo/](https://ouyangjiahong26.github.io/altgo/)**

For `url` / `baseUrl` and the organization name, see [`docusaurus.config.ts`](docusaurus.config.ts). On pushes to `master`, [`.github/workflows/deploy-docs.yml`](../.github/workflows/deploy-docs.yml) builds and publishes the site (see [`CONTRIBUTING.md`](../CONTRIBUTING.md) for details).

## 本地开发

```bash
cd docs-site
npm install
npm start
```

浏览器默认打开开发服务器，文档变更会热更新。

## Local Development

```bash
cd docs-site
npm install
npm start
```

The browser opens the development server automatically, and documentation changes hot-reload.

## 构建

```bash
npm run build
```

产物在 `build/` 目录，可用任意静态文件服务托管。

## Build

```bash
npm run build
```

The output lands in the `build/` directory and can be served by any static file server.

## 内容与侧边栏

- 文档页面：`docs/*.mdx`（侧边栏由 [`sidebars.ts`](sidebars.ts) 配置）
- 英文站：`i18n/en/docusaurus-plugin-content-docs/current/*.mdx`（对应页面的纯英文版；修改文档后需同步更新）
- 页面 UI 文案：运行 `npx docusaurus write-translations --locale en` 生成骨架后填写
- 营销首页：`src/pages/index.tsx`（与文档首页 `docs/intro.mdx` 不同：前者为站点落地页，后者为文档模块入口）

仓库根目录另有维护者用的 [`docs/`](../docs/)（设计/计划归档），请勿与本文档目录混淆。

## Content and Sidebar

- Documentation pages: `docs/*.mdx` (the sidebar is configured by [`sidebars.ts`](sidebars.ts))
- English site: `i18n/en/docusaurus-plugin-content-docs/current/*.mdx` (plain-English copies of each page; keep them in sync when editing docs)
- Page UI strings: run `npx docusaurus write-translations --locale en` to generate the skeleton, then fill in translations
- Marketing homepage: `src/pages/index.tsx` (different from the docs homepage `docs/intro.mdx`: the former is the site landing page, the latter is the entry point of the docs module)

The repository root also contains [`docs/`](../docs/) for maintainers (design/planning archives); do not confuse it with this documentation directory.
