// 把 SVG 渲染成位图资产（横幅、社交卡片等），并把 README 引用的矢量副本同步过去。
// 用法：node tools/assets/build-assets.mjs
import { readFile, writeFile, mkdir, copyFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Resvg } from '@resvg/resvg-js';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');

/** 用 resvg 渲染 SVG 文件到指定位图路径。 */
async function renderTo(svgRel, outRel, width) {
  const svg = await readFile(path.resolve(repo, svgRel));
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: width }, background: 'rgba(0,0,0,0)' })
    .render()
    .asPng();
  const out = path.resolve(repo, outRel);
  await mkdir(path.dirname(out), { recursive: true });
  await writeFile(out, png);
  console.log(`${outRel} ← ${svgRel}（宽 ${width}）`);
}

/**
 * README 直接内嵌 `assets/overlay-phase-*.svg`，母版在 `tools/assets/`。
 * 以前这两份靠手工同步，改样式后漏改过插图；这里改成构建期复制，
 * 母版即唯一来源，校验脚本继续只看 README 实际用的那份。
 */
async function copySvg(svgRel, outRel) {
  const out = path.resolve(repo, outRel);
  await mkdir(path.dirname(out), { recursive: true });
  await copyFile(path.resolve(repo, svgRel), out);
  console.log(`${outRel} ← ${svgRel}（复制）`);
}

await renderTo('tools/assets/banner.svg', 'assets/banner.png', 1536);
await renderTo('tools/assets/og-image.svg', 'assets/og-image.png', 1536);

for (const phase of ['recording', 'transcribing', 'result']) {
  await copySvg(`tools/assets/overlay-phase-${phase}.svg`, `assets/overlay-phase-${phase}.svg`);
}
