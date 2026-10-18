/*
 * 播放器状态：当前模式和步骤、自动播放、速度、换行、折叠的目录。
 * 所有跳转都经过这个 reducer，键盘、按钮、进度条和定时器的行为保持一致。
 */
import type { Model, Step, Track, TrackName } from '../core/model';
import { clamp } from '../lib/format';

export const SPEEDS = [0.5, 1, 1.5, 2];

export interface PlayerState {
  model: Model | null;
  /** 每加载一次数据加 1，用来让步骤区重新播放进入动画 */
  serial: number;
  /** steps = 讲解顺序，timeline = 真实过程 */
  mode: TrackName;
  index: number;
  /** 切换模式时记住各自的位置 */
  saved: Record<TrackName, number>;
  playing: boolean;
  /** SPEEDS 的下标 */
  speed: number;
  wrap: boolean;
  /** 折叠起来的目录 */
  collapsed: ReadonlySet<string>;
}

/** 地址栏里的位置，例如 #steps/3 */
export interface HashTarget {
  mode: TrackName;
  step: number;
}

export type Action =
  | { type: 'load'; model: Model; target: HashTarget | null }
  | { type: 'go'; index: number }
  | { type: 'step'; delta: number }
  | { type: 'first' }
  | { type: 'last' }
  | { type: 'setMode'; mode: TrackName }
  | { type: 'toggleMode' }
  | { type: 'jump'; target: HashTarget }
  | { type: 'gotoUncovered' }
  | { type: 'play' }
  | { type: 'pause' }
  | { type: 'togglePlay' }
  | { type: 'advance' }
  | { type: 'cycleSpeed' }
  | { type: 'toggleWrap' }
  | { type: 'toggleDir'; path: string };

export const initialState: PlayerState = {
  model: null,
  serial: 0,
  mode: 'steps',
  index: 0,
  saved: { steps: 0, timeline: 0 },
  playing: false,
  speed: 1,
  wrap: false,
  collapsed: new Set(),
};

export function trackOf(model: Model, mode: TrackName): Track {
  return model.tracks[mode] ?? model.tracks.steps;
}

/** 当前步骤；只在已加载数据时调用 */
export function currentStep(state: PlayerState): Step {
  if (!state.model) throw new Error('还没有加载数据');
  const step = trackOf(state.model, state.mode).items[state.index];
  if (!step) throw new Error(`步骤下标越界：${state.index}`);
  return step;
}

/** 自动播放时每一步停留的时间：讲解越长、代码越多，停得越久 */
export function dwellMs(step: Step, speed: number): number {
  const lines = step.hunks.reduce((n, h) => n + h.lines.length, 0);
  return clamp(2000 + step.explain.length * 40 + lines * 90, 3000, 20000) / (SPEEDS[speed] ?? 1);
}

const lastIndex = (s: PlayerState) => (s.model ? trackOf(s.model, s.mode).items.length - 1 : 0);

// 当前步骤涉及的文件如果在折叠的目录里，自动展开
function reveal(s: PlayerState): PlayerState {
  if (!s.model || !s.collapsed.size) return s;
  const step = trackOf(s.model, s.mode).items[s.index];
  if (!step) return s;
  const keep = [...s.collapsed].filter((dir) => !step.hunks.some((h) => h.file.startsWith(`${dir}/`)));
  return keep.length === s.collapsed.size ? s : { ...s, collapsed: new Set(keep) };
}

// 手动跳转会停止自动播放
function go(s: PlayerState, index: number): PlayerState {
  if (!s.model) return s;
  const next = clamp(index, 0, lastIndex(s));
  const paused = s.playing ? { ...s, playing: false } : s;
  return next === s.index ? paused : reveal({ ...paused, index: next });
}

function setMode(s: PlayerState, mode: TrackName): PlayerState {
  if (!s.model || mode === s.mode || !s.model.tracks[mode]) return s;
  const saved = { ...s.saved, [s.mode]: s.index };
  const moved = { ...s, playing: false, saved, mode };
  return reveal({ ...moved, index: clamp(saved[mode], 0, lastIndex(moved)) });
}

export function reducer(s: PlayerState, action: Action): PlayerState {
  switch (action.type) {
    case 'load': {
      const { model, target } = action;
      const mode = target && model.tracks[target.mode] ? target.mode : 'steps';
      const count = trackOf(model, mode).items.length;
      const index = target ? clamp(target.step - 1, 0, count - 1) : 0;
      return {
        ...s,
        model,
        serial: s.serial + 1,
        mode,
        index,
        saved: { steps: 0, timeline: 0 },
        playing: false,
        collapsed: new Set(),
      };
    }
    case 'go':
      return go(s, action.index);
    case 'step':
      return go(s, s.index + action.delta);
    case 'first':
      return go(s, 0);
    case 'last':
      return go(s, lastIndex(s));
    case 'setMode':
      return setMode(s, action.mode);
    case 'toggleMode':
      return setMode(s, s.mode === 'steps' ? 'timeline' : 'steps');
    case 'jump': {
      if (!s.model || !s.model.tracks[action.target.mode]) return s;
      return go(setMode(s, action.target.mode), action.target.step - 1);
    }
    case 'gotoUncovered':
      return go(setMode(s, 'steps'), Number.MAX_SAFE_INTEGER);
    case 'play': {
      if (!s.model) return s;
      // 已经在最后一步时，从头开始播放
      const restart = s.index >= lastIndex(s) ? reveal({ ...s, index: 0 }) : s;
      return { ...restart, playing: true };
    }
    case 'pause':
      return s.playing ? { ...s, playing: false } : s;
    case 'togglePlay':
      return s.playing ? reducer(s, { type: 'pause' }) : reducer(s, { type: 'play' });
    case 'advance': {
      if (!s.playing || !s.model) return s;
      if (s.index >= lastIndex(s)) return { ...s, playing: false };
      return reveal({ ...s, index: s.index + 1 });
    }
    case 'cycleSpeed':
      return { ...s, speed: (s.speed + 1) % SPEEDS.length };
    case 'toggleWrap':
      return { ...s, wrap: !s.wrap };
    case 'toggleDir': {
      const collapsed = new Set(s.collapsed);
      if (collapsed.has(action.path)) collapsed.delete(action.path);
      else collapsed.add(action.path);
      return { ...s, collapsed };
    }
  }
}

export function readHash(hash: string): HashTarget | null {
  const m = /^#(steps|timeline)\/(\d+)$/.exec(hash);
  return m ? { mode: m[1] as TrackName, step: Number(m[2]) } : null;
}
