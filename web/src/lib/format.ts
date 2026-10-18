import type { Hunk } from '../core/model';

export const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));
export const baseName = (p: string) => p.slice(p.lastIndexOf('/') + 1);
export const dirName = (p: string) => p.slice(0, p.lastIndexOf('/') + 1);
export const sumOf = (hunks: Hunk[], key: 'adds' | 'dels') => hunks.reduce((n, h) => n + h[key], 0);
export const errorMessage = (err: unknown) => (err instanceof Error ? err.message : String(err));

export interface FileGroup {
  file: string;
  hunks: Hunk[];
}

/** 按文件分组，保持文件第一次出现的顺序 */
export function groupByFile(hunks: Hunk[]): FileGroup[] {
  const groups: FileGroup[] = [];
  const byFile = new Map<string, FileGroup>();
  for (const h of hunks) {
    let group = byFile.get(h.file);
    if (!group) {
      group = { file: h.file, hunks: [] };
      byFile.set(h.file, group);
      groups.push(group);
    }
    group.hunks.push(h);
  }
  return groups;
}

/** 改动块的范围标题；二进制、纯重命名等没有文本差异的改动显示为「文件级变化」 */
export function hunkLabel(h: Hunk): string {
  if (!h.oldCount && !h.newCount) return '文件级变化';
  return `@@ -${h.oldStart},${h.oldCount} +${h.newStart},${h.newCount} @@${h.header ? ` ${h.header}` : ''}`;
}
