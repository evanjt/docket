import { describe, expect, it } from 'vitest';
import { applies, filterChanges, nextParams, pagedParams, parseFilter, remainingHref, searchParams, sortRows } from './filter';

const scale = { priorities: ['critical', 'high', 'normal', 'low'], levels: ['high', 'medium', 'low'] };
const parse = (s: string, on = scale) => parseFilter(new URLSearchParams(s), on);

describe('filter state', () => {
  it('round-trips through the query string', () => {
    const f = parse('key=B&priority=high&complexity=low&release=1.1&under=A20&state=open&sort=priority');
    expect(f).toEqual({ key: 'B', priority: 'high', complexity: 'low', release: '1.1', under: 'A20', area: '', state: 'open', sort: 'priority' });
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(filterChanges(f))) if (v) q.set(k, v);
    expect(parseFilter(q, scale)).toEqual(f);
  });

  it('drops values outside the options', () => {
    const f = parse('priority=urgent&complexity=huge&state=maybe&sort=random');
    expect(f).toEqual({ key: '', priority: '', complexity: '', release: '', under: '', area: '', state: '', sort: '' });
  });

  it('takes the tiers and levels the server lists', () => {
    const wider = { priorities: ['urgent', ...scale.priorities], levels: ['epic', ...scale.levels] };
    expect(parse('priority=urgent&complexity=epic', wider)).toMatchObject({ priority: 'urgent', complexity: 'epic' });
    expect(parse('priority=urgent&complexity=epic')).toMatchObject({ priority: '', complexity: '' });
    const rows = [
      { id: 'B1', priority: 'critical' },
      { id: 'B2', priority: 'urgent' },
    ];
    expect(sortRows(rows, 'priority', wider.priorities).map((r) => r.id)).toEqual(['B2', 'B1']);
    expect(rows.filter((r) => applies(r, parse('priority=urgent', wider), [], wider.priorities)).map((r) => r.id)).toEqual(['B2']);
  });

  it('keeps a value from the address until the lists are read', () => {
    expect(parse('priority=high&complexity=low', { priorities: [], levels: [] })).toMatchObject({ priority: 'high', complexity: 'low' });
  });

  it('maps to the parameters /next and /search take', () => {
    const f = parse('key=b&priority=high&complexity=low&release=1.1&under=A20&state=open');
    expect(nextParams(f)).toEqual({ key: 'b', priority: 'high', complexity: 'low', under: 'A20' });
    expect(searchParams(f)).toEqual({ key: 'b', state: 'open', under: 'A20' });
    expect(pagedParams(f)).toEqual({ release: '1.1', under: 'A20' });
    expect(pagedParams(parse(''))).toEqual({ release: undefined, under: undefined });
  });

  it('applies what the routes leave out to rows', () => {
    const rows = [
      { id: 'B1', priority: 'low', complexity: 'low', state: 'open', release: '1.1' },
      { id: 'B2', priority: 'critical', complexity: 'high', state: 'done', release: '1.0' },
    ];
    const releases = ['1.0', '1.1'];
    expect(rows.filter((r) => applies(r, parse('priority=high'), releases, scale.priorities)).map((r) => r.id)).toEqual(['B2']);
    expect(rows.filter((r) => applies(r, parse('complexity=low'), releases, scale.priorities)).map((r) => r.id)).toEqual(['B1']);
    expect(rows.filter((r) => applies(r, parse('release=1.0'), releases, scale.priorities)).map((r) => r.id)).toEqual(['B2']);
    expect(rows.filter((r) => applies(r, parse('state=done&key=B'), releases, scale.priorities)).map((r) => r.id)).toEqual(['B2']);
    expect(rows.filter((r) => applies(r, parse('key=T'), releases, scale.priorities))).toEqual([]);
  });

  it('sorts by priority, then id', () => {
    const rows = [
      { id: 'B10', priority: 'low' },
      { id: 'B2', priority: 'high' },
      { id: 'B1', priority: 'low' },
    ];
    expect(sortRows(rows, 'priority', scale.priorities).map((r) => r.id)).toEqual(['B2', 'B1', 'B10']);
    expect(sortRows(rows, 'id', scale.priorities).map((r) => r.id)).toEqual(['B1', 'B2', 'B10']);
    expect(sortRows(rows, '', scale.priorities).map((r) => r.id)).toEqual(['B10', 'B2', 'B1']);
  });

  it('links a plan to the work still open under it', () => {
    expect(remainingHref('acme/site', 'A20')).toBe('/ui/acme/site/work?list=next&under=A20');
  });

  it('keeps the rows in one area, ignoring case, and leaves the area out of the routes\' parameters', () => {
    const rows = [{ id: 'B1', area: 'Kites' }, { id: 'B2', area: 'lanterns' }, { id: 'B3', area: null }];
    const f = parse('area=kites');
    expect(f.area).toBe('kites');
    expect(rows.filter((r) => applies(r, f, [], scale.priorities)).map((r) => r.id)).toEqual(['B1']);
    expect(nextParams(f)).not.toHaveProperty('area');
    expect(pagedParams(f)).not.toHaveProperty('area');
  });
});
