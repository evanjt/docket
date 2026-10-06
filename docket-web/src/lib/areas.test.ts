import { describe, expect, it } from 'vitest';
import { areaCounts } from './areas';
import type { Area, GraphNode } from './types';

const area = (name: string, position: number, priority: string | null = null): Area => ({ id: position, name, description: `${name} work`, position, priority });
const node = (id: string, areaName: string | null, word: string, kind: GraphNode['kind'] = 'work'): GraphNode => ({
  id, key: id.replace(/\d+/, ''), kind, state: word === 'done' ? 'done' : 'open', title: id, word, area: areaName,
});

describe('areaCounts', () => {
  it('counts each area\'s open and closed items in position order, leaving plans and dropped items out', () => {
    const nodes = [
      node('T1', 'lanterns', 'ready'),
      node('T2', 'lanterns', 'done'),
      node('T3', 'kites', 'under way'),
      node('T4', 'kites', 'dropped'),
      node('A1', 'kites', 'under way', 'audit'),
    ];
    expect(areaCounts([area('lanterns', 2, 'high'), area('kites', 1)], nodes)).toEqual([
      { area: area('kites', 1), open: 1, done: 0 },
      { area: area('lanterns', 2, 'high'), open: 1, done: 1 },
    ]);
  });
});
