import { describe, expect, it } from 'vitest';
import { jobLine, leadLine, machineUse } from './fleet';
import type { LeadState, Machine, Row } from './types';

const machine = (name: string, slots: number): Machine => ({
  name, ssh: `user@${name}`, slots, runners: ['claude'], note: null, updated_at: '',
});
const NOW = Date.parse('2026-10-05T12:00:00Z') / 1000;
const claim = (id: string, on: string | null, by: string): Row =>
  ({ id, claim_on: on, claim_host: by, claim_branch: `lead/${id}` }) as unknown as Row;

describe('machineUse', () => {
  it('counts a claim against the machine its job runs on, not the host that made it', () => {
    const use = machineUse(
      [machine('alpha', 2), machine('beta', 4)],
      [claim('T1', 'beta', 'alpha'), claim('T2', 'beta', 'alpha'), claim('T3', null, 'alpha')],
      'alpha',
      NOW,
    );
    expect(use.map((u) => [u.name, u.used, u.slots, u.here])).toEqual([
      ['alpha', 0, 2, true],
      ['beta', 2, 4, false],
    ]);
  });
});

describe('machineUse limits', () => {
  const limited = { ...machine('alpha', 2), runners: ['claude', 'codex'], limits: { codex: '2026-10-07T18:29:00Z' } };

  it('lists a runner under a usage limit with its reset until the reset passes', () => {
    expect(machineUse([limited], [], 'alpha', NOW)[0].limited).toEqual([
      { runner: 'codex', until: '2026-10-07T18:29:00Z' },
    ]);
    expect(machineUse([limited], [], 'alpha', Date.parse('2026-10-07T18:29:00Z') / 1000)[0].limited).toEqual([]);
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
