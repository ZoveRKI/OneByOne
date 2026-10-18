import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { connectBridge } from './data/mcp';
import { readEmbedded } from './data/source';
import './styles.css';

// 嵌在 iframe 里、又没有内嵌数据时，可能是在 MCP 客户端的对话里：尽早开始握手，免得错过客户端送来的结果
const bridge = window.parent !== window && !readEmbedded() ? connectBridge() : null;

const root = document.getElementById('root');
if (!root) throw new Error('页面里缺少 #root 元素');
createRoot(root).render(
  <StrictMode>
    <App bridge={bridge} />
  </StrictMode>,
);
