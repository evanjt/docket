export interface SseEvent {
  event: string;
  data: string;
}

/** The complete events in a buffer of `text/event-stream`, and what is left of an event not yet ended. */
export function parse(buffer: string): { events: SseEvent[]; rest: string } {
  const text = buffer.replace(/\r\n?/g, '\n');
  const blocks = text.split('\n\n');
  const rest = blocks.pop() ?? '';
  const events: SseEvent[] = [];
  for (const block of blocks) {
    let event = 'message';
    const data: string[] = [];
    for (const line of block.split('\n')) {
      if (line === '' || line.startsWith(':')) continue;
      const at = line.indexOf(':');
      const field = at < 0 ? line : line.slice(0, at);
      const value = at < 0 ? '' : line.slice(at + 1).replace(/^ /, '');
      if (field === 'event') event = value;
      else if (field === 'data') data.push(value);
    }
    if (data.length || event !== 'message') events.push({ event, data: data.join('\n') });
  }
  return { events, rest };
}
