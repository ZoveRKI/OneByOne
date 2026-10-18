import { describe, expect, it } from 'vitest';
import { getLang, highlightLine, langOfPath, type Token } from './highlight';

const pairs = (tokens: Token[]) => tokens.map((t) => [t.cls, t.text]);

describe('highlightLine', () => {
  it('区分关键字、函数名、类型名、字符串、数字和注释', () => {
    const st = { inBlock: false };
    expect(pairs(highlightLine("const x: Foo = bar('a', 42); // ok", getLang('ts'), st))).toEqual([
      ['kw', 'const'],
      [null, ' '],
      [null, 'x'],
      [null, ': '],
      ['type', 'Foo'],
      [null, ' = '],
      ['fn', 'bar'],
      [null, '('],
      ['str', "'a'"],
      [null, ', '],
      ['num', '42'],
      [null, '); '],
      ['com', '// ok'],
    ]);
  });

  it('块注释可以跨行', () => {
    const lang = getLang('js');
    const st = { inBlock: false };
    expect(pairs(highlightLine('a /* start', lang, st))).toEqual([
      [null, 'a'],
      [null, ' '],
      ['com', '/* start'],
    ]);
    expect(st.inBlock).toBe(true);
    expect(pairs(highlightLine('still */ b', lang, st))).toEqual([
      ['com', 'still */'],
      [null, ' '],
      [null, 'b'],
    ]);
    expect(st.inBlock).toBe(false);
  });

  it('不认识的语言按纯文本显示', () => {
    expect(pairs(highlightLine('const a', null, { inBlock: false }))).toEqual([[null, 'const a']]);
  });
});

describe('langOfPath', () => {
  it('按扩展名或文件名找规则', () => {
    expect(langOfPath('src/App.TSX')).toBe(getLang('tsx'));
    expect(langOfPath('Makefile')).toBe(getLang('makefile'));
    expect(langOfPath('notes.txt')).toBeNull();
  });

  it('对象原型上的名字不会被当成语言', () => {
    expect(langOfPath('a.constructor')).toBeNull();
    expect(getLang('toString')).toBeNull();
  });
});
