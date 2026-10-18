import type { Ref } from 'react';
import { CloseIcon } from './icons';

interface LandingProps {
  message: string;
  error: boolean;
  onOpen: () => void;
  /** 在 MCP 客户端的对话里：数据由客户端送来，只显示提示，不显示选择文件 */
  embedded?: boolean;
}

export function Landing({ message, error, onOpen, embedded = false }: LandingProps) {
  return (
    <div className="landing">
      <div className="landing-card">
        <div className="landing-logo">OneByOne</div>
        <p className="landing-sub">一步一步看懂 AI 的代码改动</p>
        {!embedded && (
          <div className="dropzone">
            <p>
              把 <code>walkthrough.json</code> 拖到这里
            </p>
            <button className="btn primary" type="button" onClick={onOpen}>
              选择文件
            </button>
          </div>
        )}
        {message && <p className={`landing-msg${error ? ' error' : ''}`}>{message}</p>}
        {!embedded && (
          <p className="landing-hint">
            用命令行 <code>onebyone open</code> 打开时，页面会自动带上讲解数据。
          </p>
        )}
      </div>
    </div>
  );
}

export function DropOverlay() {
  return (
    <div className="drop-overlay">
      <div>松开鼠标，加载这个 JSON 文件</div>
    </div>
  );
}

const KEYS: [string[], string][] = [
  [['←', '→'], '上一步 / 下一步'],
  [['空格'], '播放 / 暂停'],
  [['Home', 'End'], '第一步 / 最后一步'],
  [['M'], '切换讲解顺序 / 真实过程'],
  [['W'], '代码自动换行'],
  [['O'], '打开 JSON 文件'],
  [['T'], '切换主题'],
  [['?'], '显示本帮助'],
];

export function HelpDialog({ ref }: { ref: Ref<HTMLDialogElement> }) {
  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby="help-title"
      onClick={(e) => {
        // 点对话框外的遮罩或关闭按钮时关闭
        const dialog = e.currentTarget;
        if (e.target === dialog || (e.target instanceof Element && e.target.closest('[data-close]')))
          dialog.close();
      }}
    >
      <div className="dialog-head">
        <h2 id="help-title">快捷键</h2>
        <button className="icon-btn" type="button" data-close aria-label="关闭">
          <CloseIcon />
        </button>
      </div>
      <table className="keys">
        <tbody>
          {KEYS.map(([keys, label]) => (
            <tr key={label}>
              <td>
                {keys.map((k, i) => (
                  <span key={k}>
                    {i > 0 && ' '}
                    <kbd>{k}</kbd>
                  </span>
                ))}
              </td>
              <td>{label}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="dialog-note">
        也可以直接把 JSON 文件拖进页面。地址栏里的 <code>#steps/3</code> 记录当前位置，刷新后会回到同一步。
      </p>
    </dialog>
  );
}
