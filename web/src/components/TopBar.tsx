import type { Dispatch } from 'react';
import type { Model, Track } from '../core/model';
import { THEME_LABEL, type ThemePref } from '../hooks/useTheme';
import { SPEEDS, type Action, type PlayerState } from '../state/player';
import { HelpIcon, NextIcon, OpenIcon, PauseIcon, PlayIcon, PrevIcon, ThemeIcon } from './icons';
import { Scrubber } from './Scrubber';

interface Props {
  state: PlayerState;
  model: Model;
  track: Track;
  dispatch: Dispatch<Action>;
  themePref: ThemePref;
  onCycleTheme: () => void;
  onOpenFile: () => void;
  onHelp: () => void;
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
}: Props) {
  const total = track.items.length;
  const playLabel = state.playing ? '暂停' : '播放';
  const themeLabel = THEME_LABEL[themePref];
  return (
    <header className="topbar">
      <div className="brand">OneByOne</div>
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
      <button
        className="text-btn"
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
        <button
          className="icon-btn"
          type="button"
          title="打开 JSON 文件（O）"
          aria-label="打开 JSON 文件"
          onClick={onOpenFile}
        >
          <OpenIcon />
        </button>
        <button
          className="icon-btn"
          type="button"
          title={`主题：${themeLabel}（T）`}
          aria-label={`切换主题，当前：${themeLabel}`}
          onClick={onCycleTheme}
        >
          <ThemeIcon pref={themePref} />
        </button>
        <button className="icon-btn" type="button" title="快捷键（?）" aria-label="快捷键" onClick={onHelp}>
          <HelpIcon />
        </button>
      </div>
    </header>
  );
}
