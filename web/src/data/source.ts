/*
 * 播放器的数据来源，按顺序尝试：
 * 1. 页面里内嵌的数据（命令行生成的 HTML 会写进 #onebyone-data）
 * 2. 通过 http 访问时，读取 ?src= 指定的文件，默认 walkthrough.json
 * 3. 都没有时显示欢迎页，让用户拖入或选择 JSON 文件
 */
import { normalize, type Model } from '../core/model';
import { errorMessage } from '../lib/format';

export type Initial =
  | { kind: 'data'; text: string; name: string; restore: boolean }
  | { kind: 'landing'; message: string };

export const EMBED_ID = 'onebyone-data';

export function readEmbedded(doc: Document = document): string | null {
  const text = doc.getElementById(EMBED_ID)?.textContent?.trim();
  return text && text !== 'null' ? text : null;
}

export async function loadInitial(): Promise<Initial> {
  const embedded = readEmbedded();
  if (embedded) return { kind: 'data', text: embedded, name: '页面内嵌的数据', restore: true };
  const src = new URLSearchParams(location.search).get('src') || 'walkthrough.json';
  if (location.protocol === 'file:') {
    return {
      kind: 'landing',
      message:
        '直接双击打开页面时，浏览器不允许它自动读取本地文件。请把 JSON 拖进来，或者点击上面的按钮选择。',
    };
  }
  try {
    const res = await fetch(src, { cache: 'no-store' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return { kind: 'data', text: await res.text(), name: src, restore: true };
  } catch (err) {
    return {
      kind: 'landing',
      message: `没能自动加载 ${src}（${errorMessage(err)}）。可以把 JSON 文件拖进来，或者点击上面的按钮选择。`,
    };
  }
}

export function parseWalkthrough(text: string, name: string): { model: Model } | { error: string } {
  try {
    return { model: normalize(JSON.parse(text)) };
  } catch (err) {
    const reason = err instanceof SyntaxError ? '不是合法的 JSON' : '的内容不符合格式要求';
    return { error: `${name} ${reason}：${errorMessage(err)}` };
  }
}
