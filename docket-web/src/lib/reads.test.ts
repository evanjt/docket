import { describe, expect, it } from 'vitest';
import { Refused } from './api';
import { aborted, supersede } from './reads';

describe('supersede', () => {
  it('aborts the previous read when the next starts', () => {
    const next = supersede();
    const first = next();
    expect(first.aborted).toBe(false);
    const second = next();
    expect(first.aborted).toBe(true);
    expect(second.aborted).toBe(false);
  });

  it('aborts the last read when stopped', () => {
    const next = supersede();
    const read = next();
    next.stop();
    expect(read.aborted).toBe(true);
  });
});

describe('aborted', () => {
  it('knows an abort from a refusal', () => {
    expect(aborted(new DOMException('x', 'AbortError'))).toBe(true);
    expect(aborted(new Refused(500, 'x'))).toBe(false);
    expect(aborted(new Error('x'))).toBe(false);
  });
});
