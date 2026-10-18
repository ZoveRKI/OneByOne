import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Markdown } from './markdown';

const html = (source: string) => {
  const { container } = render(
    <div>
      <Markdown source={source} />
    </div>,
  );
  return container.firstElementChild?.innerHTML;
};

describe('Markdown', () => {
  it('段落、换行、标题和引用', () => {
    expect(html('第一行\n第二行\n\n## 小标题\n> 引用一\n> 引用二')).toBe(
      '<p>第一行<br>第二行</p><h4>小标题</h4><blockquote>引用一<br>引用二</blockquote>',
    );
  });

  it('有序和无序列表', () => {
    expect(html('- a\n- **b**\n1. x\n2) y')).toBe(
      '<ul><li>a</li><li><strong>b</strong></li></ul><ol><li>x</li><li>y</li></ol>',
    );
  });

  it('行内代码里的内容原样显示', () => {
    expect(html('用 `x<y` 比较 **大小**')).toBe('<p>用 <code>x&lt;y</code> 比较 <strong>大小</strong></p>');
  });

  it('只把 http 和 https 地址变成链接', () => {
    expect(html('[文档](https://example.com) [坏](javascript:alert(1))')).toBe(
      '<p><a href="https://example.com" target="_blank" rel="noopener noreferrer">文档</a> [坏](javascript:alert(1))</p>',
    );
  });

  it('讲解里的 HTML 不会被执行', () => {
    expect(html('<img src=x onerror=alert(1)>')).toBe('<p>&lt;img src=x onerror=alert(1)&gt;</p>');
  });

  it('代码块按语言高亮', () => {
    expect(html('```js\nconst a = 1;\nreturn a;\n```')).toBe(
      '<pre><code><span class="tok-kw">const</span> a = <span class="tok-num">1</span>;\n<span class="tok-kw">return</span> a;</code></pre>',
    );
  });
});
