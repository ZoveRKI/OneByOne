import { afterEach, describe, expect, it, vi } from 'vitest';
import { applyLayout, connectBridge, INLINE_HEIGHT, resultToInitial } from './mcp';
import type { Initial } from './source';

// 用假的 App 代替 ext-apps：记下注册的监听，测试里手动模拟客户端发来的通知
const fake = vi.hoisted(() => {
  class FakeApp {
    static last: FakeApp | null = null;
    // 下一个实例握手的结果，默认成功
    static connectWith: () => Promise<void> = () => Promise.resolve();
    listeners = new Map<string, (params: never) => void>();
    context: Record<string, unknown> = {
      theme: 'dark',
      displayMode: 'inline',
      availableDisplayModes: ['inline', 'fullscreen'],
      containerDimensions: { maxHeight: 500 },
    };
    connect = vi.fn<() => Promise<void>>(FakeApp.connectWith);
    requestDisplayMode = vi.fn<(params: { mode: string }) => Promise<{ mode: string }>>(({ mode }) =>
      Promise.resolve({ mode }),
    );
    openLink = vi.fn<(params: { url: string }) => Promise<object>>(() => Promise.resolve({}));
    onteardown?: () => unknown;
    constructor() {
      FakeApp.last = this;
    }
    addEventListener(name: string, listener: (params: never) => void) {
      this.listeners.set(name, listener);
    }
    getHostContext() {
      return this.context;
    }
    emit(name: string, params: unknown) {
      this.listeners.get(name)?.(params as never);
    }
  }
  return { FakeApp };
});

vi.mock('@modelcontextprotocol/ext-apps', () => ({ App: fake.FakeApp }));

const app = () => {
  const last = fake.FakeApp.last;
  if (!last) throw new Error('还没有创建 App');
  return last;
};
const walkthrough = { title: 't', hunks: [], steps: [{ title: '概览' }] };
const click = (node: Element | undefined) => {
  const event = new MouseEvent('click', { bubbles: true, cancelable: true });
  node?.dispatchEvent(event);
  return event;
};

afterEach(() => {
  document.documentElement.className = '';
  document.documentElement.removeAttribute('style');
  document.body.innerHTML = '';
  vi.useRealTimers();
  fake.FakeApp.connectWith = () => Promise.resolve();
});

describe('resultToInitial', () => {
  it('讲解数据在 structuredContent.walkthrough 里', () => {
    expect(resultToInitial({ content: [], structuredContent: { report: '', walkthrough } })).toEqual({
      kind: 'data',
      text: JSON.stringify(walkthrough),
      name: '讲解数据',
      restore: false,
    });
  });

  it('出错或没有数据时显示工具返回的文字', () => {
    const content = [{ type: 'text' as const, text: '错误：请先调用 get_changes' }];
    expect(resultToInitial({ content, isError: true })).toEqual({
      kind: 'landing',
      message: '错误：请先调用 get_changes',
    });
    expect(resultToInitial({ content: [] })).toEqual({ kind: 'landing', message: '没有收到讲解数据。' });
  });
});

describe('applyLayout', () => {
  it('客户端只限制最大高度时用固定高度，给了高度或全屏时铺满', () => {
    const root = document.createElement('div');
    applyLayout('inline', undefined, root);
    expect(root.classList.contains('embedded')).toBe(true);
    expect(root.style.getPropertyValue('--app-height')).toBe(`${INLINE_HEIGHT}px`);
    applyLayout('inline', { maxHeight: 400 }, root);
    expect(root.style.getPropertyValue('--app-height')).toBe('400px');
    applyLayout('inline', { height: 300 }, root);
    expect(root.style.getPropertyValue('--app-height')).toBe('100vh');
    applyLayout('fullscreen', { maxHeight: 400 }, root);
    expect(root.style.getPropertyValue('--app-height')).toBe('100vh');
  });
});

describe('connectBridge', () => {
  it('握手成功后跟随客户端的主题和尺寸', async () => {
    const bridge = connectBridge();
    expect(await bridge.ready).toBe(true);
    expect(bridge.getView()).toEqual({ theme: 'dark', displayMode: 'inline', canFullscreen: true });
    expect(document.documentElement.style.getPropertyValue('--app-height')).toBe('500px');
    // 客户端卸载界面前会发 teardown 请求，要能正常回应
    expect(app().onteardown?.()).toEqual({});

    const changed = vi.fn<() => void>();
    bridge.subscribe(changed);
    app().context = { ...app().context, theme: 'light' };
    app().emit('hostcontextchanged', { theme: 'light' });
    expect(changed).toHaveBeenCalled();
    expect(bridge.getView()?.theme).toBe('light');
  });

  it('工具结果交给订阅者，之后才订阅的也能收到', async () => {
    const bridge = connectBridge();
    await bridge.ready;
    const first = vi.fn<(initial: Initial) => void>();
    bridge.onResult(first);
    app().emit('toolresult', { content: [], structuredContent: { walkthrough } });
    expect(first).toHaveBeenCalledWith(expect.objectContaining({ kind: 'data' }));

    const late = vi.fn<(initial: Initial) => void>();
    bridge.onResult(late);
    expect(late).toHaveBeenCalledWith(expect.objectContaining({ kind: 'data' }));

    app().emit('toolcancelled', { reason: '用户取消' });
    expect(late).toHaveBeenLastCalledWith({ kind: 'landing', message: '工具调用已取消：用户取消' });
  });

  it('切换全屏后改为铺满', async () => {
    const bridge = connectBridge();
    await bridge.ready;
    bridge.toggleFullscreen();
    expect(app().requestDisplayMode).toHaveBeenCalledWith({ mode: 'fullscreen' });
    await vi.waitFor(() => expect(bridge.getView()?.displayMode).toBe('fullscreen'));
    expect(document.documentElement.style.getPropertyValue('--app-height')).toBe('100vh');

    bridge.toggleFullscreen();
    expect(app().requestDisplayMode).toHaveBeenLastCalledWith({ mode: 'inline' });
  });

  it('链接交给客户端打开', async () => {
    const bridge = connectBridge();
    await bridge.ready;
    document.body.innerHTML = '<p><a href="https://example.com/doc">文档</a><a href="#x">锚点</a></p>';
    const [external, anchor] = document.querySelectorAll('a');
    expect(click(external).defaultPrevented).toBe(true);
    expect(app().openLink).toHaveBeenCalledWith({ url: 'https://example.com/doc' });
    // 页面内的锚点照常处理
    expect(click(anchor).defaultPrevented).toBe(false);
  });

  it('不在 MCP 客户端里时，超时后 ready 为 false', async () => {
    vi.useFakeTimers();
    fake.FakeApp.connectWith = () => new Promise(() => {});
    const bridge = connectBridge();
    await vi.advanceTimersByTimeAsync(2000);
    expect(await bridge.ready).toBe(false);
    expect(bridge.getView()).toBeNull();
  });

  it('握手失败时 ready 为 false，也不改页面布局', async () => {
    fake.FakeApp.connectWith = () => Promise.reject(new Error('没有客户端'));
    const bridge = connectBridge();
    expect(await bridge.ready).toBe(false);
    expect(document.documentElement.classList.contains('embedded')).toBe(false);
  });
});
