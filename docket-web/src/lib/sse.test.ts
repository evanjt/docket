import { afterEach, describe, expect, it, vi } from 'vitest';
import { concerns, paced, parse, settled } from './sse';

describe('parse', () => {
  it('reads named events and keeps an unfinished one', () => {
    const { events, rest } = parse('event: hello\ndata: 0\n\nevent: change\ndata: 1\n\nevent: cha');
    expect(events).toEqual([
      { event: 'hello', data: '0' },
      { event: 'change', data: '1' },
    ]);
    expect(rest).toBe('event: cha');
  });

  it('skips keep-alive comments', () => {
    expect(parse(':\n\n').events).toEqual([]);
  });

  it('reads CRLF line ends', () => {
    expect(parse('event: change\r\ndata: 2\r\n\r\n').events).toEqual([{ event: 'change', data: '2' }]);
  });

  it('joins several data lines', () => {
    expect(parse('data: a\ndata: b\n\n').events).toEqual([{ event: 'message', data: 'a\nb' }]);
  });
});

describe('concerns', () => {
  it('ignores a change naming only other projects', () => {
    expect(concerns('{"n":3,"projects":["o/q"]}', 'o/p')).toBe(false);
  });

  it('takes a change naming the shown project, among others or alone', () => {
    expect(concerns('{"n":3,"projects":["o/q","o/p"]}', 'o/p')).toBe(true);
  });

  it('takes a change naming no project, or unreadable, as any project', () => {
    expect(concerns('{"n":3,"projects":[]}', 'o/p')).toBe(true);
    expect(concerns('3', 'o/p')).toBe(true);
  });

  it('takes every change on a page of no single project', () => {
    expect(concerns('{"n":3,"projects":["o/q"]}', null)).toBe(true);
  });
});

describe('paced', () => {
  afterEach(() => vi.useRealTimers());

  it('reloads at most once for ten changes within a minute', () => {
    vi.useFakeTimers();
    const fire = vi.fn();
    const request = paced(60_000, fire);
    for (let i = 0; i < 10; i++) {
      request();
      vi.advanceTimersByTime(5_000);
    }
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it('answers a change after a quiet minute at once and defers the next', () => {
    vi.useFakeTimers();
    const fire = vi.fn();
    const request = paced(60_000, fire);
    request();
    vi.advanceTimersByTime(0);
    expect(fire).toHaveBeenCalledTimes(1);
    request();
    vi.advanceTimersByTime(59_000);
    expect(fire).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1_000);
    expect(fire).toHaveBeenCalledTimes(2);
  });
});

describe('settled', () => {
  afterEach(() => vi.useRealTimers());

  it('fires once after a quiet second', () => {
    vi.useFakeTimers();
    const fire = vi.fn();
    const change = settled(1_000, 5_000, fire);
    change();
    change();
    vi.advanceTimersByTime(999);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it('refreshes at the ceiling when changes never stop', () => {
    vi.useFakeTimers();
    const fire = vi.fn();
    const change = settled(1_000, 5_000, fire);
    for (let t = 0; t < 2_000; t += 100) {
      change();
      vi.advanceTimersByTime(100);
    }
    expect(fire).not.toHaveBeenCalled();
    for (let t = 0; t < 3_000; t += 100) {
      change();
      vi.advanceTimersByTime(100);
    }
    expect(fire).toHaveBeenCalledTimes(1);
  });
});
