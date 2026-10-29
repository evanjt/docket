import { describe, expect, it } from 'vitest';
import { ended, highlight, landing } from './note';

describe('highlight', () => {
  it('splits a snippet into plain and matched parts', () => {
    expect(highlight('a [lantern] for')).toEqual([
      { text: 'a ', hit: false },
      { text: 'lantern', hit: true },
      { text: ' for', hit: false },
    ]);
  });

  it('keeps text with no markers as one plain part', () => {
    expect(highlight('nothing marked')).toEqual([{ text: 'nothing marked', hit: false }]);
  });

  it('marks several matches and a match at either end', () => {
    expect(highlight('[ore] and [ash]')).toEqual([
      { text: 'ore', hit: true },
      { text: ' and ', hit: false },
      { text: 'ash', hit: true },
    ]);
  });
});

describe('ended', () => {
  const at = Date.parse('2026-10-05T10:20:00Z') / 1000;

  it('reads the end of a claim\'s job as an age', () => {
    expect(ended({ claim_ended_at: '2026-10-05T10:20:00Z' }, at + 300)).toBe('ended 5m ago');
  });

  it('is empty while the job runs', () => {
    expect(ended({ claim_ended_at: null }, at)).toBe('');
    expect(ended({}, at)).toBe('');
  });
});

describe('landing', () => {
  const at = Date.parse('2026-10-05T10:20:00Z') / 1000;
  const land = (stage: 'queued' | 'gating' | 'gate_failed' | 'conflict', batch: string | null = null) => ({
    claim_land: { stage, at: '2026-10-05T10:08:00Z', note: null, batch },
  });

  it('words each stage with its age', () => {
    expect(landing(land('queued'), at)).toBe('queued 12m ago');
    expect(landing(land('gating', 'b7'), at)).toBe('gating 12m ago (batch b7)');
    expect(landing(land('gate_failed'), at)).toBe('gate failed 12m ago');
    expect(landing(land('conflict'), at)).toBe('conflict 12m ago');
  });

  it('is empty before a build is queued', () => {
    expect(landing({ claim_land: null }, at)).toBe('');
    expect(landing({}, at)).toBe('');
  });
});
