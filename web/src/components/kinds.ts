/** 步骤类型标签；tone 对应 styles.css 里的 tone-xxx 配色 */
export const KINDS: Record<string, { label: string; tone: string }> = {
  add: { label: '新增', tone: 'add' },
  modify: { label: '修改', tone: 'accent' },
  delete: { label: '删除', tone: 'del' },
  refactor: { label: '重构', tone: 'accent' },
  fix: { label: '修复', tone: 'warn' },
  rework: { label: '返工', tone: 'warn' },
  test: { label: '测试', tone: 'accent' },
  config: { label: '配置', tone: 'neutral' },
  docs: { label: '文档', tone: 'neutral' },
  style: { label: '样式', tone: 'neutral' },
  uncovered: { label: '未讲解', tone: 'warn' },
};

/** 这些类型的步骤编号在文件树里用黄色标出，提醒多看一眼 */
export const WARN_KINDS = new Set(['fix', 'rework', 'uncovered']);

export function kindOf(kind: string): { label: string; tone: string } | null {
  if (Object.hasOwn(KINDS, kind)) return KINDS[kind] ?? null;
  return kind ? { label: kind, tone: 'neutral' } : null;
}
