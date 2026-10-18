import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { App } from './App';
import type { Bridge, HostView } from './data/mcp';
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

/** 模拟已经握手成功的 MCP 客户端，send 相当于客户端送来工具结果 */
function fakeBridge(view: HostView) {
  let listener: ((initial: Initial) => void) | null = null;
  const toggleFullscreen = vi.fn<() => void>();
  const bridge: Bridge = {
    ready: Promise.resolve(true),
    onResult(next) {
      listener = next;
      return () => {
        listener = null;
      };
    },
    getView: () => view,
    subscribe: () => () => {},
    toggleFullscreen,
  };
  return { bridge, toggleFullscreen, send: (initial: Initial) => act(() => listener?.(initial)) };
}

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

describe('App 在 MCP 客户端的对话里', () => {
  it('等工具结果送来数据，跟随客户端的主题，可以全屏', async () => {
    const { bridge, send, toggleFullscreen } = fakeBridge({
      theme: 'dark',
      displayMode: 'inline',
      canFullscreen: true,
    });
    render(<App load={loadOf({ kind: 'host' })} bridge={bridge} />);
    expect(await screen.findByText('正在等待讲解数据……')).toBeTruthy();
    // 数据由客户端送来，不提示选择文件
    expect(screen.queryByRole('button', { name: '选择文件' })).toBeNull();

    send({ kind: 'data', text: JSON.stringify(data), name: '讲解数据', restore: false });
    expect(title('概览')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '打开 JSON 文件' })).toBeNull();
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    fireEvent.click(screen.getByRole('button', { name: '全屏显示' }));
    expect(toggleFullscreen).toHaveBeenCalled();
  });

  it('工具出错时显示返回的说明', async () => {
    const { bridge, send } = fakeBridge({ displayMode: 'inline', canFullscreen: false });
    render(<App load={loadOf({ kind: 'host' })} bridge={bridge} />);
    await screen.findByText('正在等待讲解数据……');
    send({ kind: 'landing', message: '错误：请先调用 get_changes' });
    expect(screen.getByText('错误：请先调用 get_changes')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '全屏显示' })).toBeNull();
  });
});
