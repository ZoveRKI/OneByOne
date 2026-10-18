import { useMemo, type CSSProperties, type ReactNode } from 'react';
import { STATUS, type Model, type Track } from '../core/model';
import { baseName, sumOf } from '../lib/format';
import { buildTree, byName, type DirNode } from '../lib/tree';
import { CheckIcon, ChevronIcon, WarnIcon } from './icons';
import { WARN_KINDS } from './kinds';
import { Stat } from './Stat';

interface Props {
  model: Model;
  track: Track;
  index: number;
  collapsed: ReadonlySet<string>;
  onGo: (index: number) => void;
  onOpenFile: (path: string) => void;
  onToggleDir: (path: string) => void;
  onGotoUncovered: () => void;
}

export function Sidebar(props: Props) {
  const { model, track } = props;
  return (
    <aside className="sidebar" aria-label="改动文件">
      <Summary model={model} />
      <div className="sidebar-head">
        <span>改动文件</span>
        <span>{track.fileSteps.size}</span>
      </div>
      <FileTree {...props} />
      <Coverage model={model} onGotoUncovered={props.onGotoUncovered} />
    </aside>
  );
}

function Summary({ model }: { model: Model }) {
  const finals = [...model.hunks.values()].filter((h) => h.final);
  const fileCount = new Set(finals.map((h) => h.file)).size;
  return (
    <div className="summary">
      <div className="summary-title">{model.title}</div>
      {model.summary && <div className="summary-text">{model.summary}</div>}
      <div className="summary-stats">
        {fileCount} 个文件 · <Stat adds={sumOf(finals, 'adds')} dels={sumOf(finals, 'dels')} />
        {model.base && (
          <>
            {' '}
            · 基准 <code>{model.base}</code>
          </>
        )}
      </div>
    </div>
  );
}

const depthStyle = (depth: number) => ({ '--depth': depth }) as CSSProperties;

function FileTree({ model, track, index, collapsed, onGo, onOpenFile, onToggleDir }: Props) {
  const root = useMemo(() => buildTree([...track.fileSteps.keys()]), [track]);

  const renderNode = (node: DirNode, depth: number): ReactNode[] => {
    const rows: ReactNode[] = [];
    for (const d of [...node.dirs.values()].toSorted((a, b) => byName(a.name, b.name))) {
      const open = !collapsed.has(d.path);
      rows.push(
        <div
          key={`d:${d.path}`}
          className={`tree-row tree-dir${open ? ' open' : ''}`}
          style={depthStyle(depth)}
          role="button"
          tabIndex={0}
          aria-expanded={open}
          onClick={() => onToggleDir(d.path)}
        >
          <ChevronIcon />
          <span className="tree-name">{d.name}</span>
        </div>,
      );
      if (open) rows.push(...renderNode(d, depth + 1));
    }
    for (const path of node.files.toSorted((a, b) => byName(baseName(a), baseName(b)))) {
      const file = model.files.get(path);
      if (!file) continue;
      const status = STATUS[file.status];
      const steps = track.fileSteps.get(path) ?? [];
      // current：当前步骤涉及；pending：还没讲到（灰色）
      const cls = steps.includes(index) ? ' current' : steps.some((k) => k < index) ? '' : ' pending';
      rows.push(
        <div
          key={`f:${path}`}
          className={`tree-row tree-file${cls}`}
          style={depthStyle(depth)}
          title={path}
          onClick={() => onOpenFile(path)}
        >
          <span className={`st st-${file.status}`} title={status.label}>
            {status.short}
          </span>
          <span className="tree-name">{baseName(path)}</span>
          <span className="tree-stat">
            {file.adds || file.dels ? <Stat adds={file.adds} dels={file.dels} /> : null}
          </span>
          <span className="chips">
            {steps.map((k) => {
              const s = track.items[k];
              return (
                <button
                  key={k}
                  className={`chip${k === index ? ' on' : ''}${s && WARN_KINDS.has(s.kind) ? ' warn' : ''}`}
                  type="button"
                  title={`第 ${k + 1} 步 · ${s?.title ?? ''}`}
                  onClick={(e) => {
                    // 点编号只跳转步骤，不触发整行的「打开文件」
                    e.stopPropagation();
                    onGo(k);
                  }}
                >
                  {k + 1}
                </button>
              );
            })}
          </span>
        </div>,
      );
    }
    return rows;
  };

  const rows = renderNode(root, 0);
  return <nav className="tree">{rows.length ? rows : <div className="muted">没有文件</div>}</nav>;
}

function Coverage({ model, onGotoUncovered }: { model: Model; onGotoUncovered: () => void }) {
  const { coverage, issues } = model;
  let summary: ReactNode;
  if (!coverage.total) {
    summary = <div className="cov">没有可检查的改动</div>;
  } else if (!coverage.uncovered.length) {
    summary = (
      <div className="cov ok">
        <CheckIcon />
        <span>讲解覆盖了全部 {coverage.total} 处改动</span>
      </div>
    );
  } else {
    summary = (
      <button className="cov warn" type="button" onClick={onGotoUncovered}>
        <WarnIcon />
        <span>
          {coverage.uncovered.length} 处改动未讲解（共 {coverage.total} 处）
        </span>
        <span className="cov-link">查看</span>
      </button>
    );
  }
  return (
    <div className="coverage">
      {summary}
      {issues.length > 0 && (
        <details className="issues">
          <summary>{issues.length} 条数据问题</summary>
          <ul>
            {issues.map((msg, i) => (
              <li key={i}>{msg}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
