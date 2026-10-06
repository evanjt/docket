import { describe, expect, it } from 'vitest';
import { findItems, score } from './palette';
import type { GraphNode } from './types';

const n = (id: string, title: string, word = 'ready'): GraphNode => ({
  id, title, word, key: 'T', kind: 'work', state: 'open',
});

describe('score', () => {
  it('ranks an exact match over a prefix over a substring over scattered words', () => {
    const s = [score('t5', 'T5'), score('t5', 'T56'), score('web', 'a web client'), score('client web', 'web page client')];
    expect(s[0]).toBeGreaterThan(s[1]);
    expect(s[1]).toBeGreaterThan(s[2]);
    expect(s[2]).toBeGreaterThan(s[3]);
    expect(s[3]).toBeGreaterThan(0);
  });

  it('finds nothing in an unrelated text', () => {
    expect(score('zebra', 'web client')).toBe(0);
  });
});

describe('findItems', () => {
  const nodes = [n('T5', 'Search the bodies', 'done'), n('T56', 'TUI follows the simple model'), n('A8', 'Web client')];

  it('puts the id typed first', () => {
    expect(findItems('t5', nodes).map((x) => x.id)).toEqual(['T5', 'T56']);
  });

  it('matches titles', () => {
    expect(findItems('client', nodes).map((x) => x.id)).toEqual(['A8']);
  });

  it('answers nothing to nothing', () => {
    expect(findItems(' ', nodes)).toEqual([]);
  });
});
