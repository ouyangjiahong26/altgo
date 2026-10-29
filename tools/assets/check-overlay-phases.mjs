// 校验四张悬浮窗相位图的结构（结果相位含中英两版。检查标签平衡、XML 注释合法、无旧值残留、画布尺寸正确、母版与副本一致）。
// 用法：node tools/assets/check-overlay-phases.mjs
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');

/** 旧版演示图用过的值，出现即说明改样式后漏改插图。 */
const STALE = [
  'stroke-dasharray="4 3"',
  '#e06c75',
  '#d2a24c',
  '#67b487',
  '#63b381',
  '#4ade80',
  '#c8c8c8',
  '#222222',
  '#3a3a3a',
  'rgba(255,255,255,0.16)',
  'rx="6"',
  'rx="20"',
];
const PHASES = ['recording', 'transcribing', 'result', 'result-en'];

let failed = 0;
for (const phase of PHASES) {
  const masterRel = `tools/assets/overlay-phase-${phase}.svg`;
  const copyRel = `assets/overlay-phase-${phase}.svg`;
  const [master, copy] = await Promise.all([
    readFile(path.join(repo, masterRel), 'utf8'),
    readFile(path.join(repo, copyRel), 'utf8'),
  ]);

  const open = (master.match(/<[a-zA-Z]/g) || []).length;
  const close = (master.match(/<\//g) || []).length;
  const selfClose = (master.match(/\/>/g) || []).length;
  const balanced = open === close + selfClose;
  // XML 注释里不能出现 --，也不能以 - 结尾。CSS 变量名（如 --color-red）直接写进注释会让整张图解析失败。
  const commentBodies = [...master.matchAll(/<!--([\s\S]*?)-->/g)].map((m) => m[1]);
  const commentOpen = (master.match(/<!--/g) || []).length;
  const commentClose = (master.match(/-->/g) || []).length;
  const badComment =
    commentOpen !== commentClose ||
    commentBodies.some((body) => body.includes('--') || body.endsWith('-'));
  const stale = STALE.filter((s) => master.includes(s));
  const viewBox = (master.match(/viewBox="([^"]+)"/) || [])[1];
  const inSync = master === copy;

  if (!balanced || badComment || stale.length || !inSync || viewBox !== '0 0 440 40') failed += 1;
  console.log(
    `${phase}: 标签平衡=${balanced ? '是' : '否'}，注释合法=${badComment ? '否' : '是'}，画布=${viewBox}，残留旧值=${stale.join(' ') || '无'}，副本同步=${inSync ? '是' : '否'}`,
  );
}

console.log(failed ? `${failed} 个文件不合格` : '四张相位图均合格');
process.exit(failed ? 1 : 0);
