// 校验三张悬浮窗相位图的结构（标签平衡、无旧值残留、画布尺寸正确）。
// 用法：node tools/assets/check-overlay-phases.mjs
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');

/** 旧版演示图用过的值，出现即说明改样式后漏改插图。 */
const STALE = ['stroke-dasharray="4 3"', '#f87171', '#fbbf24', '#4ade80', 'rx="20"'];
const PHASES = ['recording', 'transcribing', 'result'];

let failed = 0;
for (const phase of PHASES) {
  const rel = `assets/overlay-phase-${phase}.svg`;
  const svg = await readFile(path.join(repo, rel), 'utf8');

  const open = (svg.match(/<[a-zA-Z]/g) || []).length;
  const close = (svg.match(/<\//g) || []).length;
  const selfClose = (svg.match(/\/>/g) || []).length;
  const balanced = open === close + selfClose;
  const stale = STALE.filter((s) => svg.includes(s));
  const viewBox = (svg.match(/viewBox="([^"]+)"/) || [])[1];

  if (!balanced || stale.length || viewBox !== '0 0 440 40') failed += 1;
  console.log(
    `${phase}: 标签平衡=${balanced ? '是' : '否'}，画布=${viewBox}，残留旧值=${stale.join(' ') || '无'}`,
  );
}

console.log(failed ? `${failed} 个文件不合格` : '三张相位图均合格');
process.exit(failed ? 1 : 0);
