import { describe, expect, it } from 'vitest';
import { normalize } from '../core/model';
import { initialState, readHash, reducer, type Action, type PlayerState } from './player';

const model = normalize({
  hunks: [
    { id: 'h1', file: 'src/a.ts', lines: ['+a'] },
    { id: 'h2', file: 'src/lib/b.ts', lines: ['+b'] },
    { id: 'h3', file: 'c.ts', lines: ['+c'] },
  ],
  steps: [
    { title: '一', hunks: ['h1'] },
    { title: '二', hunks: ['h2'] },
  ],
  timeline: [{ title: '甲', hunks: ['h2'] }, { title: '乙' }],
});

const run = (...actions: Action[]): PlayerState =>
  actions.reduce(reducer, reducer(initialState, { type: 'load', model, target: null }));

describe('reducer', () => {
  it('讲解顺序末尾自动补上「未讲解的改动」', () => {
    const s = run({ type: 'last' });
    expect(s.index).toBe(2);
    expect(model.tracks.steps.items[2]?.synthetic).toBe(true);
  });

  it('跳转会限制在范围内，并停止自动播放', () => {
    expect(run({ type: 'go', index: 99 }).index).toBe(2);
    expect(run({ type: 'step', delta: -5 }).index).toBe(0);
    const s = run({ type: 'play' }, { type: 'go', index: 1 });
    expect([s.index, s.playing]).toEqual([1, false]);
  });

  it('切换模式时分别记住两边的位置', () => {
    const s = run({ type: 'go', index: 1 }, { type: 'setMode', mode: 'timeline' }, { type: 'go', index: 1 });
    expect([s.mode, s.index]).toEqual(['timeline', 1]);
    const back = reducer(s, { type: 'toggleMode' });
    expect([back.mode, back.index]).toEqual(['steps', 1]);
  });

  it('在最后一步按播放会从头开始，播到最后自动停止', () => {
    let s = run({ type: 'last' }, { type: 'play' });
    expect([s.index, s.playing]).toEqual([0, true]);
    s = reducer(reducer(s, { type: 'advance' }), { type: 'advance' });
    expect([s.index, s.playing]).toEqual([2, true]);
    s = reducer(s, { type: 'advance' });
    expect([s.index, s.playing]).toEqual([2, false]);
  });

  it('「查看」未讲解的改动会切回讲解顺序的最后一步', () => {
    const s = run({ type: 'setMode', mode: 'timeline' }, { type: 'gotoUncovered' });
    expect([s.mode, s.index]).toEqual(['steps', 2]);
  });

  it('当前步骤的文件在折叠的目录里时自动展开', () => {
    const s = run({ type: 'toggleDir', path: 'src/lib' }, { type: 'toggleDir', path: 'src' });
    expect([...s.collapsed]).toEqual(['src/lib', 'src']);
    // 第二步涉及 src/lib/b.ts，两层目录都要展开
    const moved = run(
      { type: 'toggleDir', path: 'src/lib' },
      { type: 'toggleDir', path: 'src' },
      { type: 'go', index: 1 },
    );
    expect([...moved.collapsed]).toEqual([]);
  });

  it('地址栏位置可以恢复到对应模式和步骤', () => {
    expect(readHash('#timeline/2')).toEqual({ mode: 'timeline', step: 2 });
    expect(readHash('#other/2')).toBeNull();
    const s = reducer(initialState, { type: 'load', model, target: { mode: 'timeline', step: 9 } });
    expect([s.mode, s.index]).toEqual(['timeline', 1]);
  });
});
