import type { Dispatch } from 'react';
import type { Model, Track } from '../core/model';
import { THEME_LABEL, type ThemePref } from '../hooks/useTheme';
import { SPEEDS, type Action, type PlayerState } from '../state/player';
import {
  CheckIcon,
  ExitFullscreenIcon,
  FullscreenIcon,
  HelpIcon,
  NextIcon,
  OpenIcon,
  PauseIcon,
  PlayIcon,
  PrevIcon,
  ThemeIcon,
  WarnIcon,
} from './icons';
import { Scrubber } from './Scrubber';

interface Props {
  state: PlayerState;
  model: Model;
  track: Track;
  dispatch: Dispatch<Action>;
  themePref: ThemePref;
  onCycleTheme: () => void;
  /** 在 MCP 客户端的对话里时不提供，不显示打开文件的按钮 */
  onOpenFile?: () => void;
  onHelp: () => void;
  /** MCP 客户端支持全屏时才提供 */
  fullscreen?: { active: boolean; onToggle: () => void };
  /** 紧凑布局（在对话里内嵌显示）：没有侧栏，覆盖情况改为显示在这里 */
  compact?: boolean;
}

const MODES = [
  { mode: 'steps', label: '讲解顺序' },
  { mode: 'timeline', label: '真实过程' },
] as const;

export function TopBar({
  state,
  model,
  track,
  dispatch,
  themePref,
  onCycleTheme,
  onOpenFile,
  onHelp,
  fullscreen,
  compact = false,
}: Props) {
  const total = track.items.length;
  const playLabel = state.playing ? '暂停' : '播放';
  const themeLabel = THEME_LABEL[themePref];
  return (
    <header className="topbar">
      {!compact && <div className="brand">OneByOne</div>}
      <div className="transport">
        <button
          className="icon-btn"
          type="button"
          title="上一步（←）"
          aria-label="上一步"
          disabled={state.index === 0}
          onClick={() => dispatch({ type: 'step', delta: -1 })}
        >
          <PrevIcon />
        </button>
        <button
          className="icon-btn play"
          type="button"
          title={`${playLabel}（空格）`}
          aria-label={playLabel}
          onClick={() => dispatch({ type: 'togglePlay' })}
        >
          {state.playing ? <PauseIcon /> : <PlayIcon />}
        </button>
        <button
          className="icon-btn"
          type="button"
          title="下一步（→）"
          aria-label="下一步"
          disabled={state.index === total - 1}
          onClick={() => dispatch({ type: 'step', delta: 1 })}
        >
          <NextIcon />
        </button>
      </div>
      <Scrubber items={track.items} index={state.index} onGo={(index) => dispatch({ type: 'go', index })} />
      <div className="counter">
        {state.index + 1} / {total}
      </div>
      {compact && <CoverageBadge model={model} onGotoUncovered={() => dispatch({ type: 'gotoUncovered' })} />}
      <button
        className="text-btn speed"
        type="button"
        title="自动播放速度"
        onClick={() => dispatch({ type: 'cycleSpeed' })}
      >
        {SPEEDS[state.speed]}×
      </button>
      {model.tracks.timeline && (
        <div className="segmented" role="group" aria-label="步骤顺序">
          {MODES.map(({ mode, label }) => (
            <button
              key={mode}
              type="button"
              aria-pressed={state.mode === mode}
              title="切换顺序（M）"
              onClick={() => dispatch({ type: 'setMode', mode })}
            >
              {label}
            </button>
          ))}
        </div>
      )}
      <div className="tools">
        {onOpenFile && (
          <button
            className="icon-btn"
            type="button"
            title="打开 JSON 文件（O）"
            aria-label="打开 JSON 文件"
            onClick={onOpenFile}
          >
            <OpenIcon />
          </button>
        )}
        {fullscreen && (
          <button
            className="icon-btn"
            type="button"
            title={fullscreen.active ? '退出全屏' : '全屏显示'}
            aria-label={fullscreen.active ? '退出全屏' : '全屏显示'}
            onClick={fullscreen.onToggle}
          >
            {fullscreen.active ? <ExitFullscreenIcon /> : <FullscreenIcon />}
          </button>
        )}
        <button
          className="icon-btn"
          type="button"
          title={`主题：${themeLabel}（T）`}
          aria-label={`切换主题，当前：${themeLabel}`}
          onClick={onCycleTheme}
        >
          <ThemeIcon pref={themePref} />
        </button>
        <button
          className="icon-btn help-btn"
          type="button"
          title="快捷键（?）"
          aria-label="快捷键"
          onClick={onHelp}
        >
          <HelpIcon />
        </button>
      </div>
    </header>
  );
}

/** 紧凑布局里的覆盖情况：讲全了只显示对勾；有没讲到的改动时显示数量，点击跳到「未讲解的改动」 */
function CoverageBadge({ model, onGotoUncovered }: { model: Model; onGotoUncovered: () => void }) {
  const { coverage, issues } = model;
  const issueNote = issues.length ? `\n另有 ${issues.length} 条数据问题，全屏后在左下角查看` : '';
  if (coverage.uncovered.length) {
    return (
      <button
        className="cov-badge warn"
        type="button"
        title={`${coverage.uncovered.length} 处改动未讲解（共 ${coverage.total} 处），点击查看${issueNote}`}
        onClick={onGotoUncovered}
      >
        <WarnIcon />
        <span>{coverage.uncovered.length} 处未讲解</span>
      </button>
    );
  }
  if (issues.length) {
    return (
      <span className="cov-badge warn" title={issues.join('\n')}>
        <WarnIcon />
        <span>{issues.length} 条数据问题</span>
      </span>
    );
  }
  if (!coverage.total) return null;
  const label = `讲解覆盖了全部 ${coverage.total} 处改动`;
  return (
    <span className="cov-badge ok" role="img" title={label} aria-label={label}>
      <CheckIcon />
    </span>
  );
}
