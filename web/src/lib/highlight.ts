/*
 * 轻量语法高亮：只区分注释、字符串、数字、关键字、函数名、类型名和标签。
 * 输出词法片段而不是 HTML，由组件负责渲染，讲解和代码里的内容不会被当成 HTML 执行。
 */
import { baseName } from './format';

export interface Token {
  /** 对应样式类 tok-xxx；null 表示普通文本 */
  cls: string | null;
  text: string;
}

export interface Lang {
  re: RegExp;
  kw: Set<string>;
  block: [string, string] | null;
  ci: boolean;
}

/** 逐行高亮时记录上一行是否停在块注释里 */
export interface HighlightState {
  inBlock: boolean;
}

interface LangConfig {
  line: string | null;
  block: [string, string] | null;
  kw: string;
  tags?: boolean;
  ci?: boolean;
}

const C_KEYWORDS =
  'abstract as async await break case catch class const continue debugger default defer delete do else enum export extends false final finally fn for from func function go if impl implements import in instanceof interface let match mod mut new nil null of package private protected pub public readonly return self static struct super switch this throw throws trait true try type typeof undefined use var void while with yield';
const HASH_KEYWORDS =
  'and as assert async await break case class continue def del do done elif else esac except export false fi finally for from function global if import in is lambda local nonlocal not null or pass raise return self then true try until while with yield None True False';
const SQL_KEYWORDS =
  'all alter and as asc by case create default delete desc distinct drop else end exists foreign from group having in index inner insert into is join key left like limit not null on or order outer primary references right select set table then union update values when where';

const LANGS: Record<string, LangConfig> = {
  clike: { line: '\\/\\/.*', block: ['/*', '*/'], kw: C_KEYWORDS },
  jsx: { line: '\\/\\/.*', block: ['/*', '*/'], kw: C_KEYWORDS, tags: true },
  css: { line: null, block: ['/*', '*/'], kw: 'important' },
  hash: { line: '#.*', block: null, kw: HASH_KEYWORDS },
  sql: { line: '--.*', block: ['/*', '*/'], kw: SQL_KEYWORDS, ci: true },
  markup: { line: null, block: ['<!--', '-->'], kw: '', tags: true },
};

// prettier-ignore
const LANG_OF: Record<string, string> = {
  js: 'clike', mjs: 'clike', cjs: 'clike', ts: 'clike', mts: 'clike', cts: 'clike',
  javascript: 'clike', typescript: 'clike', jsx: 'jsx', tsx: 'jsx',
  java: 'clike', kt: 'clike', kts: 'clike', scala: 'clike', groovy: 'clike', c: 'clike', h: 'clike',
  cc: 'clike', cpp: 'clike', hpp: 'clike', cs: 'clike', go: 'clike', rs: 'clike', swift: 'clike',
  dart: 'clike', php: 'clike', json: 'clike', jsonc: 'clike', scss: 'clike', less: 'clike', css: 'css',
  py: 'hash', python: 'hash', rb: 'hash', ruby: 'hash', sh: 'hash', bash: 'hash', zsh: 'hash',
  shell: 'hash', ps1: 'hash', yaml: 'hash', yml: 'hash', toml: 'hash', r: 'hash', pl: 'hash',
  dockerfile: 'hash', makefile: 'hash', sql: 'sql', lua: 'sql',
  html: 'markup', htm: 'markup', xml: 'markup', svg: 'markup', vue: 'markup', svelte: 'markup',
};

const compiled = new Map<string, Lang>();
const FN_AFTER = /\s*\(/y;
const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** 按扩展名或代码块语言名找到高亮规则，找不到时返回 null（按纯文本显示） */
export function getLang(key: string | null | undefined): Lang | null {
  const name = String(key ?? '').toLowerCase();
  const id = Object.hasOwn(LANG_OF, name) ? LANG_OF[name] : undefined;
  const cfg = id ? LANGS[id] : undefined;
  if (!id || !cfg) return null;
  let lang = compiled.get(id);
  if (!lang) {
    const parts: string[] = [];
    if (cfg.line) parts.push(`(?<com>${cfg.line})`);
    if (cfg.block) parts.push(`(?<bcom>${escapeRe(cfg.block[0])}[\\s\\S]*?(?:${escapeRe(cfg.block[1])}|$))`);
    parts.push('(?<str>"(?:[^"\\\\]|\\\\.)*"?|\'(?:[^\'\\\\]|\\\\.)*\'?|`(?:[^`\\\\]|\\\\.)*`?)');
    if (cfg.tags) parts.push('(?<tag><\\/?[A-Za-z][\\w:.-]*)');
    parts.push('(?<num>\\b(?:0[xX][\\da-fA-F]+|\\d[\\d_]*(?:\\.\\d+)?(?:[eE][+-]?\\d+)?)\\b)');
    parts.push('(?<id>[A-Za-z_$][\\w$]*)');
    lang = {
      re: new RegExp(parts.join('|'), 'g'),
      kw: new Set(cfg.kw.split(' ').filter(Boolean)),
      block: cfg.block,
      ci: Boolean(cfg.ci),
    };
    compiled.set(id, lang);
  }
  return lang;
}

export function langOfPath(path: string): Lang | null {
  const name = baseName(path).toLowerCase();
  const dot = name.lastIndexOf('.');
  return getLang(dot >= 0 ? name.slice(dot + 1) : name);
}

export function highlightLine(text: string, lang: Lang | null, st: HighlightState): Token[] {
  const out: Token[] = [];
  const push = (cls: string | null, value: string) => {
    if (value) out.push({ cls, text: value });
  };
  if (!lang) {
    push(null, text);
    return out;
  }
  let pos = 0;
  const block = lang.block;
  if (st.inBlock && block) {
    const end = text.indexOf(block[1]);
    if (end < 0) {
      push('com', text);
      return out;
    }
    pos = end + block[1].length;
    push('com', text.slice(0, pos));
    st.inBlock = false;
  }
  const re = lang.re;
  re.lastIndex = pos;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    const tok = m[0];
    if (!tok) {
      re.lastIndex += 1;
      continue;
    }
    push(null, text.slice(pos, m.index));
    const g = m.groups ?? {};
    if (g.com !== undefined) {
      push('com', tok);
    } else if (g.bcom !== undefined) {
      push('com', tok);
      if (block && (tok.length < block[0].length + block[1].length || !tok.endsWith(block[1])))
        st.inBlock = true;
    } else if (g.str !== undefined) {
      push('str', tok);
    } else if (g.tag !== undefined) {
      push('tag', tok);
    } else if (g.num !== undefined) {
      push('num', tok);
    } else {
      let cls: string | null = null;
      if (lang.kw.has(lang.ci ? tok.toLowerCase() : tok)) {
        cls = 'kw';
      } else {
        FN_AFTER.lastIndex = re.lastIndex;
        if (FN_AFTER.test(text)) cls = 'fn';
        else if (/^[A-Z]/.test(tok)) cls = 'type';
      }
      push(cls, tok);
    }
    pos = re.lastIndex;
  }
  push(null, text.slice(pos));
  return out;
}

/** 高亮一段多行代码，返回每一行的词法片段 */
export function highlightBlock(code: string, langKey: string): Token[][] {
  const lang = getLang(langKey);
  const st: HighlightState = { inBlock: false };
  return code.split('\n').map((line) => highlightLine(line, lang, st));
}
