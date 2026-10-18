/*
 * 简易 Markdown：段落、标题、列表、引用、代码块、行内代码、加粗和链接。
 * 直接生成 React 元素，讲解里写的 HTML 只会原样显示，不会被执行。
 */
import { Fragment, type ReactNode } from 'react';
import { Tokens } from '../components/Tokens';
import { highlightBlock } from './highlight';

const BLOCK_START = /^(```|#{1,4}\s|\s*[-*]\s+|\s*\d+[.)]\s+|>)/;
const UL_ITEM = /^\s*[-*]\s+/;
const OL_ITEM = /^\s*\d+[.)]\s+/;
const LINK = /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g;

export function Markdown({ source }: { source: string }) {
  return renderBlocks(source);
}

export function renderBlocks(src: string): ReactNode[] {
  const lines = src.replace(/\r\n?/g, '\n').split('\n');
  const out: ReactNode[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? '';
    const key = out.length;
    if (!line.trim()) {
      i += 1;
      continue;
    }

    if (line.startsWith('```')) {
      const lang = line.slice(3).trim();
      const buf: string[] = [];
      i += 1;
      while (i < lines.length && !(lines[i] ?? '').startsWith('```')) buf.push(lines[i++] ?? '');
      i += 1;
      out.push(
        <pre key={key}>
          <code>
            {highlightBlock(buf.join('\n'), lang).map((tokens, li) => (
              <Fragment key={li}>
                {li > 0 && '\n'}
                <Tokens tokens={tokens} />
              </Fragment>
            ))}
          </code>
        </pre>,
      );
      continue;
    }

    const heading = /^#{1,4}\s+(.*)$/.exec(line);
    if (heading) {
      out.push(<h4 key={key}>{inline(heading[1] ?? '')}</h4>);
      i += 1;
      continue;
    }

    const itemRe = UL_ITEM.test(line) ? UL_ITEM : OL_ITEM.test(line) ? OL_ITEM : null;
    if (itemRe) {
      const items: string[] = [];
      while (i < lines.length && itemRe.test(lines[i] ?? ''))
        items.push((lines[i++] ?? '').replace(itemRe, ''));
      const children = items.map((t, n) => <li key={n}>{inline(t)}</li>);
      out.push(itemRe === UL_ITEM ? <ul key={key}>{children}</ul> : <ol key={key}>{children}</ol>);
      continue;
    }

    if (line.startsWith('>')) {
      const buf: string[] = [];
      while (i < lines.length && (lines[i] ?? '').startsWith('>'))
        buf.push((lines[i++] ?? '').replace(/^>\s?/, ''));
      out.push(<blockquote key={key}>{withBreaks(buf)}</blockquote>);
      continue;
    }

    const buf = [line];
    i += 1;
    while (i < lines.length && (lines[i] ?? '').trim() && !BLOCK_START.test(lines[i] ?? ''))
      buf.push(lines[i++] ?? '');
    out.push(<p key={key}>{withBreaks(buf)}</p>);
  }
  return out;
}

function withBreaks(lines: string[]): ReactNode[] {
  return lines.map((l, n) => (
    <Fragment key={n}>
      {n > 0 && <br />}
      {inline(l)}
    </Fragment>
  ));
}

/** 行内格式：先切出 `代码`，其余部分再处理加粗和链接 */
export function inline(text: string): ReactNode[] {
  return text.split(/(`[^`]+`)/).map((part, n) => {
    if (part.length > 2 && part.startsWith('`') && part.endsWith('`'))
      return <code key={n}>{part.slice(1, -1)}</code>;
    return <Fragment key={n}>{bold(part)}</Fragment>;
  });
}

function bold(text: string): ReactNode[] {
  // split 带捕获组时，奇数位置是 **加粗** 的内容
  return text
    .split(/\*\*([^*]+)\*\*/)
    .map((part, n) =>
      n % 2 ? <strong key={n}>{links(part)}</strong> : <Fragment key={n}>{links(part)}</Fragment>,
    );
}

// 只认 http / https 链接，避免 javascript: 之类的地址
function links(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  let pos = 0;
  for (const m of text.matchAll(LINK)) {
    if (m.index > pos) out.push(text.slice(pos, m.index));
    out.push(
      <a key={m.index} href={m[2]} target="_blank" rel="noopener noreferrer">
        {m[1]}
      </a>,
    );
    pos = m.index + m[0].length;
  }
  if (pos < text.length) out.push(text.slice(pos));
  return out;
}
