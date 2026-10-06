import { describe, expect, it } from 'vitest';
import { checkByRelease, checkCounts, checkIds, forecastLine, holdsOf, inversionsOf, staleClaims, releaseList, releaseOf, releaseRow, releaseTally } from './releases';
import type { Problem, ReleaseCounts } from './types';

describe('releaseList', () => {
  it('reads the releases the server lists in order and nothing when there are none', () => {
    expect(releaseList({ releases: ' 1.0  1.1 2.0 ' })).toEqual(['1.0', '1.1', '2.0']);
    expect(releaseList({})).toEqual([]);
    expect(releaseList(undefined)).toEqual([]);
  });
});

describe('releaseOf', () => {
  const r = ['1.0', '1.1', '1.2'];
  it('is the release when listed, and nothing for the backlog or a release shipped', () => {
    expect(releaseOf('1.2', r)).toBe('1.2');
    expect(releaseOf(null, r)).toBeNull();
    expect(releaseOf('0.9', r)).toBeNull();
  });
  it('is nothing without releases', () => {
    expect(releaseOf('1.1', [])).toBeNull();
  });
});

const counts = (over: Partial<ReleaseCounts> = {}): ReleaseCounts => ({
  name: '1.0', open: 5, ready: 2, in_progress: 1, under_way: 3, waiting_owner: 1, blocked: 1, closed: 3, held_later: 1, pace: 0.5,
  forecast: { open: 5, burn: 0.5, converging: true, p50: null, p85: null, target: null, late: null },
  ...over,
});

describe('releaseTally', () => {
  it('prints the row of a release: closed of open and closed, live, and each word apart with held later', () => {
    expect(releaseTally(counts(), true)).toEqual({
      name: '1.0', current: true, done: 3, total: 8, live: 1,
      words: { ready: 2, 'in progress': 1, 'plans under way': 3, 'waiting on owner': 1, blocked: 1, 'held later': 1 },
    });
  });
});

describe('forecastLine', () => {
  it('prints the P50 date and a late target', () => {
    const forecast = { open: 12, burn: 1.5, converging: true, p50: '2026-10-20', p85: '2026-11-02', target: '2026-10-30', late: true };
    expect(forecastLine(forecast)).toBe('12 open, clear by P50 2026-10-20, P85 2026-11-02; late for the target 2026-10-30');
  });
  it('says not converging for a zero burn', () => {
    const forecast = { open: 12, burn: 0, converging: false, p50: null, p85: null, target: null, late: null };
    expect(forecastLine(forecast)).toBe('12 open, not converging: closes do not outrun opens');
  });
  it('says nothing is open for a cleared release', () => {
    const forecast = { open: 0, burn: 0, converging: true, p50: '2026-10-05', p85: '2026-10-05', target: null, late: null };
    expect(forecastLine(forecast)).toBe('nothing open');
  });
});

describe('releaseRow', () => {
  it('gives an item in a listed release a Release row', () => {
    expect(releaseRow('0.4', ['1.0', '0.4'])).toEqual({ name: '0.4', current: false });
  });
  it('marks the current release and gives the backlog no row', () => {
    expect(releaseRow('1.0', ['1.0', '0.4'])).toEqual({ name: '1.0', current: true });
    expect(releaseRow(null, ['1.0', '0.4'])).toBeNull();
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

const late = (id: string, by: string, release: string, later: string): Problem => ({ kind: 'held_later', id, by, release, later });

describe('checkByRelease', () => {
  it('groups the held-later problems by the release of the held item and leaves other kinds out', () => {
    const problems: Problem[] = [
      late('T1', 'T9', '1.0', '1.1'),
      { kind: 'cycle', id: 'T4' },
      late('T2', 'T9', '1.0', 'the backlog'),
      late('T5', 'T6', '1.1', '2.0'),
    ];
    expect(checkByRelease(problems, ['1.0', '1.1', '2.0'])).toEqual([
      { release: '1.0', problems: [problems[0], problems[2]] },
      { release: '1.1', problems: [problems[3]] },
    ]);
  });
  it('puts a release not listed after the listed ones, and nothing when there are no problems', () => {
    expect(checkByRelease([late('T1', 'T2', '0.9', '1.0')], ['1.0']).map((g) => g.release)).toEqual(['0.9']);
    expect(checkByRelease([], ['1.0'])).toEqual([]);
  });
});

describe('inversionsOf', () => {
  it('finds the problems an item is held in or holds in another release', () => {
    const a = late('T1', 'T9', '1.0', '1.1');
    const b = late('T9', 'T3', '1.1', '1.2');
    expect(inversionsOf([a, b, late('T7', 'T8', '1.0', '1.1')], 'T9')).toEqual([a, b]);
    expect(inversionsOf([a], 'T5')).toEqual([]);
  });
});

describe('staleClaims', () => {
  it('maps each flagged claim by its id', () => {
    const claims = [
      { id: 'T1', title: 'a', branch: 'b', host: 'h', since: 1, flag: 'no event for 3h' },
      { id: 'T2', title: 'b', branch: 'b', host: 'h', since: 1, flag: null },
    ];
    expect([...staleClaims(claims)]).toEqual([['T1', 'no event for 3h']]);
    expect(staleClaims(undefined).size).toBe(0);
  });
});

describe('checkCounts', () => {
  const p = (kind: string, id: string): Problem => ({ kind, id });
  it('counts each kind of problem in the words the CLI uses, each with the list it links to', () => {
    const problems = [...['a', 'b', 'c', 'd', 'e'].map((id) => p('cycle', id)), p('held_later', 'x'), p('held_later', 'y'), p('held_gate', 'z')];
    expect(checkCounts(problems, 'proj')).toEqual([
      { kind: 'cycle', line: '5 cycles', href: '/ui/proj/work?check=cycle' },
      { kind: 'held_later', line: '2 release inversions', href: '/ui/proj/work?check=held_later' },
      { kind: 'held_gate', line: '1 hold on closed work', href: '/ui/proj/work?check=held_gate' },
    ]);
  });
  it('names the items of one kind once each', () => {
    const problems = [p('cycle', 'a'), p('cycle', 'a'), p('cycle', 'b'), p('held_later', 'c')];
    expect(checkIds(problems, 'cycle')).toEqual(['a', 'b']);
  });
});
