import { describe, expect, it } from 'vitest';
import { board } from './flow';
import { caption, countOf, pageSize, queueNodes, wordList } from './lists';
import type { GraphNode } from './types';

describe('caption', () => {
  it('reads the rows shown against the total the server counts', () => {
    expect(caption(20, 3752)).toBe('20 of 3752');
  });

  it('is the bare count when everything is shown or the total is unknown', () => {
    expect(caption(12, 12)).toBe('12');
    expect(caption(12, null)).toBe('12');
  });
});

describe('countOf', () => {
  const status = { by_word: { done: 3752, dropped: 40, ready: 5 } };
  it('takes the total of a list from the status counts', () => {
    expect(countOf('done', status)).toBe(3752);
    expect(countOf('dropped', status)).toBe(40);
    expect(countOf('derived', status)).toBeNull();
    expect(countOf('done', undefined)).toBeNull();
  });
});

describe('pageSize', () => {
  it('grows by a page per step and reads everything under a release', () => {
    expect(pageSize(0, false)).toBe(100);
    expect(pageSize(2, false)).toBe(300);
    expect(pageSize(0, true)).toBe(5000);
  });
});

describe('wordList', () => {
  it('sends a word to the list route that carries its reasons and priorities', () => {
    expect(wordList('building')).toBe('wip');
    expect(wordList('ready')).toBe('next');
    expect(wordList('blocked')).toBe('waiting');
    expect(wordList('parked')).toBe('todo');
    expect(wordList('done')).toBe('done');
    expect(wordList('dropped')).toBe('dropped');
    expect(wordList('checking')).toBeNull();
  });
});

describe('queueNodes', () => {
  const node = (id: string, kind: GraphNode['kind'], word: string): GraphNode => ({
    id, rid: 0, key: id.replace(/\d+/, ''), kind, state: 'open', theme: null, title: id, word,
  });

  it('counts a claimed ticket and leaves out a package whose tickets are open', () => {
    const b = board({
      project: 'o/p',
      nodes: [node('T1', 'work', 'building'), node('P1', 'package', 'building'), node('A1', 'audit', 'building'), node('CON1', 'concept', 'building')],
      edges: [],
    });
    expect(queueNodes(b, 'building').map((n) => n.id)).toEqual(['T1']);
  });
});
