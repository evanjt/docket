import { describe, expect, it } from 'vitest';
import { jobLine, leadLine, machineUse } from './fleet';
import type { EventRow, LeadState, Machine, Row } from './types';

const machine = (name: string, slots: number): Machine => ({
  name, ssh: `user@${name}`, slots, runners: ['claude'], note: null, updated_at: '',
});
const NOW = Date.parse('2026-10-05T12:00:00Z') / 1000;
const claim = (id: string, on: string | null, by: string): Row =>
  ({ id, claim_on: on, claim_host: by, claim_branch: `lead/${id}` }) as unknown as Row;

const ended = (branch: string): EventRow =>
  ({ seq: 1, project: 'o/p', at: '', host: 'alpha', branch, kind: 'job_reported', data: { end: 'exited' } }) as EventRow;

describe('machineUse', () => {
  it('counts a claim against the machine its job runs on, not the host that made it', () => {
    const use = machineUse(
      [machine('alpha', 2), machine('beta', 4)],
      [claim('T1', 'beta', 'alpha'), claim('T2', 'beta', 'alpha'), claim('T3', null, 'alpha')],
      [],
      'alpha',
      NOW,
    );
    expect(use.map((u) => [u.name, u.used, u.slots, u.here])).toEqual([
      ['alpha', 0, 2, true],
      ['beta', 2, 4, false],
    ]);
  });
});

describe('machineUse waiting to land', () => {
  const row = (id: string): Row => claim(id, 'beta', 'alpha');

  it('counts a claim whose job reported its end as waiting to land, not as a used slot', () => {
    const claims = Array.from({ length: 10 }, (_, i) => row(`T${i}`));
    const reports = claims.map((c) => ended(c.claim_branch as string));
    const [use] = machineUse([machine('beta', 4)], claims, reports, 'alpha', NOW);
    expect([use.used, use.waiting]).toEqual([0, 10]);
  });

  it('keeps a claim with no reported end as used, and ignores an end reported for another branch', () => {
    const claims = [row('T1'), row('T2')];
    const [use] = machineUse([machine('beta', 4)], claims, [ended('lead/T1'), ended('lead/other')], 'alpha', NOW);
    expect([use.used, use.waiting]).toEqual([1, 1]);
  });
});

describe('machineUse limits', () => {
  const limited = { ...machine('alpha', 2), runners: ['claude', 'codex'], limits: { codex: '2026-10-07T18:29:00Z' } };

  it('lists a runner under a usage limit with its reset until the reset passes', () => {
    expect(machineUse([limited], [], [], 'alpha', NOW)[0].limited).toEqual([
      { runner: 'codex', until: '2026-10-07T18:29:00Z' },
    ]);
    expect(machineUse([limited], [], [], 'alpha', Date.parse('2026-10-07T18:29:00Z') / 1000)[0].limited).toEqual([]);
  });
});

describe('leadLine', () => {
  const now = Date.parse('2026-10-02T10:10:00Z') / 1000;
  const state = (lapsed: boolean): LeadState => ({
    project: 'o/p',
    lead: { project: 'o/p', host: 'alpha', session: 'lead-1', branch: 'main', since: '2026-10-02T10:00:00Z', renewed_at: '2026-10-02T10:05:00Z' },
    lapsed,
    lapses_at: '2026-10-02T10:15:00Z',
    lapse_minutes: 10,
  });

  it('says when no lead holds the project', () => {
    expect(leadLine({ project: 'o/p', lead: null, lapsed: false, lapses_at: null, lapse_minutes: 10 }, now))
      .toMatch(/^No lead/);
  });

  it('names a holder and when it renewed', () => {
    expect(leadLine(state(false), now)).toBe('Led by lead-1 on alpha, renewed 5m ago.');
  });

  it('says a lapsed lead lapsed', () => {
    expect(leadLine(state(true), now)).toBe('The lead lapsed: lead-1 on alpha last renewed 5m ago.');
  });
});

describe('jobLine', () => {
  it('names the runner and job of a dispatched claim and nothing for one by hand', () => {
    expect(jobLine({ claim_runner: 'codex', claim_job: 'lead-t1-3' } as unknown as Row)).toBe('codex lead-t1-3');
    expect(jobLine({} as Row)).toBe('');
  });
});
