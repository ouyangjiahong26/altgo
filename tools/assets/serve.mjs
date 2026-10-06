// 极简静态文件服务器，用于截图核对（Node 内置 http，无依赖）。
// 用法：node tools/assets/serve.mjs --root frontend --port 4180
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
const argv = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};

const root = path.resolve(repo, arg('root', 'frontend'));
const port = Number(arg('port', '4180'));

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.woff2': 'font/woff2',
};

// 根目录映射：/ 指向 frontend（产品样式与审查页），/repo 指向仓库根（assets、src-tauri/icons 等）。
// 审查页要同时引用两边，单一 root 做不到，所以按前缀映射，避免再起第二个端口。
const MOUNTS = [
  { prefix: '/repo', dir: repo },
  { prefix: '/', dir: root },
];

/** 按最长前缀匹配把 URL 路径解析成磁盘路径，并挡住目录穿越。 */
function resolve(urlPath) {
  const decoded = decodeURIComponent(urlPath);
  const mount = MOUNTS.filter((m) => m.prefix === '/' || decoded === m.prefix || decoded.startsWith(`${m.prefix}/`))
    .sort((a, b) => b.prefix.length - a.prefix.length)[0];
  const rest = mount.prefix === '/' ? decoded : decoded.slice(mount.prefix.length) || '/';
  const filePath = path.join(mount.dir, rest);
  // startsWith 必须带上分隔符，否则 /repo-evil 这类同级目录也会被前缀匹配放过。
  // decodeURIComponent 在 join 之前发生，%2e%2e%2f 这类编码穿越靠这一步兜底挡住。
  return filePath === mount.dir || filePath.startsWith(mount.dir + path.sep) ? filePath : null;
}

createServer(async (req, res) => {
  const started = Date.now();
  res.on('finish', () => {
    // 每条请求都打日志：排查相对路径与 MIME 问题时至关重要
    process.stdout.write(`${res.statusCode} ${req.method} ${req.url} (${Date.now() - started}ms)\n`);
  });
  try {
    const url = new URL(req.url, `http://127.0.0.1:${port}`);
    let filePath = resolve(url.pathname);
    if (!filePath) {
      res.writeHead(403).end('forbidden');
      return;
    }
    // 目录请求回落到 index.html
    const info = await stat(filePath).catch(() => null);
    if (!info || info.isDirectory()) filePath = path.join(filePath, 'index.html');

    const body = await readFile(filePath);
    const ext = path.extname(filePath).toLowerCase();
    let payload = body;
    let type = TYPES[ext] || 'application/octet-stream';

    // CSS 的 @import 用相对路径时，解析结果依赖浏览器以哪个 URL 为基准。
    // 截图核对时统一改写成以本文件目录为基准的绝对路径，避免歧义。
    // 兼容 @import 'x'、@import "x"、@import url(x)、@import url('x') 四种写法。
    if (ext === '.css') {
      const dir = path.posix.dirname(url.pathname);
      const rewritten = body
        .toString('utf8')
        .replace(/@import\s+(?:url\(\s*)?(['"]?)([^'")]+)\1\s*\)?/g, (whole, quote, target) => {
          if (/^(https?:)?\/\//.test(target) || target.startsWith('data:')) return whole;
          const abs = target.startsWith('/') ? target : path.posix.join(dir, target);
          return `@import url("${abs}")`;
        });
      payload = Buffer.from(rewritten, 'utf8');
      type = 'text/css; charset=utf-8';
    }

    res.writeHead(200, { 'Content-Type': type, 'Cache-Control': 'no-store' });
    res.end(payload);
  } catch {
    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' }).end('not found');
  }
}).listen(port, '127.0.0.1', () => {
  process.stdout.write(
    `静态服务器已启动：http://127.0.0.1:${port}/（/ → ${root}，/repo → ${repo}）\n`,
  );
});
