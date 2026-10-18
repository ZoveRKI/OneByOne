import { describe, expect, it } from 'vitest';
import { buildTree } from './tree';

describe('buildTree', () => {
  it('只有一个子目录、没有文件的目录合并显示', () => {
    const root = buildTree(['src/main/java/A.java', 'src/main/java/B.java', 'README.md']);
    expect(root.files).toEqual(['README.md']);
    const src = root.dirs.get('src');
    expect(src?.name).toBe('src/main/java');
    expect(src?.path).toBe('src/main/java');
    expect(src?.files).toEqual(['src/main/java/A.java', 'src/main/java/B.java']);
  });

  it('目录里有文件时不合并', () => {
    const root = buildTree(['src/a.ts', 'src/lib/b.ts']);
    const src = root.dirs.get('src');
    expect(src?.name).toBe('src');
    expect(src?.files).toEqual(['src/a.ts']);
    expect(src?.dirs.get('lib')?.files).toEqual(['src/lib/b.ts']);
  });
});
