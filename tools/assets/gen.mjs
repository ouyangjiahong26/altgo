// 用 GPT 图像模型批量生成品牌视觉资产（Node 18+ 内置 fetch，无第三方依赖）。
// 用法：node tools/assets/gen.mjs [--prompts tools/assets/prompts.brand.json] [--out tools/assets/out]
//       [--concurrency 8] [--only id1,id2] [--filter 前缀]
// 密钥来源：环境变量 ALTGO_IMAGE_API_KEY，或 tools/assets/.env 里的同名一行（该文件不进版本库）。
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));

/** 按需读取 tools/assets/.env，只认 KEY=VALUE 形式，不覆盖已有环境变量。 */
async function loadDotEnv() {
  try {
    const text = await readFile(path.join(here, '.env'), 'utf8');
    for (const line of text.split(/\r?\n/)) {
      const match = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$/.exec(line);
      if (match && !process.env[match[1]]) process.env[match[1]] = match[2].replace(/^["']|["']$/g, '');
    }
  } catch {
    // 没有 .env 就只靠环境变量
  }
}
await loadDotEnv();

const argv = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};

const baseUrl = (process.env.ALTGO_IMAGE_BASE_URL || 'http://lingganyaapi.com').replace(/\/+$/, '');
const apiKey = process.env.ALTGO_IMAGE_API_KEY;
if (!apiKey) {
  console.error('缺少 API key：在 tools/assets/.env 写 ALTGO_IMAGE_API_KEY=...，或设同名环境变量');
  process.exit(1);
}

const promptPath = path.resolve(here, '..', '..', arg('prompts', 'tools/assets/prompts.brand.json'));
const outRoot = path.resolve(here, '..', '..', arg('out', 'tools/assets/out'));
const concurrency = Math.max(1, Number(arg('concurrency', '6')));
const only = arg('only', '');
const filter = arg('filter', '');

const spec = JSON.parse(await readFile(promptPath, 'utf8'));
const model = spec.model;
const modelDir = path.join(outRoot, model.replace(/[^A-Za-z0-9._-]/g, '_'));
await mkdir(modelDir, { recursive: true });

const wanted = spec.items.filter(
  (it) => (!only || only.split(',').includes(it.id)) && (!filter || it.id.startsWith(filter)),
);
console.log(`模型 ${model}，待生成 ${wanted.length} 张，并发 ${concurrency} → ${modelDir}`);

const manifestPath = path.join(modelDir, 'index.json');
let manifest = [];
try {
  manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
} catch {
  // 首次运行没有清单
}

/** 生成单张图，失败按指数退避重试 3 次后放弃。 */
async function generate(item) {
  const size = item.size || '1024x1024';
  const file = path.join(modelDir, `${item.id}.png`);
  const started = Date.now();

  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const res = await fetch(`${baseUrl}/v1/images/generations`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${apiKey}`, 'Content-Type': 'application/json' },
        // 各家模型的尺寸参数名不统一：OpenAI 系用 size，nano_banana 系要 resolution。
        // 请求体里的额外字段由 prompts.json 的 item.extra 透传。
        body: JSON.stringify({ model, prompt: item.prompt, n: 1, size, ...(item.extra || {}) }),
        signal: AbortSignal.timeout(600_000),
      });
      if (!res.ok) {
        const detail = (await res.text()).slice(0, 200);
        console.log(`[${item.id}] 第 ${attempt} 次失败：HTTP ${res.status} ${detail}`);
      } else {
        const json = await res.json();
        const first = json.data?.[0];
        if (first?.b64_json) {
          await writeFile(file, Buffer.from(first.b64_json, 'base64'));
          return { ok: true, file, seconds: (Date.now() - started) / 1000 };
        }
        if (first?.url) {
          // 签名 URL 会过期：非 2xx 时把错误页字节写成 png 会以假图混进清单，按失败重试。
          const img = await fetch(first.url, { signal: AbortSignal.timeout(300_000) });
          if (!img.ok) throw new Error(`图片 URL 下载失败：HTTP ${img.status}`);
          await writeFile(file, Buffer.from(await img.arrayBuffer()));
          return { ok: true, file, seconds: (Date.now() - started) / 1000 };
        }
        console.log(`[${item.id}] 返回体里没有图片数据：${JSON.stringify(json).slice(0, 200)}`);
      }
    } catch (err) {
      console.log(`[${item.id}] 第 ${attempt} 次异常：${String(err?.message || err)}`);
    }
    if (attempt < 3) await new Promise((r) => setTimeout(r, 1500 * attempt));
  }
  return { ok: false, seconds: (Date.now() - started) / 1000 };
}

// 简单的并发池：固定数量的 worker 消费同一个任务队列
const queue = [...wanted];
const results = [];
let cursor = 0;
await Promise.all(
  Array.from({ length: Math.min(concurrency, queue.length) }, async () => {
    while (cursor < queue.length) {
      const item = queue[cursor++];
      const outcome = await generate(item);
      const seconds = outcome.seconds.toFixed(1);
      if (outcome.ok) {
        console.log(`[${item.id}] OK ${seconds}s ${outcome.file}`);
        manifest = manifest.filter((m) => m.id !== item.id);
        manifest.push({ id: item.id, size: item.size || '1024x1024', file: outcome.file, seconds: Number(seconds) });
      } else {
        console.log(`[${item.id}] FAILED ${seconds}s`);
      }
      results.push(outcome.ok);
      await writeFile(manifestPath, JSON.stringify(manifest, null, 2), 'utf8');
    }
  }),
);

const failed = results.filter((ok) => !ok).length;
console.log(`完成 ${results.length - failed}/${wanted.length}，失败 ${failed}`);
console.log(`清单：${manifestPath}`);
process.exit(wanted.length > 0 && failed === wanted.length ? 1 : 0);
