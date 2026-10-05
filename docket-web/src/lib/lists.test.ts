import { describe, expect, it } from 'vitest';
import { caption, countOf, pageSize } from './lists';

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
