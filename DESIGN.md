---
version: alpha
name: altgo
description: 桌面语音转文字工具的界面设计语言。安静、紧凑、中性灰、扁平。
colors:
  background: "oklch(1 0 0)"
  surface-0: "oklch(1 0 0)"
  surface-1: "oklch(0.985 0 0)"
  surface-2: "oklch(0.967 0.001 286.375)"
  surface-3: "oklch(0.92 0.004 286.32)"
  bg-input: "oklch(1 0 0)"
  border-subtle: "oklch(0.967 0.001 286.375)"
  border: "oklch(0.92 0.004 286.32)"
  border-strong: "oklch(0.871 0.006 286.286)"
  text: "oklch(0.141 0.005 285.823)"
  text-secondary: "oklch(0.442 0.017 285.786)"
  text-muted: "oklch(0.552 0.016 285.938)"
  text-faint: "oklch(0.705 0.015 286.067)"
  primary: "oklch(0.21 0.006 285.885)"
  accent: "oklch(0.21 0.006 285.885)"
  accent-hover: "oklch(0.274 0.006 286.033)"
  accent-active: "oklch(0.141 0.005 285.823)"
  accent-soft: "oklch(0.37 0.013 285.805)"
  accent-contrast: "oklch(0.985 0 0)"
  accent-tint: "oklch(0.21 0.006 285.885 / 6%)"
  accent-tint-border: "oklch(0.21 0.006 285.885 / 26%)"
  fill-inverse: "oklch(0.21 0.006 285.885)"
  content-inverse: "oklch(0.985 0 0)"
  hover-surface: "oklch(0.141 0.005 285.823 / 5%)"
  green: "#0f9f6e"
  red: "#dc2626"
  amber: "#b45309"
  blue: "#2563eb"
typography:
  sans:
    fontFamily: Inter
  mono:
    fontFamily: JetBrains Mono
  text-2xs:
    fontFamily: Inter
    fontSize: 11px
    lineHeight: 1.5
  text-xs:
    fontFamily: Inter
    fontSize: 12px
    lineHeight: 1.5
  text-sm:
    fontFamily: Inter
    fontSize: 13px
    lineHeight: 1.5
  text-base:
    fontFamily: Inter
    fontSize: 14px
    lineHeight: 1.5
  text-md:
    fontFamily: Inter
    fontSize: 15px
    lineHeight: 1.5
  text-lg:
    fontFamily: Inter
    fontSize: 17px
    lineHeight: 1.3
  text-xl:
    fontFamily: Inter
    fontSize: 20px
    lineHeight: 1.3
  text-2xl:
    fontFamily: Inter
    fontSize: 24px
    lineHeight: 1.2
rounded:
  xs: 4px
  sm: 6px
  md: 8px
  lg: 10px
  xl: 14px
  card: 20px
  window: 16px
  full: 9999px
spacing:
  "1": 4px
  "1-5": 6px
  "2": 8px
  "2-5": 10px
  "3": 12px
  "3-5": 14px
  "4": 16px
  "5": 20px
  "6": 24px
  "8": 32px
  "10": 40px
---

## Overview

altgo 是桌面语音转文字工具：按住触发键说话，松开完成转写，结果写入剪贴板并在悬浮窗确认。界面由三个窗口组成：主窗（主页与设置）、屏幕底部的悬浮窗、更新说明窗口。

设计方向是安静、紧凑、中性灰。界面用一套 zinc 灰阶搭全部结构，颜色只在表达成功、危险、进行中时出现。层次靠面阶差与发丝线表达，不靠投影与渐变。信息密度向开发者工具看齐：小字号、紧凑行高、数字对齐，不做营销页的留白与装饰。

主窗是窄窗（400 至 480 宽），只有一列内容。页面切换放在窗口底部的浮动底栏里，标题栏只留品牌与窗口按钮。

## Colors

页底用 `background`，内容卡与对话框用 `surface-1` 加一圈发丝线，统计面与次级底色用 `surface-2`，需要再抬一级时用 `surface-3`。日间与夜间是同一套令牌的两组取值，令牌名不变。

主色是近黑（夜间是近白），只用在三处：主操作的实心底、选中态、焦点环。界面里没有第二种彩色强调。`accent-tint` 只给选中态做底，不给卡片描边、不给列表行染色。

主操作用反色实心按钮（`fill-inverse` 底加 `content-inverse` 字），深浅主题互换，不依赖彩色。

语义色只落在状态文字、徽标与小面积底色：`green` 表示完成与已连接，`red` 表示错误与危险操作，`amber` 表示进行中与待恢复，`blue` 表示信息与外部链接。语义色不铺大面积，也不做按钮底色。

边框一律是发丝线：结构默认 `border-subtle`，控件与分隔用 `border`，需要更强区分时才用 `border-strong`。同一处不叠两层边线。

## Themes

夜间主题把上述语义令牌换成下表的值，令牌名不变。

