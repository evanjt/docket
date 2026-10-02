import { describe, expect, it } from 'vitest';
import { ids, render } from './markdown';

const links = { known: (id: string) => ['T5', 'A7'].includes(id), item: (id: string) => `/ui/o/p/work?i=${id}` };

describe('render', () => {
  it('escapes HTML in the source', () => {
    expect(render('<script>alert(1)</script>')).toBe('<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>');
  });

  it('escapes HTML inside code spans', () => {
    expect(render('`<b>`')).toBe('<p><code>&lt;b&gt;</code></p>');
  });

  it('reads bold, code and paragraphs', () => {
    expect(render('**Fix.** Run `cargo test`.\n\nThen close.')).toBe(
      '<p><strong>Fix.</strong> Run <code>cargo test</code>.</p>\n<p>Then close.</p>',
    );
  });

  it('reads bullet and numbered lists, with continuation lines', () => {
    expect(render('- one\n  more\n- two\n\n1. first')).toBe(
      '<ul><li>one more</li><li>two</li></ul>\n<ol><li>first</li></ol>',
    );
  });

  it('links the ids the project holds and no others', () => {
    expect(render('See T5 and UTF8 and A7.', links)).toBe(
      '<p>See <a class="ref" href="/ui/o/p/work?i=T5">T5</a> and UTF8 and <a class="ref" href="/ui/o/p/work?i=A7">A7</a>.</p>',
    );
  });

  it('leaves ids inside code alone', () => {
    expect(render('`T5`', links)).toBe('<p><code>T5</code></p>');
  });

  it('links web addresses and nothing else', () => {
    expect(render('at https://example.com/x and [docs](https://d.example)')).toBe(
      '<p>at <a href="https://example.com/x" target="_blank" rel="noreferrer">https://example.com/x</a> and <a href="https://d.example" target="_blank" rel="noreferrer">docs</a></p>',
    );
    expect(render('[x](javascript:alert(1))')).toBe('<p>[x](javascript:alert(1))</p>');
  });

  it('keeps fenced code as it is', () => {
    expect(render('```\na <b>\n**x**\n```')).toBe('<pre><code>a &lt;b&gt;\n**x**</code></pre>');
  });
});

describe('ids', () => {
  it('names each id once, in order', () => {
    expect(ids('T5 then A7, T5 again')).toEqual(['T5', 'A7']);
  });
});
