// 生成发布包里的第三方许可证声明（THIRD-PARTY-NOTICES.md）。
//
// 用法：node .github/scripts/third-party-notices.mjs <Rust 目标平台> <输出文件>
// 例如：node .github/scripts/third-party-notices.mjs x86_64-pc-windows-msvc dist/THIRD-PARTY-NOTICES.md
//
// 发布的程序里有两类第三方代码：编进 onebyone 的 Rust 依赖，和打包进内嵌播放器页面的前端依赖。
// 它们的许可证要求分发时附上版权和许可声明，这里收集每个依赖自带的 LICENSE、NOTICE 等文件，
// 相同的全文只写一次。依赖信息来自 cargo metadata 和 pnpm licenses list，不需要额外安装工具。

import { execFileSync, execSync } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const [target, output] = process.argv.slice(2);
if (!target || !output) {
  console.error('用法：node .github/scripts/third-party-notices.mjs <Rust 目标平台> <输出文件>');
  process.exit(1);
}

const root = fileURLToPath(new URL('../..', import.meta.url));
// LICENSE、LICENSE-MIT、COPYING、NOTICE、UNLICENSE 等
const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-._].*)?$/i;

function readText(path) {
  return readFileSync(path, 'utf8').replace(/\r\n?/g, '\n').trim();
}

function licenseFiles(dir, extra) {
  let names = [];
  try {
    names = readdirSync(dir).filter((name) => LICENSE_FILE.test(name));
  } catch {
    // 目录不存在时当作没有许可证文件
  }
  if (extra && !names.includes(extra)) names.push(extra);
  return names.toSorted().map((name) => ({ name, text: readText(join(dir, name)) }));
}

function rustPackages() {
  const json = execFileSync(
    'cargo',
    ['metadata', '--format-version', '1', '--locked', '--filter-platform', target],
    { cwd: join(root, 'server'), encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 },
  );
  const meta = JSON.parse(json);
  const packages = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  // 从 onebyone 出发只沿普通依赖走：开发依赖（测试用）和构建依赖（build.rs 用）不会进入发布的程序
  const used = new Set();
  const walk = (id) => {
    if (used.has(id)) return;
    used.add(id);
    for (const dep of nodes.get(id).deps) {
      if (dep.dep_kinds.some((k) => k.kind === null)) walk(dep.pkg);
    }
  };
  walk(meta.resolve.root);
  used.delete(meta.resolve.root);
  return [...used].map((id) => {
    const p = packages.get(id);
    return {
      name: p.name,
      version: p.version,
      license: p.license ?? '见许可证文件',
      url: p.repository ?? p.homepage ?? `https://crates.io/crates/${p.name}`,
      files: licenseFiles(dirname(p.manifest_path), p.license_file),
    };
  });
}

function webPackages() {
  // Windows 上的 pnpm 是 .cmd 脚本，要经过 shell 启动，所以用整条命令执行；命令是固定的，不涉及外部输入
  const json = execSync('pnpm licenses list --prod --json', {
    cwd: join(root, 'web'),
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  return Object.values(JSON.parse(json)).flatMap((list) =>
    list.flatMap((p) =>
      p.versions.map((version, i) => ({
        name: p.name,
        version,
        license: p.license,
        url: p.homepage ?? `https://www.npmjs.com/package/${p.name}`,
        files: licenseFiles(p.paths[i]),
      })),
    ),
  );
}

const byName = (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version);
const groups = [
  { title: 'Rust 依赖（编进 onebyone 程序）', packages: rustPackages().toSorted(byName) },
  { title: '前端依赖（打包进内嵌的播放器页面）', packages: webPackages().toSorted(byName) },
];

// 许可证全文去重：内容相同的文件只写一次，标出使用它的依赖
const texts = new Map();
for (const { packages } of groups) {
  for (const p of packages) {
    for (const file of p.files) {
      const entry = texts.get(file.text) ?? { users: [], names: new Set() };
      entry.users.push(`${p.name} ${p.version}`);
      entry.names.add(file.name);
      texts.set(file.text, entry);
    }
  }
}

// 代码块的围栏要比全文里最长的一串反引号更长，避免提前结束
const fence = (text) => '`'.repeat(Math.max(3, ...(text.match(/`+/g) ?? []).map((s) => s.length + 1)));
const cell = (value) => String(value).replaceAll('|', '\\|');

const out = [
  '# 第三方许可证声明',
  '',
  'OneByOne 的发布包里包含以下第三方软件，它们按各自的许可证授权。下面先列出每个依赖的名称、版本和许可证，再附上依赖自带的许可证文件全文（内容相同的只列一次）。',
  '',
  `本文件由 \`.github/scripts/third-party-notices.mjs\` 生成，对应的平台：\`${target}\`。`,
  '',
];
for (const { title, packages } of groups) {
  out.push(`## ${title}`, '', '| 名称 | 版本 | 许可证 | 来源 |', '|---|---|---|---|');
  for (const p of packages) out.push(`| ${cell(p.name)} | ${p.version} | ${cell(p.license)} | ${p.url} |`);
  out.push('');
}
const missing = groups.flatMap((g) => g.packages).filter((p) => !p.files.length);
if (missing.length) {
  out.push(
    '以下依赖的发布包里没有附带许可证文件，许可证以上表为准；MIT、Apache-2.0 等标准许可证的全文，见下方其他依赖附带的同类文件，也可以到它们的来源查看：',
    '',
    ...missing.map((p) => `- ${p.name} ${p.version}（${p.license}）`),
    '',
  );
}
out.push('## 许可证全文', '');
let n = 0;
for (const [text, { users, names }] of texts) {
  n += 1;
  const f = fence(text);
  out.push(`### ${n}. ${[...names].join('、')}`, '', `使用它的依赖：${users.join('、')}`, '', f, text, f, '');
}

mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, out.join('\n'));
const total = groups.reduce((sum, g) => sum + g.packages.length, 0);
console.log(`已生成 ${output}：${total} 个依赖，${texts.size} 份许可证全文`);
