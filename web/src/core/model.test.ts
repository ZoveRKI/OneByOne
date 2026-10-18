import { describe, expect, it } from 'vitest';
import sample from '../../../examples/walkthrough.json';
import { normalize } from './model';

describe('normalize', () => {
  it('示例数据只有故意留下的 src/config.ts 没讲到', () => {
    const model = normalize(sample);
    expect(model.issues).toEqual([]);
    expect(model.coverage.uncovered.map((h) => h.file)).toEqual(['src/config.ts']);
    expect(model.tracks.steps.items.at(-1)?.synthetic).toBe(true);
    expect(model.tracks.timeline).not.toBeNull();
  });

  it('按 unified diff 的规则计算行号，并统一路径写法', () => {
    const model = normalize({
      hunks: [
        {
          id: 'h1',
          file: './src\\a.ts',
          oldStart: 10,
          newStart: 12,
          lines: [' a', '-b', '+c', '+d', '\\ No newline at end of file', ' e'],
        },
      ],
      steps: [{ title: '一', hunks: ['h1'] }],
    });
    const h = model.hunks.get('h1');
    expect(h?.file).toBe('src/a.ts');
    expect(h?.lines.map((l) => [l.type, l.oldNo, l.newNo])).toEqual([
      ['ctx', 10, 12],
      ['del', 11, null],
      ['add', null, 13],
      ['add', null, 14],
      ['meta', null, null],
      ['ctx', 12, 15],
    ]);
    expect([h?.oldCount, h?.newCount, h?.adds, h?.dels]).toEqual([3, 4, 2, 1]);
  });

  it('数据有问题时记录下来，不中断播放', () => {
    const model = normalize({
      version: 2,
      hunks: [
        { id: 'h1', file: 'a.ts', lines: ['+x'] },
        { id: 'h1', file: 'b.ts', lines: [] },
        { file: 'c.ts', lines: [] },
      ],
      files: [{ path: 'a.ts', status: 'weird' }],
      steps: [
        { id: 'one', title: '一', hunks: ['h1', 'nope'], related: ['two', 9] },
        { id: 'two', title: '二', related: [1] },
      ],
      timeline: 'x',
    });
    expect(model.issues).toEqual([
      '数据版本是 2，播放器按版本 1 解析',
      '改动块 id 重复：h1，后出现的已忽略',
      'hunks[2] 缺少 id，已忽略',
      '文件 a.ts 的 status 无法识别：weird，按“修改”处理',
      '讲解顺序第 1 步引用了不存在的改动块：nope',
      '讲解顺序第 1 步的关联步骤无效：9',
      'timeline 不是数组，已忽略',
    ]);
    expect(model.tracks.steps.items.map((s) => s.related)).toEqual([[1], [0]]);
    expect(model.coverage).toEqual({ total: 1, uncovered: [] });
  });

  it('没写 files 时根据改动块推断新增和删除', () => {
    const model = normalize({
      hunks: [
        { id: 'h1', file: 'new.ts', oldStart: 0, newStart: 1, lines: ['+a'] },
        { id: 'h2', file: 'old.ts', oldStart: 1, newStart: 0, lines: ['-a'] },
        { id: 'h3', file: 'mid.ts', lines: [' a', '+b'] },
      ],
      steps: [{ title: '全部', hunks: ['h1', 'h2', 'h3'] }],
    });
    expect([...model.files.values()].map((f) => [f.path, f.status])).toEqual([
      ['new.ts', 'added'],
      ['old.ts', 'deleted'],
      ['mid.ts', 'modified'],
    ]);
  });

  it('缺少 hunks 或 steps 时直接报错', () => {
    expect(() => normalize([])).toThrow('顶层必须是一个 JSON 对象');
    expect(() => normalize({ steps: [{}] })).toThrow('缺少 hunks 数组');
    expect(() => normalize({ hunks: [], steps: [] })).toThrow('缺少 steps 数组');
  });
});
