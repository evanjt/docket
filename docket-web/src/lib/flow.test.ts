import { describe, expect, it } from 'vitest';
import { board, byId, daily, moves, plans, tally } from './flow';
import type { EventRow, Graph, GraphNode } from './types';

const node = (id: string, kind: GraphNode['kind'], word: string, rid = 0): GraphNode => ({
  id, rid, key: id.replace(/\d+/, ''), kind, state: word === 'done' ? 'done' : 'open', theme: null, title: id, word,
});

const GRAPH: Graph = {
  project: 'o/p',
  nodes: [
    node('A1', 'audit', 'blocked', 1),
    node('A2', 'audit', 'ready', 2),
    node('T1', 'work', 'done', 3),
    node('T2', 'work', 'building', 4),
    node('T3', 'work', 'done', 5),
    node('CON1', 'concept', 'standing', 6),
  ],
  edges: [
    { from: 'T1', kind: 'opened', to: 'A1' },
    { from: 'T2', kind: 'opened', to: 'A1' },
    { from: 'T3', kind: 'opened', to: 'A2' },
    { from: 'A1', kind: 'related', to: 'CON1' },
    { from: 'T1', kind: 'cites', to: 'src/a.rs' },
  ],
};

describe('board', () => {
  const b = board(GRAPH);

  it('reads opened edges from the parent', () => {
    expect(b.children.get('A1')).toEqual(['T1', 'T2']);
    expect(b.parents.get('T3')).toEqual(['A2']);
  });

  it('reads related both ways', () => {
    expect(b.related.get('CON1')).toEqual(['A1']);
    expect(b.related.get('A1')).toEqual(['CON1']);
  });

  it('tallies what an item opened', () => {
    // T1 done, T2 building
    expect(tally(b, 'A1')).toEqual({ done: 1, total: 2, live: 1 });
  });

  it('tallies what a concept is related to', () => {
    expect(tally(b, 'CON1')).toEqual({ done: 0, total: 1, live: 0 });
  });

  it('lists a plan whose items are all closed as due, first', () => {
    const rows = plans(b, 'audit');
    expect(rows.map((r) => [r.node.id, r.due])).toEqual([
      ['A2', true],
      ['A1', false],
    ]);
  });

  it('finds an item by the rid its events name', () => {
    expect(b.byRid.get(4)?.id).toBe('T2');
  });
});

describe('byId', () => {
  it('orders by key, then by number', () => {
    expect(['T10', 'B2', 'T9'].sort(byId)).toEqual(['B2', 'T9', 'T10']);
  });
});

const event = (at: string, kind: string, rid: number): EventRow => ({ seq: 0, project: 'o/p', rid, at, host: 'h', kind });

describe('daily', () => {
  const now = Date.parse('2026-10-02T12:00:00') / 1000;
  const events = [
    event('2026-10-02T09:00:00', 'closed', 1),
    event('2026-10-02T08:00:00', 'opened', 2),
    event('2026-10-01T08:00:00', 'dropped', 3),
    event('2026-09-29T08:00:00', 'opened', 4),
  ];

  it('counts each kind on its local day', () => {
    const days = daily(events, 3, now, true);
    expect(days.map((d) => [d.day, d.opened, d.closed, d.dropped])).toEqual([
      ['2026-09-30', 0, 0, 0],
      ['2026-10-01', 0, 0, 1],
      ['2026-10-02', 1, 1, 0],
    ]);
  });

  it('leaves out the days before the oldest event of a partial read', () => {
    const days = daily(events.slice(0, 3), 5, now, false);
    // the oldest event read is on 10-01, which may hold more than was read
    expect(days.map((d) => d.day)).toEqual(['2026-10-02']);
  });

  it('shows nothing for a partial read holding no event', () => {
    expect(daily([], 5, now, false)).toEqual([]);
  });
});

describe('moves', () => {
  const ids: Record<number, string> = { 1: 'T1', 2: 'T2' };

  it('gathers the events of one minute by verb', () => {
    const m = moves(
      [
        event('2026-10-02T09:00:40Z', 'closed', 1),
        event('2026-10-02T09:00:10Z', 'closed', 2),
        event('2026-10-02T09:00:05Z', 'claimed', 1),
        event('2026-10-02T08:59:00Z', 'opened', 2),
      ],
      (rid) => ids[rid],
    );
    expect(m.map((x) => x.verbs)).toEqual([
      [
        ['closed', ['T1', 'T2']],
        ['claimed', ['T1']],
      ],
      [['opened', ['T2']]],
    ]);
  });

  it('skips an event of an item the board does not hold', () => {
    expect(moves([event('2026-10-02T09:00:00Z', 'closed', 9)], (rid) => ids[rid])).toEqual([]);
  });
});
