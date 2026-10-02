import { describe, expect, it } from 'vitest';
import { parse } from './sse';

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
