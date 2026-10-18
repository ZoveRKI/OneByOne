/*
 * 在 MCP 客户端的对话里显示（MCP Apps）：客户端把播放器放进 iframe，
 * 讲解数据随 show_walkthrough 的工具结果送来，放在 structuredContent.walkthrough 里。
 * 这里负责和客户端握手、接收工具结果，并跟随客户端的主题、显示模式和容器尺寸。
 */
import {
  App,
  type McpUiDisplayMode,
  type McpUiHostContext,
  type McpUiToolResultNotification,
} from '@modelcontextprotocol/ext-apps';
import { version } from '../../package.json';
import type { Initial } from './source';

/** 在对话里内嵌显示时播放器的高度；客户端限制的最大高度更小时，用最大高度 */
export const INLINE_HEIGHT = 640;
// 握手超过这个时间还没完成，就先当作不在 MCP 客户端里，照常读取数据；之后握手成功也照样接收工具结果
const HANDSHAKE_MS = 1500;

export interface HostView {
  theme?: 'light' | 'dark';
  displayMode: McpUiDisplayMode;
  /** 客户端支持全屏显示 */
  canFullscreen: boolean;
}

export interface Bridge {
  /** 握手的结果：true 表示确实在 MCP 客户端里 */
  ready: Promise<boolean>;
  /** 订阅工具结果；订阅之前已经收到的结果会马上补发一次 */
  onResult(listener: (initial: Initial) => void): () => void;
  /** 客户端的主题和显示模式；握手完成之前是 null */
  getView(): HostView | null;
  /** 订阅 getView() 的变化 */
  subscribe(listener: () => void): () => void;
  toggleFullscreen(): void;
}

type ToolResult = McpUiToolResultNotification['params'];

/** 把 show_walkthrough 的结果换成播放器的数据；出错时显示工具返回的说明 */
export function resultToInitial(result: ToolResult): Initial {
  const data: unknown = result.structuredContent;
  const walkthrough = isRecord(data) ? data.walkthrough : undefined;
  if (!result.isError && isRecord(walkthrough)) {
    return { kind: 'data', text: JSON.stringify(walkthrough), name: '讲解数据', restore: false };
  }
  const text = result.content?.find((block) => block.type === 'text')?.text;
  return { kind: 'landing', message: text || '没有收到讲解数据。' };
}

/**
 * 客户端按页面高度调整 iframe，所以页面不能用 100vh 撑满自己。
 * 客户端给了固定高度（或者处于全屏）时铺满，否则用固定的像素高度
 */
export function applyLayout(
  mode: McpUiDisplayMode,
  dims: McpUiHostContext['containerDimensions'],
  root: HTMLElement = document.documentElement,
) {
  const fixed = mode === 'fullscreen' || (dims !== undefined && 'height' in dims);
  const max = dims && 'maxHeight' in dims ? dims.maxHeight : undefined;
  const height = fixed ? '100vh' : `${Math.min(INLINE_HEIGHT, max && max > 0 ? max : INLINE_HEIGHT)}px`;
  root.classList.add('embedded');
  root.style.setProperty('--app-height', height);
}

/** 开始和客户端握手；页面不在 MCP 客户端里时，ready 会在超时后得到 false */
export function connectBridge(): Bridge {
  const app = new App({ name: 'OneByOne', version }, { availableDisplayModes: ['inline', 'fullscreen'] });
  const resultListeners = new Set<(initial: Initial) => void>();
  const viewListeners = new Set<() => void>();
  let last: Initial | null = null;
  let view: HostView | null = null;
  let mode: McpUiDisplayMode = 'inline';

  const emit = (initial: Initial) => {
    last = initial;
    for (const listener of resultListeners) listener(initial);
  };
  const update = () => {
    const context = app.getHostContext();
    applyLayout(mode, context?.containerDimensions);
    view = {
      theme: context?.theme,
      displayMode: mode,
      canFullscreen: Boolean(context?.availableDisplayModes?.includes('fullscreen')),
    };
    for (const listener of viewListeners) listener();
  };

  // 监听要在握手之前注册，握手一完成客户端就可能送来结果
  app.addEventListener('toolresult', (result) => emit(resultToInitial(result)));
  app.addEventListener('toolcancelled', ({ reason }) =>
    emit({ kind: 'landing', message: reason ? `工具调用已取消：${reason}` : '工具调用已取消。' }),
  );
  app.addEventListener('hostcontextchanged', (changed) => {
    if (changed.displayMode) mode = changed.displayMode;
    update();
  });
  app.onteardown = () => ({});

  const connected = app.connect().then(
    () => {
      mode = app.getHostContext()?.displayMode ?? 'inline';
      update();
      openLinksThroughHost(app);
      return true;
    },
    () => false,
  );
  const timeout = new Promise<boolean>((resolve) => setTimeout(() => resolve(false), HANDSHAKE_MS));

  return {
    ready: Promise.race([connected, timeout]),
    onResult(listener) {
      resultListeners.add(listener);
      if (last) listener(last);
      return () => {
        resultListeners.delete(listener);
      };
    },
    getView: () => view,
    subscribe(listener) {
      viewListeners.add(listener);
      return () => {
        viewListeners.delete(listener);
      };
    },
    toggleFullscreen() {
      app
        .requestDisplayMode({ mode: mode === 'fullscreen' ? 'inline' : 'fullscreen' })
        .then((result) => {
          mode = result.mode;
          update();
        })
        .catch(() => {
          // 客户端拒绝切换时保持原样
        });
    },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

// 页面在沙箱 iframe 里，直接打开链接可能被拦下，统一交给客户端打开
function openLinksThroughHost(app: App) {
  document.addEventListener('click', (e) => {
    const link = e.target instanceof Element ? e.target.closest('a[href]') : null;
    // 只处理写明 http(s):// 的链接；页面内的锚点和相对地址照常处理
    if (!(link instanceof HTMLAnchorElement) || !/^https?:\/\//i.test(link.getAttribute('href') ?? '')) {
      return;
    }
    e.preventDefault();
    app.openLink({ url: link.href }).catch(() => {
      // 打不开时没有别的办法，忽略即可
    });
  });
}
