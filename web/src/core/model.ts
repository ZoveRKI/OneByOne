/*
 * walkthrough 数据的校验与整理，并计算讲解覆盖率。
 * 命令行（server/src/model.rs）用同一套规则，build 的检查结果和播放器里看到的才一致。
 */

export type FileStatus = 'added' | 'modified' | 'deleted' | 'renamed';

export const STATUS: Record<FileStatus, { short: string; label: string }> = {
  added: { short: 'A', label: '新增' },
  modified: { short: 'M', label: '修改' },
  deleted: { short: 'D', label: '删除' },
  renamed: { short: 'R', label: '重命名' },
};

export type LineType = 'add' | 'del' | 'ctx' | 'meta';

export interface DiffLine {
  type: LineType;
  text: string;
  oldNo: number | null;
  newNo: number | null;
}

export interface Hunk {
  id: string;
  file: string;
  oldStart: number;
  newStart: number;
  oldCount: number;
  newCount: number;
  header: string;
  /** false 表示过程中的中间改动，不在最终 diff 里 */
  final: boolean;
  lines: DiffLine[];
  adds: number;
  dels: number;
}

export interface FileInfo {
  path: string;
  status: FileStatus;
  oldPath: string;
  adds: number;
  dels: number;
}

export interface Step {
  id: string;
  title: string;
  chapter: string;
  kind: string;
  time: string;
  explain: string;
  /** 关联步骤的下标 */
  related: number[];
  hunks: Hunk[];
  /** 自动生成的「未讲解的改动」步骤 */
  synthetic: boolean;
}

export type TrackName = 'steps' | 'timeline';

export interface Track {
  label: string;
  items: Step[];
  /** 每个文件出现在哪些步骤里（步骤下标，从小到大） */
  fileSteps: Map<string, number[]>;
}

export interface Model {
  title: string;
  summary: string;
  base: string;
  hunks: Map<string, Hunk>;
  files: Map<string, FileInfo>;
  tracks: { steps: Track; timeline: Track | null };
  issues: string[];
  coverage: { total: number; uncovered: Hunk[] };
}

type Raw = Record<string, unknown>;

const field = (obj: unknown, key: string): unknown =>
  typeof obj === 'object' && obj !== null ? (obj as Raw)[key] : undefined;
const isStatus = (value: string): value is FileStatus => Object.hasOwn(STATUS, value);
const normPath = (p: unknown) => String(p).trim().replace(/\\/g, '/').replace(/^\.\//, '');
const lineStart = (value: unknown, fallback: number) =>
  typeof value === 'number' && Number.isInteger(value) && value >= 0 ? value : fallback;
const text = (value: unknown, fallback = '') => (value ? String(value) : fallback);
const sum = (hunks: Hunk[], key: 'adds' | 'dels') => hunks.reduce((n, h) => n + h[key], 0);

export function normalize(raw: unknown): Model {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw))
    throw new Error('顶层必须是一个 JSON 对象');
  const data = raw as Raw;
  if (!Array.isArray(data.hunks)) throw new Error('缺少 hunks 数组');
  if (!Array.isArray(data.steps) || data.steps.length === 0)
    throw new Error('缺少 steps 数组，或 steps 为空');

  const issues: string[] = [];
  if (data.version !== undefined && data.version !== 1) {
    issues.push(`数据版本是 ${String(data.version)}，播放器按版本 1 解析`);
  }

  const hunks = new Map<string, Hunk>();
  data.hunks.forEach((h: unknown, i: number) => {
    if (typeof h !== 'object' || h === null) {
      issues.push(`hunks[${i}] 不是对象，已忽略`);
      return;
    }
    const rawId = field(h, 'id');
    const id = rawId == null ? '' : String(rawId);
    if (!id) {
      issues.push(`hunks[${i}] 缺少 id，已忽略`);
    } else if (hunks.has(id)) {
      issues.push(`改动块 id 重复：${id}，后出现的已忽略`);
    } else if (!field(h, 'file')) {
      issues.push(`改动块 ${id} 缺少 file，已忽略`);
    } else if (!Array.isArray(field(h, 'lines'))) {
      issues.push(`改动块 ${id} 缺少 lines 数组，已忽略`);
    } else {
      hunks.set(id, parseHunk(id, h as Raw));
    }
  });

  const files = new Map<string, FileInfo>();
  (Array.isArray(data.files) ? data.files : []).forEach((f: unknown, i: number) => {
    if (!field(f, 'path')) {
      issues.push(`files[${i}] 缺少 path，已忽略`);
      return;
    }
    const path = normPath(field(f, 'path'));
    const rawStatus = text(field(f, 'status'), 'modified');
    let status: FileStatus = 'modified';
    if (isStatus(rawStatus)) status = rawStatus;
    else issues.push(`文件 ${path} 的 status 无法识别：${rawStatus}，按“修改”处理`);
    const oldPath = field(f, 'oldPath');
    files.set(path, { path, status, oldPath: oldPath ? normPath(oldPath) : '', adds: 0, dels: 0 });
  });
  const allHunks = [...hunks.values()];
  for (const h of allHunks) {
    if (!files.has(h.file)) {
      files.set(h.file, {
        path: h.file,
        status: inferStatus(h.file, allHunks),
        oldPath: '',
        adds: 0,
        dels: 0,
      });
    }
  }
  for (const f of files.values()) {
    const own = allHunks.filter((h) => h.final && h.file === f.path);
    f.adds = sum(own, 'adds');
    f.dels = sum(own, 'dels');
  }

  const steps = buildTrack('讲解顺序', data.steps, hunks, issues);
  let timeline: Track | null = null;
  if (Array.isArray(data.timeline) && data.timeline.length) {
    timeline = buildTrack('真实过程', data.timeline, hunks, issues);
  } else if (data.timeline !== undefined && !Array.isArray(data.timeline)) {
    issues.push('timeline 不是数组，已忽略');
  }
  steps.items.forEach((s, i) => {
    for (const h of s.hunks) if (!h.final) issues.push(`讲解顺序第 ${i + 1} 步引用了过程中的改动块：${h.id}`);
  });

  // 覆盖率：最终 diff 里的每个改动块，都应该出现在某个讲解步骤里
  const covered = new Set<string>();
  for (const s of steps.items) for (const h of s.hunks) covered.add(h.id);
  const finals = allHunks.filter((h) => h.final);
  const uncovered = finals.filter((h) => !covered.has(h.id));
  if (uncovered.length) {
    steps.items.push({
      id: '',
      title: '未讲解的改动',
      chapter: '待检查',
      kind: 'uncovered',
      time: '',
      explain: '',
      related: [],
      hunks: uncovered,
      synthetic: true,
    });
  }
  indexTrack(steps);
  if (timeline) indexTrack(timeline);

  return {
    title: text(data.title, '未命名的改动'),
    summary: text(data.summary),
    base: text(data.base),
    hunks,
    files,
    tracks: { steps, timeline },
    issues,
    coverage: { total: finals.length, uncovered },
  };
}

