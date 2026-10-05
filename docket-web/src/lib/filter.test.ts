import { describe, expect, it } from 'vitest';
import { applies, filterChanges, nextParams, parseFilter, remainingHref, searchParams, sortRows } from './filter';

const parse = (s: string) => parseFilter(new URLSearchParams(s));

describe('filter state', () => {
  it('round-trips through the query string', () => {
    const f = parse('key=B&priority=high&complexity=low&release=1.1&under=A20&state=open&sort=priority');
    expect(f).toEqual({ key: 'B', priority: 'high', complexity: 'low', release: '1.1', under: 'A20', state: 'open', sort: 'priority' });
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(filterChanges(f))) if (v) q.set(k, v);
    expect(parseFilter(q)).toEqual(f);
  });

  it('drops values outside the options', () => {
    const f = parse('priority=urgent&complexity=huge&state=maybe&sort=random');
    expect(f).toEqual({ key: '', priority: '', complexity: '', release: '', under: '', state: '', sort: '' });
  });

  it('maps to the parameters /next and /search take', () => {
    const f = parse('key=b&priority=high&complexity=low&release=1.1&under=A20&state=open');
    expect(nextParams(f)).toEqual({ key: 'b', priority: 'high', complexity: 'low', under: 'A20' });
    expect(searchParams(f)).toEqual({ key: 'b', state: 'open' });
  });

  it('applies what the routes leave out to rows', () => {
    const rows = [
      { id: 'B1', priority: 'low', complexity: 'low', state: 'open', theme: '1.1' },
      { id: 'B2', priority: 'critical', complexity: 'high', state: 'done', theme: null },
    ];
    const releases = ['1.0', '1.1'];
    expect(rows.filter((r) => applies(r, parse('priority=high'), releases)).map((r) => r.id)).toEqual(['B2']);
    expect(rows.filter((r) => applies(r, parse('complexity=low'), releases)).map((r) => r.id)).toEqual(['B1']);
    expect(rows.filter((r) => applies(r, parse('release=1.0'), releases)).map((r) => r.id)).toEqual(['B2']);
    expect(rows.filter((r) => applies(r, parse('state=done&key=B'), releases)).map((r) => r.id)).toEqual(['B2']);
    expect(rows.filter((r) => applies(r, parse('key=T'), releases))).toEqual([]);
  });

  it('sorts by priority, then id', () => {
    const rows = [
      { id: 'B10', priority: 'low' },
      { id: 'B2', priority: 'high' },
      { id: 'B1', priority: 'low' },
    ];
    expect(sortRows(rows, 'priority').map((r) => r.id)).toEqual(['B2', 'B1', 'B10']);
    expect(sortRows(rows, 'id').map((r) => r.id)).toEqual(['B1', 'B2', 'B10']);
    expect(sortRows(rows, '').map((r) => r.id)).toEqual(['B10', 'B2', 'B1']);
  });

  it('links a plan to the work still open under it', () => {
    expect(remainingHref('acme/site', 'A20')).toBe('/ui/acme/site/work?list=next&under=A20');
  });
});
