import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import type { Step } from '../core/model';
import { clamp } from '../lib/format';

// 进度条圆点的宽度（px），和 styles.css 里的滑块样式保持一致
const THUMB = 18;

interface Props {
  items: Step[];
  index: number;
  onGo: (index: number) => void;
}

/** 进度条：原生滑块负责拖动、点击和键盘；悬停或拖动时在下方提示对应的步骤 */
export function Scrubber({ items, index, onGo }: Props) {
  const input = useRef<HTMLInputElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const [tipIndex, setTipIndex] = useState<number | null>(null);
  const n = items.length;
  const current = items[index];
  const tipStep = tipIndex === null ? undefined : items[tipIndex];

  // 指针位置换算成步骤序号：圆点中心的活动范围是两端各缩进半个圆点
  const stepAt = (clientX: number) => {
    const r = input.current?.getBoundingClientRect();
    if (!r) return index;
    const travel = r.width - THUMB;
    const ratio = travel > 0 ? clamp((clientX - r.left - THUMB / 2) / travel, 0, 1) : 0;
    return Math.round(ratio * (n - 1));
  };

  // 提示框跟着对应的圆点位置，但不超出进度条两端
  useLayoutEffect(() => {
    const tip = tipRef.current;
    const r = input.current?.getBoundingClientRect();
    if (tipIndex === null || !tip || !r) return;
    const x = THUMB / 2 + (n > 1 ? tipIndex / (n - 1) : 0) * (r.width - THUMB);
    const half = tip.offsetWidth / 2;
    tip.style.left = `${clamp(x, half, Math.max(half, r.width - half))}px`;
  }, [tipIndex, n]);

  useEffect(() => {
    const onUp = () => {
      if (!dragging.current) return;
      dragging.current = false;
      setTipIndex(null);
    };
    window.addEventListener('pointerup', onUp);
    return () => window.removeEventListener('pointerup', onUp);
  }, []);

  const style = { '--pct': `${n > 1 ? (index / (n - 1)) * 100 : 0}%` } as CSSProperties;

  return (
    <div className="scrubber">
      <input
        ref={input}
        className="scrubber-input"
        type="range"
        min={1}
        max={n}
        step={1}
        value={index + 1}
        style={style}
        aria-label="步骤进度"
        aria-valuetext={current ? `第 ${index + 1} 步：${current.title}` : undefined}
        onChange={(e) => {
          const next = clamp(Number(e.target.value) - 1, 0, n - 1);
          onGo(next);
          if (dragging.current) setTipIndex(next);
        }}
        onPointerDown={() => {
          dragging.current = true;
        }}
        onPointerMove={(e) => setTipIndex(dragging.current ? index : stepAt(e.clientX))}
        onPointerLeave={() => {
          if (!dragging.current) setTipIndex(null);
        }}
      />
      {tipStep && tipIndex !== null && (
        <div ref={tipRef} className="scrubber-tip">
          第 {tipIndex + 1} 步{tipStep.chapter ? ` · ${tipStep.chapter}` : ''} · {tipStep.title}
        </div>
      )}
    </div>
  );
}
