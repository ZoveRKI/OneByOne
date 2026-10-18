import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { App } from './App';
import type { Initial } from './data/source';

const data = {
  title: '测试改动',
  hunks: [
    { id: 'h1', file: 'src/a.ts', oldStart: 1, newStart: 1, lines: ['-const a = 1;', '+const a = 2;'] },
    { id: 'h2', file: 'src/b.ts', oldStart: 0, newStart: 1, lines: ['+export {};'] },
  ],
  steps: [
    { title: '概览', hunks: [], explain: '整体思路' },
    { title: '修改 a', hunks: ['h1'], explain: '把 **a** 改成 2' },
  ],
};

const loadOf = (initial: Initial) => () => Promise.resolve(initial);
const title = (name: string) => screen.getByRole('heading', { level: 1, name });

describe('App', () => {
  it('加载数据后可以用按钮和键盘逐步切换', async () => {
    render(<App load={loadOf({ kind: 'data', text: JSON.stringify(data), name: 'test', restore: false })} />);
    expect(await screen.findByRole('heading', { level: 1, name: '概览' })).toBeTruthy();
    expect(screen.getByText('1 / 3')).toBeTruthy();
    expect(screen.getByText('这一步没有代码改动，只有讲解。')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    expect(title('修改 a')).toBeTruthy();
    // 上一步的内容不能残留在页面上
    expect(screen.queryByText('这一步没有代码改动，只有讲解。')).toBeNull();
    expect(screen.getAllByRole('complementary', { name: '讲解' })).toHaveLength(1);
    expect(screen.getByText('a').tagName).toBe('STRONG');

    // 第三步是自动补上的「未讲解的改动」
    fireEvent.keyDown(document, { key: 'ArrowRight' });
    expect(title('未讲解的改动')).toBeTruthy();
    expect(screen.getByText(/1 处改动未讲解/)).toBeTruthy();

    fireEvent.keyDown(document, { key: 'Home' });
    expect(title('概览')).toBeTruthy();
  });

  it('点击文件树里的文件会跳到涉及它的步骤', async () => {
    render(<App load={loadOf({ kind: 'data', text: JSON.stringify(data), name: 'test', restore: false })} />);
    await screen.findByRole('heading', { level: 1, name: '概览' });
    fireEvent.click(screen.getByTitle('src/a.ts'));
    expect(title('修改 a')).toBeTruthy();
  });

  it('数据写错时显示欢迎页和原因', async () => {
    render(<App load={loadOf({ kind: 'data', text: '{', name: 'bad.json', restore: false })} />);
    expect(await screen.findByText(/bad\.json 不是合法的 JSON/)).toBeTruthy();
    expect(screen.getByRole('button', { name: '选择文件' })).toBeTruthy();
  });
});
