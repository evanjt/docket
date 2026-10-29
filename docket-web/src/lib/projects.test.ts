import { describe, expect, it } from 'vitest';
import { byOrganisation, byRecent, loadOrder, saveOrder } from './projects';
import type { Count } from './types';

const count = (slug: string, last_event: string | null): Count => ({ slug, open: 1, done: 0, dropped: 0, last_event });

const counts = [
  count('a/x', '2026-10-29T08:00:00Z'),
  count('b/y', '2026-10-28T08:00:00Z'),
  count('a/z', '2026-10-22T08:00:00Z'),
  count('idle', null),
];

function store(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
  } as Storage;
}

describe('byRecent', () => {
  it('puts the newest first and a project never moved last', () => {
    expect(byRecent(['a/z', 'idle', 'b/y', 'a/x'], counts)).toEqual(['a/x', 'b/y', 'a/z', 'idle']);
  });

  it('breaks ties by slug and places a project without counts last', () => {
    const tied = [count('m/b', '2026-10-01T00:00:00Z'), count('m/a', '2026-10-01T00:00:00Z')];
    expect(byRecent(['m/b', 'unknown', 'm/a'], tied)).toEqual(['m/a', 'm/b', 'unknown']);
  });
});

describe('byOrganisation', () => {
  it('orders groups by their newest project and each group by recency', () => {
    expect(byOrganisation(['a/z', 'b/y', 'a/x'], counts)).toEqual([
      ['a', ['a/x', 'a/z']],
      ['b', ['b/y']],
    ]);
  });

  it('puts the prefix-less group last', () => {
    expect(byOrganisation(['idle', 'b/y'], counts).map(([o]) => o)).toEqual(['b', '']);
  });
});

describe('stored order', () => {
  it('round-trips and falls back to recent on an unknown value', () => {
    const s = store();
    expect(loadOrder(s)).toBe('recent');
    saveOrder('organisation', s);
    expect(loadOrder(s)).toBe('organisation');
    s.setItem('docket.projectOrder', 'sideways');
    expect(loadOrder(s)).toBe('recent');
  });
});
