---
version: alpha
name: altgo
description: 桌面语音转文字工具的界面设计语言。安静、编辑感、代码优先。
colors:
  background: "#101010"
  surface-0: "#151515"
  surface-1: "#191919"
  surface-2: "#202020"
  surface-3: "#292929"
  bg-input: "#131313"
  border-subtle: "#232323"
  border: "#2a2a2a"
  border-strong: "#3d3d3d"
  text: "#ececec"
  text-secondary: "#a3a3a3"
  text-muted: "#737373"
  text-faint: "#4d4d4d"
  primary: "#7b82f4"
  accent: "#7b82f4"
  accent-hover: "#8d93f6"
  accent-active: "#6b72e8"
  accent-soft: "#9a9ff6"
  accent-contrast: "#ffffff"
  accent-tint: "rgba(123, 130, 244, 0.12)"
  accent-tint-border: "rgba(123, 130, 244, 0.38)"
  fill-inverse: "#ececec"
  content-inverse: "#101010"
  hover-surface: "rgba(255, 255, 255, 0.055)"
  green: "#67b487"
  red: "#e06c75"
  amber: "#d2a24c"
typography:
  sans:
    fontFamily: Inter
  mono:
    fontFamily: JetBrains Mono
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
rounded:
  xs: 3px
  sm: 4px
  md: 6px
  lg: 10px
  "2xl": 14px
  full: 9999px
spacing:
  "1": 4px
  "1-5": 6px
  "2": 8px
  "2-5": 10px
  "3": 12px
  "4": 16px
  "5": 20px
  "6": 24px
  "8": 32px
  "10": 40px
---

## Overview

altgo 是桌面语音转文字工具：按住触发键说话，松开完成转写，结果写入剪贴板并在悬浮窗确认。界面由三个窗口组成：主窗（主页与设置）、屏幕底部的悬浮窗、更新说明窗口。

设计方向是安静、编辑感、代码优先。界面像一件精确的开发工具，不像营销页面。界面不抢内容的戏，层次靠面阶差与发丝线表达，颜色只用来表达含义。

## Colors

页底用 `background`，卡片与面板用 `surface-1`，抬升面与悬停底用 `surface-2`，深浅主题都是四级灰阶。

强调色 `accent` 在同一视图里表达当前选中态与链接文字，不做装饰：不给卡片描边、不给列表行染底、不给普通图标上色。选中态用 `accent-tint` 底或 `accent` 描边二选一。

主操作用反色实心按钮（`fill-inverse` 底加 `content-inverse` 字），深浅主题互换，不依赖强调色。

语义色只落在状态文字与小面积底色：`green` 表示完成与已复制，`red` 表示错误与危险操作，`amber` 表示进行中与待恢复。语义色不铺大面积。

边框一律是发丝线：结构默认 `border-subtle`，控件与分隔用 `border`，需要更强区分时才用 `border-strong`。同一处不叠两层边线。

## Themes

日间主题把上述语义令牌换成下表的值，令牌名不变。

| 令牌 | 日间值 |
|---|---|
| background | #ffffff |
| surface-0 | #ffffff |
| surface-1 | #fafafa |
| surface-2 | #f5f5f5 |
| surface-3 | #ededed |
| bg-input | #ffffff |
| border-subtle | #f1f1f1 |
| border | #e9e9e9 |
| border-strong | #d6d6d6 |
| text | #161616 |
| text-secondary | #6e6e6e |
| text-muted | #8a8a8a |
| text-faint | #c2c2c2 |
| primary | #4f46e5 |
| accent | #4f46e5 |
| accent-hover | #4338ca |
| accent-active | #3730a3 |
| accent-tint | rgba(79, 70, 229, 0.08) |
| fill-inverse | #161616 |
| content-inverse | #ffffff |
| hover-surface | rgba(22, 22, 22, 0.045) |
| green | #2f7d4f |
| red | #c0454f |
| amber | #8f6a1f |

## Typography

界面文案用 Inter，命令、代码、按键标记、URL 用 JetBrains Mono。

正文用 `text-sm`，强调文字与表单取值用 `text-md`。标题用 `text-lg` 至 `text-xl`，字重 600，收紧字距。中文界面下不给标题加字距，英文大写小标题才使用字母间距。

数据类文字（时间戳、容量、进度、键码）用等宽数字。标题做两端平衡换行，段落做避免孤字换行，长文本截断用单行省略或行数夹取。

## Layout

主窗是 400 至 480 宽的窄窗。标题栏高 38px，内容左右留白 20px。

设置页按分组组织：分组卡片圆角 10px，组内字段行最小高 34px，标签在左、控件在右，控件宽度上限 190px，右对齐。输入类控件占满所在列，不做整行通铺。

空态给一句动作指引和一个明确操作，不给多条并列建议。

悬浮窗几何固定 248×40，不随内容变化，单行结果超出时省略。

## Elevation & Depth

平面优先。层次用面阶差与发丝线表达，不用光晕，不用彩色投影。

投影只有三档：`xs` 静置小元素，`sm` 徽标与分段控件，`md` 对话框与浮层。悬浮窗不使用投影（部分 Linux 合成器会把半透明阴影预乘成黑影）。

焦点态用 1px 强调色环，输入类控件聚焦时同时把边框切到 `accent`。

## Shapes

控件圆角 6px，面板、卡片与对话框圆角 10px，主窗圆角 14px。圆角克制，只有状态点、单选框与头像类元素使用全圆角。

键位标记用 4px 圆角，带 1px 边框与浅底，等宽字体。

## Components

按钮三档高度：28px 默认、24px 紧凑、32px 加大，字号 13px，字重 500。

按钮四个变体：主操作反色实心（`fill-inverse` 加 `content-inverse`），每个视图至多一处；次要按钮 `surface-1` 底加发丝边框，悬停升到 `surface-2`；幽灵按钮透明底，悬停浮 `hover-surface`；危险操作默认只染红文字，悬停才浮现红底与红边。

下拉与文本输入共用同一盒式基座：高度 28px，内边距 10px，6px 圆角，token 化底色与边框，聚焦切 `accent` 边框并加 1px 色环。任何单写 `.select` 的标记都必须拿到完整皮肤，不允许出现系统原生外观。

徽标默认是描边形态，选中与成功状态才使用语义底色。勾选框与单选框自绘外观，选中态用 `accent` 填充。

服务商字母标记用中性底色，不引入第二种彩色。

## Do's and Don'ts

- 不用光晕与彩色投影。
- 不用渐变，吸底栏与遮罩用实色。
- 不用强调色做装饰。
- 不给交互反馈加超过 200ms 的过渡。
- 不动画布局属性，进度条等小面积指示器除外。
- 不依赖系统原生控件外观。
- 尊重 prefers-reduced-motion。
