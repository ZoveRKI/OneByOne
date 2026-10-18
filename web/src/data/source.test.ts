import { afterEach, describe, expect, it } from 'vitest';
import { parseWalkthrough, readEmbedded } from './source';

describe('readEmbedded', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('读取命令行写进页面的数据', () => {
    document.body.innerHTML = '<script id="onebyone-data" type="application/json">{"a":1}</script>';
    expect(readEmbedded()).toBe('{"a":1}');
  });

  it('占位的 null 表示没有内嵌数据', () => {
    document.body.innerHTML = '<script id="onebyone-data" type="application/json">null</script>';
    expect(readEmbedded()).toBeNull();
  });
});

describe('parseWalkthrough', () => {
  it('区分 JSON 写错和格式不符合要求', () => {
    expect(parseWalkthrough('{', 'a.json')).toMatchObject({
      error: expect.stringContaining('a.json 不是合法的 JSON'),
    });
    expect(parseWalkthrough('{}', 'b.json')).toEqual({
      error: 'b.json 的内容不符合格式要求：缺少 hunks 数组',
    });
  });
});
