import { useCallback, useEffect, useState } from 'react';

export const THEMES = ['auto', 'light', 'dark'] as const;
export type ThemePref = (typeof THEMES)[number];
export const THEME_LABEL: Record<ThemePref, string> = { auto: '跟随系统', light: '浅色', dark: '深色' };
// 和 index.html 里提前设置主题的脚本使用同一个键
const THEME_KEY = 'onebyone-theme';

function readPref(): ThemePref {
  try {
    const value = localStorage.getItem(THEME_KEY);
    return THEMES.find((t) => t === value) ?? 'auto';
  } catch {
    // 隐私模式等情况下读不到，按跟随系统处理
    return 'auto';
  }
}

/** 主题偏好：跟随系统 / 浅色 / 深色，保存在 localStorage */
export function useTheme() {
  const [pref, setPref] = useState<ThemePref>(readPref);

  useEffect(() => {
    const query =
      typeof window.matchMedia === 'function' ? window.matchMedia('(prefers-color-scheme: dark)') : null;
    const apply = () => {
      const dark = pref === 'dark' || (pref === 'auto' && Boolean(query?.matches));
      document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light');
    };
    apply();
    try {
      if (pref === 'auto') localStorage.removeItem(THEME_KEY);
      else localStorage.setItem(THEME_KEY, pref);
    } catch {
      // 存不下也不影响本次使用
    }
    if (pref !== 'auto' || !query) return;
    query.addEventListener('change', apply);
    return () => query.removeEventListener('change', apply);
  }, [pref]);

  const cycle = useCallback(() => {
    setPref((p) => THEMES[(THEMES.indexOf(p) + 1) % THEMES.length] ?? 'auto');
  }, []);

  return { pref, cycle };
}