| 令牌 | 夜间值 |
|---|---|
| background | oklch(0.141 0.005 285.823) |
| surface-0 | oklch(0.141 0.005 285.823) |
| surface-1 | oklch(0.21 0.006 285.885) |
| surface-2 | oklch(0.274 0.006 286.033) |
| surface-3 | oklch(0.37 0.013 285.805) |
| bg-input | oklch(0.21 0.006 285.885) |
| border-subtle | oklch(1 0 0 / 6%) |
| border | oklch(1 0 0 / 10%) |
| border-strong | oklch(1 0 0 / 20%) |
| text | oklch(0.985 0 0) |
| text-secondary | oklch(0.871 0.006 286.286) |
| text-muted | oklch(0.705 0.015 286.067) |
| text-faint | oklch(0.552 0.016 285.938) |
| primary | oklch(0.92 0.004 286.32) |
| accent | oklch(0.92 0.004 286.32) |
| accent-hover | oklch(0.985 0 0) |
| accent-active | oklch(0.871 0.006 286.286) |
| accent-tint | oklch(0.985 0 0 / 8%) |
| accent-tint-border | oklch(0.985 0 0 / 34%) |
| fill-inverse | oklch(0.985 0 0) |
| content-inverse | oklch(0.141 0.005 285.823) |
| hover-surface | oklch(1 0 0 / 6%) |
| green | #34d399 |
| red | #f87171 |
| amber | #fbbf24 |
| blue | #60a5fa |

## Typography

界面文案用 Inter，中文回退到系统 UI 字体；命令、代码、按键标记、URL 用 JetBrains Mono。

正文用 `text-sm`，强调文字与表单取值用 `text-base`。标题用 `text-lg` 至 `text-xl`，字重 600，收紧字距（`-0.018em`，统计数值用 `-0.03em`）。中文界面不用大写变形与小标题字距。

数据类文字（时间戳、容量、进度、键码）用等宽数字（`.tabular`）。标题做两端平衡换行，段落做避免孤字换行，长文本截断用单行省略或行数夹取。

统计面上的标签用 `text-2xs` 加 500 字重、`text-muted` 色；数值用 `text-xl` 加 600 字重。

## Layout

主窗是 400 至 480 宽的窄窗。标题栏高 42px，只放品牌与最小化、最大化、关闭三个按钮，整条可拖拽。内容左右留白 16px，底部留出底栏高度加 12px，内容滚到底时不被底栏压住。

页面切换在浮动底栏里：胶囊形，距窗口下沿 14px 居中，高 44px，内含两项（主页、设置），当前页用 `surface-2` 实底加 `text` 色文字。底栏是唯一的全局导航，不往上加其他控件；有新版本时在设置项的图标角上挂一个 6px 红点。

设置页按分组组织：分组卡片圆角 14px，组内字段行最小高 34px，标签在左、控件在右，控件宽度上限 190px，右对齐。输入类控件占满所在列，不做整行通铺。

空态给一个图标、一句动作指引和一个明确操作，不给多条并列建议。

悬浮窗几何固定 280×64，不随内容变化，单行结果超出时省略。

## Elevation & Depth

平面优先。层次用面阶差与发丝线表达，不用光晕，不用彩色投影。

投影只有两档：`sm` 给浮在内容上的底栏，`md` 给对话框与浮层。静置的卡片、统计面、列表行都不加投影。悬浮窗不使用投影（部分 Linux 合成器会把半透明阴影预乘成黑影）。

焦点态用 1px 主色环，输入类控件聚焦时同时把边框切到主色。

## Shapes

控件圆角 8px，面板与对话框 14px，统计面 20px，主窗 16px。圆角克制，只有状态点、徽标、开关与头像类元素使用全圆角。

键位标记用 6px 圆角，带 1px 边框与浅底，等宽字体。

分段控件与浮动底栏是胶囊形：容器全圆角，内部按钮同样全圆角。

## Components

按钮四档变体：主操作反色实心（`fill-inverse` 加 `content-inverse`，每个视图至多一处），次要操作 `surface-2` 底无边框，描边按钮透明底加发丝线，幽灵按钮透明底、悬停浮 `hover-surface`。危险操作默认只染红文字，悬停才浮现红底与红边。

按钮三档高度：32px 默认、26px 紧凑、36px 加大，字号 12 至 14px，字重 500。

下拉与文本输入共用同一盒式基座：高度 32px，内边距 10px，8px 圆角，token 化底色与边框，聚焦切主色边框并加 1px 色环。任何单写 `.select` 的标记都必须拿到完整皮肤，不允许出现系统原生外观。

开关用胶囊轨道加圆形滑块，选中态用主色轨道。勾选框与单选框自绘外观，选中态用主色填充。

徽标是胶囊形：默认 `surface-2` 底，成功、危险、警告、信息四种语义底色按需使用。服务商字母标记用中性底色，不引入第二种彩色。

卡片分两种：内容卡是 `surface-1` 加发丝线、14px 圆角；统计面是 `surface-2` 无边框、20px 圆角，用于数量、状态与关键数值。

## Do's and Don'ts

- 不用光晕与彩色投影。
- 不用渐变，吸底栏与遮罩用实色。
- 不用彩色强调，主色只有近黑与近白两种取值。
- 不给交互反馈加超过 200 ms 的过渡。
- 不动画布局属性，进度条等小面积指示器除外。
- 不依赖系统原生控件外观。
- 不在浮动底栏里加导航以外的控件。
- 尊重 prefers-reduced-motion。
