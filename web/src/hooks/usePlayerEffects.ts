import { useEffect, type Dispatch } from 'react';
import { dwellMs, readHash, trackOf, type Action, type PlayerState } from '../state/player';

/** 自动播放：按当前步骤的停留时间定时前进，切换步骤或速度后重新计时 */
export function useAutoplay(state: PlayerState, dispatch: Dispatch<Action>) {
  const { model, mode, index, speed, playing } = state;
  useEffect(() => {
    const step = model ? trackOf(model, mode).items[index] : undefined;
    if (!playing || !step) return;
    const timer = window.setTimeout(() => dispatch({ type: 'advance' }), dwellMs(step, speed));
    return () => window.clearTimeout(timer);
  }, [playing, model, mode, index, speed, dispatch]);
}

/** 地址栏的 #steps/3 记录当前位置；手动修改地址栏时跳到对应步骤 */
export function useHashSync(state: PlayerState, dispatch: Dispatch<Action>) {
  const { model, mode, index } = state;
  useEffect(() => {
    if (!model) return;
    const hash = `#${mode}/${index + 1}`;
    try {
      if (location.hash !== hash) history.replaceState(null, '', hash);
    } catch {
      // 嵌在沙箱 iframe 里时可能不允许改地址，忽略即可
    }
  }, [model, mode, index]);

  useEffect(() => {
    const onHashChange = () => {
      const target = readHash(location.hash);
      if (target) dispatch({ type: 'jump', target });
    };
    window.addEventListener('hashchange', onHashChange);
    return () => window.removeEventListener('hashchange', onHashChange);
  }, [dispatch]);
}
