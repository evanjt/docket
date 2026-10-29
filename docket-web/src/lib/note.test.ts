import { describe, expect, it } from 'vitest';
import { highlight } from './note';

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
