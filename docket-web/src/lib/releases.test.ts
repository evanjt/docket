import { describe, expect, it } from 'vitest';
import { holdsOf, releaseList, releaseOf, releaseRow, releaseRows } from './releases';
import type { GraphNode } from './types';

const node = (id: string, theme: string | null, word: string, state = 'open', kind = 'work'): GraphNode =>
  ({ id, key: 'T', kind, state, theme, title: id, word }) as GraphNode;

describe('releaseList', () => {
  it('reads the fact in order and nothing when it is unset', () => {
    expect(releaseList({ releases: ' 1.0  1.1 2.0 ' })).toEqual(['1.0', '1.1', '2.0']);
    expect(releaseList({})).toEqual([]);
    expect(releaseList(undefined)).toEqual([]);
  });
});

describe('releaseOf', () => {
  const r = ['1.0', '1.1', '1.2'];
  it('is the theme when listed, else the current release', () => {
    expect(releaseOf('1.2', r)).toBe('1.2');
    expect(releaseOf(null, r)).toBe('1.0');
    expect(releaseOf('docs', r)).toBe('1.0');
  });
  it('is nothing without releases', () => {
    expect(releaseOf('1.1', [])).toBeNull();
  });
});

describe('releaseRows', () => {
  it('counts tickets per release, done, live and open words, leaving out plans and dropped tickets', () => {
    const rows = releaseRows(
      [
        node('T1', null, 'ready'),
        node('T2', 'docs', 'building'),
        node('T3', '1.0', 'done', 'done'),
        node('T4', '1.1', 'blocked'),
        node('T5', '1.1', 'dropped', 'dropped'),
        node('A1', '1.1', 'building', 'open', 'audit'),
      ],
      ['1.0', '1.1'],
    );
    expect(rows).toEqual([
      { name: '1.0', current: true, done: 1, total: 3, live: 1, words: { ready: 1, building: 1 } },
      { name: '1.1', current: false, done: 0, total: 1, live: 0, words: { blocked: 1 } },
    ]);
  });
});

describe('releaseRow', () => {
  it('gives a plan with a listed theme a Release row', () => {
    expect(releaseRow('0.4', ['1.0', '0.4'])).toEqual({ name: '0.4', current: false });
  });
  it('marks the current release and falls back to it for an unlisted theme', () => {
    expect(releaseRow(null, ['1.0', '0.4'])).toEqual({ name: '1.0', current: true });
  });
  it('has no row without releases', () => {
    expect(releaseRow('0.4', [])).toBeNull();
  });
});

describe('holdsOf', () => {
  it('lists what an item holds, in order', () => {
    expect(holdsOf({ holds: [{ id: 'T2' }, { id: 'T3' }] })).toEqual(['T2', 'T3']);
  });
  it('is empty when the reply has none', () => {
    expect(holdsOf(undefined)).toEqual([]);
    expect(holdsOf({})).toEqual([]);
  });
});