// 按 unified diff 的规则给每一行算出新旧行号
function parseHunk(id: string, h: Raw): Hunk {
  const oldStart = lineStart(h.oldStart, 1);
  const newStart = lineStart(h.newStart, 1);
  let oldNo = oldStart;
  let newNo = newStart;
  let adds = 0;
  let dels = 0;
  const lines = (h.lines as unknown[]).map((value): DiffLine => {
    const line = value == null ? '' : String(value);
    const sign = line.charAt(0);
    if (sign === '+') {
      adds += 1;
      return { type: 'add', text: line.slice(1), oldNo: null, newNo: newNo++ };
    }
    if (sign === '-') {
      dels += 1;
      return { type: 'del', text: line.slice(1), oldNo: oldNo++, newNo: null };
    }
    if (sign === '\\') return { type: 'meta', text: line, oldNo: null, newNo: null };
    return { type: 'ctx', text: sign === ' ' ? line.slice(1) : line, oldNo: oldNo++, newNo: newNo++ };
  });
  return {
    id,
    file: normPath(h.file),
    oldStart,
    newStart,
    oldCount: lines.filter((l) => l.type === 'ctx' || l.type === 'del').length,
    newCount: lines.filter((l) => l.type === 'ctx' || l.type === 'add').length,
    header: text(h.header),
    final: h.final !== false,
    lines,
    adds,
    dels,
  };
}

function inferStatus(path: string, allHunks: Hunk[]): FileStatus {
  const own = allHunks.filter((h) => h.final && h.file === path);
  if (own.length && own.every((h) => h.oldStart === 0 && h.oldCount === 0)) return 'added';
  if (own.length && own.every((h) => h.newStart === 0 && h.newCount === 0)) return 'deleted';
  return 'modified';
}

function buildTrack(label: string, list: unknown[], hunks: Map<string, Hunk>, issues: string[]): Track {
  const items: Step[] = [];
  const relatedRaw: unknown[][] = [];
  list.forEach((s, i) => {
    const where = `${label}第 ${i + 1} 步`;
    if (typeof s !== 'object' || s === null) {
      issues.push(`${where}不是对象，已忽略`);
      return;
    }
    const step = s as Raw;
    const stepHunks: Hunk[] = [];
    for (const ref of Array.isArray(step.hunks) ? step.hunks : []) {
      const id = String(typeof ref === 'object' && ref !== null ? field(ref, 'id') : ref);
      const h = hunks.get(id);
      if (!h) issues.push(`${where}引用了不存在的改动块：${id}`);
      else if (!stepHunks.includes(h)) stepHunks.push(h);
    }
    items.push({
      id: step.id == null ? '' : String(step.id),
      title: text(step.title, `第 ${i + 1} 步`),
      chapter: text(step.chapter),
      kind: text(step.kind),
      time: text(step.time),
      explain: typeof step.explain === 'string' ? step.explain : '',
      related: [],
      hunks: stepHunks,
      synthetic: false,
    });
    relatedRaw.push(Array.isArray(step.related) ? step.related : []);
  });
  // related 可以写步骤 id，也可以写从 1 开始的序号
  items.forEach((s, i) => {
    for (const r of relatedRaw[i] ?? []) {
      const k = typeof r === 'number' ? r - 1 : items.findIndex((x) => x.id !== '' && x.id === String(r));
      if (Number.isInteger(k) && k >= 0 && k < items.length && k !== i) s.related.push(k);
      else issues.push(`${label}第 ${i + 1} 步的关联步骤无效：${String(r)}`);
    }
  });
  return { label, items, fileSteps: new Map() };
}

// 记录每个文件出现在哪些步骤里，文件树和跳转都要用
function indexTrack(track: Track) {
  track.fileSteps = new Map();
  track.items.forEach((s, i) => {
    for (const h of s.hunks) {
      const list = track.fileSteps.get(h.file) ?? [];
      if (list[list.length - 1] !== i) list.push(i);
      track.fileSteps.set(h.file, list);
    }
  });
}
