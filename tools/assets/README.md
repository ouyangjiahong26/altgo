# 品牌资产生成工具

本目录的工具负责生成 altgo 的图标、横幅、社交分享图与界面截图。全部产物都可复现：
改提示词或矢量源文件后重跑对应命令即可，不需要手工修图。

## 目录内容

| 文件 | 用途 |
|---|---|
| `altgo-icon.svg` | **应用图标母版**（带靛蓝底方块）。位图全部由它光栅化而来 |
| `altgo-mark.svg` | **桌面快捷方式标记母版**（透明底裸标记，不画底色） |
| `altgo-mark-mono.svg` | 上面的单色变体（`currentColor`），供界面内按上下文着色 |
| `banner.svg` | README 横幅母版（1536×512） |
| `og-image.svg` | 社交分享图母版（1536×1024） |
| `overlay-phase-recording.svg` | README 功能清单用图：悬浮窗录音相位（红点与电平轨迹） |
| `overlay-phase-transcribing.svg` | 同上：转写中相位（旋转环与进度线） |
| `overlay-phase-result.svg` | 同上：完成相位（对勾与单行结果） |
| `prompts.brand.json` | 品牌视觉的 GPT 出图提示词（用于出方案、对照选型） |
| `gen.mjs` | 批量出图：调图像接口，落盘到 `out/<模型>/<id>.png` |
| `build-icon.mjs` | 由两份图标母版生成 `src-tauri/icons/` 全套位图与 `.ico`，并输出透明标记 png |
| `build-assets.mjs` | 由矢量母版渲染 `assets/banner.png`、`assets/og-image.png` |
| `check-overlay-phases.mjs` | 校验三张相位图的结构，并检出改样式后漏改插图的旧值残留 |
| `serve.mjs` | 预览/核对用的静态服务器（会重写 CSS `@import` 为绝对路径） |
| `out/` | 出图中间产物，不进版本库 |

## 常用命令

```bash
# 1. 由矢量母版重生成图标全套（应用图标 png + icon.ico + 透明标记 + 前端图标）
node tools/assets/build-icon.mjs

# 2. 由矢量母版重生成横幅与社交分享图
node tools/assets/build-assets.mjs

# 3. 改过 overlay.css 或 design-tokens.css 后，校验悬浮窗插图是否跟着更新
node tools/assets/check-overlay-phases.mjs

# 4. 批量出图：先在 tools/assets/.env 写 ALTGO_IMAGE_API_KEY
node tools/assets/gen.mjs --prompts tools/assets/prompts.brand.json --out tools/assets/out --concurrency 10

# 5. 要在浏览器里核对界面样式（临时预览服务，不是产品服务）
node tools/assets/serve.mjs --root frontend --port 4180
#   然后打开 http://127.0.0.1:4180/preview.html
```

## 出图接口约定

`gen.mjs` 走 OpenAI 兼容的 `POST /v1/images/generations`，兼容两种返回：`data[0].b64_json`
与 `data[0].url`。各模型的尺寸参数名不统一，OpenAI 系用 `size`，`nano_banana` 系要
`resolution`，后者通过提示词文件里的 `extra` 字段透传：

```json
{ "id": "brand-banner", "size": "1536x1024", "extra": { "resolution": "2K" }, "prompt": "..." }
```

密钥只放在 `tools/assets/.env`（已在 `.gitignore` 里），不要写进脚本或提示词文件。

## 为什么图标是手绘矢量而不是直接用出图结果

图像模型对“文字旁的图标语义”不稳：同一个提示词可能出来读作字母 `i` 的符号而不是麦克风，
小尺寸下还会糊边。流程是**用出图定方向、用矢量定稿**——`altgo-icon.svg` 按 32×32 网格
取半像素对齐，配 `#4F46E5` 精确色值，任何尺寸下边缘都干净，且改一处即全平台一致。

两份母版各有用途，不要混用：

- `altgo-icon.svg` 带底色方块，用于应用图标、安装包、窗口标题栏——这些位置四周是系统
  绘制的底板，没有自己的底色会显得单薄。
- `altgo-mark.svg` 透明底，用于桌面快捷方式与启动器——桌面壁纸颜色不可控，带一个方块
  会在深色壁纸上出现突兀的色块。该标记在 16px 下仍读得出麦克风形状。

## 界面样式的核对方式

界面观感不要靠截图判断：无头浏览器离屏截图会把表单控件的绘制弄失真（同一份 CSS，
浏览器里正常、截图里会是未样式化的白框），据此评审会得出错误结论。

正确做法是起预览服务在浏览器里直接看：

```bash
node tools/assets/serve.mjs --root frontend --port 4180
```

- `http://127.0.0.1:4180/preview.html` — 唯一入口。一页看全各窗口的实际渲染，带深/浅配色切换；`assets/` 下的品牌资产走 `/repo/assets/...`
- `http://127.0.0.1:4180/style-review.html` — 需要逐项对照 token 与组件时再去这里

这些页面直接加载 `frontend/src/styles/` 下的真实样式表，改完刷新即可，无需重启或重新构建。
