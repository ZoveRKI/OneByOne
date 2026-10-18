import { Fragment } from 'react';
import type { Token } from '../lib/highlight';

/** 渲染一行高亮后的代码片段 */
export function Tokens({ tokens }: { tokens: Token[] }) {
  return tokens.map((t, i) =>
    t.cls ? (
      <span key={i} className={`tok-${t.cls}`}>
        {t.text}
      </span>
    ) : (
      <Fragment key={i}>{t.text}</Fragment>
    ),
  );
}
