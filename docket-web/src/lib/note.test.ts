import { describe, expect, it } from 'vitest';
import { ended, highlight } from './note';

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
