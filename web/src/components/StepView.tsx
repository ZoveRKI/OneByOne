import { useMemo, type Ref } from 'react';
import { STATUS, type FileInfo, type Hunk, type Step } from '../core/model';
import { baseName, dirName, groupByFile, hunkLabel, sumOf, type FileGroup } from '../lib/format';
import { highlightLine, langOfPath, type HighlightState, type Lang } from '../lib/highlight';
import { Markdown } from '../lib/markdown';
import { kindOf } from './kinds';
import { Stat } from './Stat';
import { Tokens } from './Tokens';

// 步骤区的三块内容。父组件按步骤给它们换 key，每一步都会重新播放进入动画

export function StepHead({
  step,
  index,
  onJump,
}: {
  step: Step;
  index: number;
  onJump: (file: string) => void;
}) {
  const kind = kindOf(step.kind);
  const groups = groupByFile(step.hunks);
  return (
    <div className="step-head-main enter">
      <div className="step-meta">
        <span className="step-no">第 {index + 1} 步</span>
        {step.chapter && (
          <>
            <span className="sep">·</span>
            <span>{step.chapter}</span>
          </>
        )}
        {step.time && (
          <>
            <span className="sep">·</span>
            <span className="mono">{step.time}</span>
          </>
        )}
        {kind && <span className={`badge tone-${kind.tone}`}>{kind.label}</span>}
      </div>
      <h1 className="step-title">{step.title}</h1>
      {groups.length > 0 && (
        <div className="step-files">
          {groups.map((g) => (
            <button
              key={g.file}
              className="file-chip"
              type="button"
              title={g.file}
              onClick={() => onJump(g.file)}
            >
              {baseName(g.file)}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

interface DiffProps {
  step: Step;
  files: Map<string, FileInfo>;
  wrap: boolean;
  ref?: Ref<HTMLDivElement>;
}

export function DiffView({ step, files, wrap, ref }: DiffProps) {
  return (
    <div ref={ref} className={`diff enter${wrap ? ' wrap' : ''}`}>
      {step.hunks.length ? (
        groupByFile(step.hunks).map((g) => <FileDiff key={g.file} group={g} file={files.get(g.file)} />)
      ) : (
        <div className="empty">这一步没有代码改动，只有讲解。</div>
      )}
    </div>
  );
}

function FileDiff({ group, file }: { group: FileGroup; file: FileInfo | undefined }) {
  const status = STATUS[file?.status ?? 'modified'];
  const lang = langOfPath(group.file);
  const oldPath = file?.oldPath && file.oldPath !== group.file ? file.oldPath : '';
  return (
    <section className="diff-file" data-diff-file={group.file}>
      <header className="diff-file-head">
        <span className={`st st-${file?.status ?? 'modified'}`} title={status.label}>
          {status.short}
        </span>
        <span className="diff-path" title={group.file}>
          {oldPath && <span className="dir">{oldPath} → </span>}
          <span className="dir">{dirName(group.file)}</span>
          {baseName(group.file)}
        </span>
        <span className="diff-stat">
          <Stat adds={sumOf(group.hunks, 'adds')} dels={sumOf(group.hunks, 'dels')} />
        </span>
      </header>
      {group.hunks.map((h) => (
        <HunkView key={h.id} hunk={h} lang={lang} />
      ))}
    </section>
  );
}

function HunkView({ hunk, lang }: { hunk: Hunk; lang: Lang | null }) {
  // 同一个改动块里逐行高亮，跨行的块注释才能接上
  const tokens = useMemo(() => {
    const st: HighlightState = { inBlock: false };
    return hunk.lines.map((line) => (line.type === 'meta' ? [] : highlightLine(line.text, lang, st)));
  }, [hunk, lang]);
  return (
    <div className="hunk">
      <div className="hunk-head">
        <span>{hunkLabel(hunk)}</span>
        {!hunk.final && (
          <span className="hunk-tag" title="这块改动只出现在过程中，不在最终 diff 里">
            过程中的改动
          </span>
        )}
      </div>
      <div className="code-wrap">
        <table className="code">
          <tbody>
            {hunk.lines.map((line, i) => {
              if (line.type === 'meta') {
                // 去掉行首表示说明行的反斜杠，例如 "\ No newline at end of file"
                return (
                  <tr key={i} className="meta">
                    <td className="no" />
                    <td className="no" />
                    <td className="sign" />
                    <td className="src">{line.text.replace(/^\\ ?/, '')}</td>
                  </tr>
                );
              }
              const lineTokens = tokens[i] ?? [];
              return (
                <tr key={i} className={line.type}>
                  <td className="no">{line.oldNo ?? ''}</td>
                  <td className="no">{line.newNo ?? ''}</td>
                  <td className="sign">{line.type === 'add' ? '+' : line.type === 'del' ? '-' : ' '}</td>
                  <td className="src">{lineTokens.length ? <Tokens tokens={lineTokens} /> : ' '}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export function Explain({ step, items, onGo }: { step: Step; items: Step[]; onGo: (index: number) => void }) {
  return (
    <aside className="explain enter" aria-label="讲解">
      {step.synthetic ? (
        <>
          <div className="explain-label">自动检查</div>
          <div className="callout warn">
            以下 {step.hunks.length} 处改动没有出现在任何讲解步骤里。可能是讲解遗漏，也可能是 AI
            顺手做的无关修改，建议重点检查。
          </div>
          <ul className="uncovered-list">
            {groupByFile(step.hunks).map((g) => (
              <li key={g.file}>
                {g.file} <Stat adds={sumOf(g.hunks, 'adds')} dels={sumOf(g.hunks, 'dels')} />
              </li>
            ))}
          </ul>
        </>
      ) : (
        <>
          <div className="explain-label">讲解</div>
          {step.explain ? (
            <div className="md">
              <Markdown source={step.explain} />
            </div>
          ) : (
            <p className="muted">这一步没有写讲解。</p>
          )}
        </>
      )}
      {step.related.length > 0 && (
        <div className="related">
          <div className="explain-label">关联步骤</div>
          {step.related.map((k) => (
            <button key={k} className="rel-item" type="button" onClick={() => onGo(k)}>
              <span className="rel-no">第 {k + 1} 步</span>
              <span>{items[k]?.title}</span>
            </button>
          ))}
        </div>
      )}
    </aside>
  );
}
