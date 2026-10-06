// 把品牌图标 SVG 光栅化成应用需要的全部位图。
// 用法：node tools/assets/build-icon.mjs [--out src-tauri/icons] [--web out.svg]
// 产出两类：
//   1. 应用图标（altgo-icon.svg，带靛蓝底方块）→ src-tauri/icons 全套 + icon.ico
//   2. 桌面快捷方式标记（altgo-mark.svg，透明底纯标记）→ assets/icon.png 与 icon-transparent.png
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Resvg } from '@resvg/resvg-js';
import { PNG } from 'pngjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');

const argv = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};

const appSvgPath = path.resolve(repo, arg('svg', 'tools/assets/altgo-icon.svg'));
const markSvgPath = path.resolve(repo, 'tools/assets/altgo-mark.svg');
const outDir = path.resolve(repo, arg('out', 'src-tauri/icons'));

const appSvg = await readFile(appSvgPath);
const markSvg = await readFile(markSvgPath);
await mkdir(outDir, { recursive: true });

/** 用 resvg 把 SVG 渲染成指定边长的 PNG 缓冲（保留透明通道）。 */
const render = (svg, size) =>
  new Resvg(svg, { fitTo: { mode: 'width', value: size }, background: 'rgba(0,0,0,0)' }).render().asPng();

/**
 * 按 ICO 规范打包多张 PNG：目录项里存 PNG 原始字节，256 及以上写 0 占位。
 * PNG 条目自带 alpha 通道，因此从 16 px 到 256 px 都保留透明背景。
 */
function encodeIco(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(entries.length, 4);

  const directory = Buffer.alloc(entries.length * 16);
  let offset = header.length + directory.length;
  entries.forEach((entry, i) => {
    const at = i * 16;
    directory.writeUInt8(entry.size >= 256 ? 0 : entry.size, at + 0);
    directory.writeUInt8(entry.size >= 256 ? 0 : entry.size, at + 1);
    directory.writeUInt8(0, at + 2); // 调色板数
    directory.writeUInt8(0, at + 3); // reserved
    directory.writeUInt16LE(1, at + 4); // 色彩平面
    directory.writeUInt16LE(32, at + 6); // 位深
    directory.writeUInt32LE(entry.data.length, at + 8);
    directory.writeUInt32LE(offset, at + 12);
    offset += entry.data.length;
  });

  return Buffer.concat([header, directory, ...entries.map((e) => e.data)]);
}

/** 校验位图确实带透明像素，避免"以为透明其实铺了底色"。 */
function assertTransparent(pngBuffer, label) {
  const png = PNG.sync.read(pngBuffer);
  let transparent = 0;
  for (let i = 3; i < png.data.length; i += 4) {
    if (png.data[i] === 0) transparent += 1;
  }
  const ratio = transparent / (png.width * png.height);
  console.log(`  ${label}: 完全透明像素占 ${(ratio * 100).toFixed(1)}%`);
  if (ratio < 0.2) throw new Error(`${label} 的透明区域过少，可能没画成透明底`);
}

// ── 1. 应用图标（带底方块） ───────────────────────────────
const pngTargets = [
  [32, '32x32.png'],
  [128, '128x128.png'],
  [256, '128x128@2x.png'],
];
for (const [size, name] of pngTargets) {
  await writeFile(path.join(outDir, name), render(appSvg, size));
  console.log(`写出 ${name} (${size}px)`);
}

await writeFile(path.join(outDir, 'icon.png'), render(appSvg, 512));
console.log('写出 icon.png (512px)');

// Windows 图标包：16/24/32/48/64/128/256 全覆盖，全部条目都是 PNG 以保留透明通道
const icoSizes = [16, 24, 32, 48, 64, 128, 256];
const icoEntries = icoSizes.map((size) => ({ size, data: render(appSvg, size) }));
await writeFile(path.join(outDir, 'icon.ico'), encodeIco(icoEntries));
console.log(`写出 icon.ico（含 ${icoSizes.join('/')} 共 ${icoSizes.length} 档，均保留透明通道）`);

// ── 2. 桌面快捷方式标记（透明底裸标记） ───────────────────
const markTargets = [
  [256, 'assets/icon.png'],
  [512, 'assets/icon-transparent.png'],
];
for (const [size, rel] of markTargets) {
  const buffer = render(markSvg, size);
  await writeFile(path.resolve(repo, rel), buffer);
  console.log(`写出 ${rel} (${size}px)`);
  assertTransparent(buffer, rel);
}

// 前端窗口图标：与应用图标同源，避免标题栏与系统图标不一致
// （--web none 表示只渲染位图，不同步前端 SVG。空串会被缺省值兜住，故用 none）
const webTarget = arg('web', 'frontend/public/altgo-logo.svg');
if (webTarget !== 'none') {
  await writeFile(path.resolve(repo, webTarget), appSvg);
  console.log(`同步前端图标 ${webTarget}`);
  // 单色变体也一并同步：界面内按上下文着色时用它，不是位图
  const mono = await readFile(path.resolve(repo, 'tools/assets/altgo-mark-mono.svg'));
  await writeFile(path.resolve(repo, 'frontend/public/altgo-mark.svg'), mono);
  console.log('同步前端单色标记 frontend/public/altgo-mark.svg');
}
