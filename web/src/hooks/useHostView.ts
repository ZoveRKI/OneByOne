import { useSyncExternalStore } from 'react';
import type { Bridge, HostView } from '../data/mcp';

const noSubscribe = () => () => {};
const noView = () => null;

/** MCP 客户端的主题和显示模式；不在客户端里，或者还没握手完成时是 null */
export function useHostView(bridge: Bridge | null): HostView | null {
  return useSyncExternalStore(bridge?.subscribe ?? noSubscribe, bridge?.getView ?? noView);
}
