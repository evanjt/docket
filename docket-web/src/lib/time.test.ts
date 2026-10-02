import { describe, expect, it } from 'vitest';
import { duration } from './time';
import { reason } from './api';

describe('duration', () => {
  it('words each span the way the TUI does', () => {
    expect(duration(0)).toBe('0s');
    expect(duration(59)).toBe('59s');
    expect(duration(60 * 12)).toBe('12m');
    expect(duration(3600 * 3 + 60 * 5)).toBe('3h 05m');
    expect(duration(86400 * 4 + 3600 * 2)).toBe('4d 02h');
  });

  it('reads a span in the past as zero', () => {
    expect(duration(-5)).toBe('0s');
  });
});

describe('reason', () => {
  it('reads a refusal and an error', () => {
    expect(reason(409, '{"refused":"T5 is claimed on main"}')).toBe('T5 is claimed on main');
    expect(reason(404, '{"error":"no item T9 in o/p"}')).toBe('no item T9 in o/p');
  });

  it('words an unknown key and an empty answer', () => {
    expect(reason(401, '')).toBe('the server does not know this key');
    expect(reason(502, '')).toBe('the server answered 502');
  });
});
