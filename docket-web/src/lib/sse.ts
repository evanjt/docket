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

/**
 * Whether a `change` event's data is news to the project on screen. A change naming other projects
 * is not; one naming none, or that cannot be read, may be any project's, and `shown` null is a page
 * of no single project.
 */
export function concerns(data: string, shown: string | null): boolean {
  if (shown === null) return true;
  try {
    const projects: unknown = JSON.parse(data).projects;
    return !Array.isArray(projects) || projects.length === 0 || projects.includes(shown);
  } catch {
    return true;
  }
}

/**
 * Calls `fire` for the first request at once and for those after it at most once per `interval`
 * milliseconds: requests arriving inside the interval collapse into one call at its end.
 */
export function paced(interval: number, fire: () => void): () => void {
  let last = -Infinity;
  let timer: ReturnType<typeof setTimeout> | undefined;
  return () => {
    if (timer !== undefined) return;
    const wait = Math.max(0, last + interval - Date.now());
    timer = setTimeout(() => {
      timer = undefined;
      last = Date.now();
      fire();
    }, wait);
  };
}

/**
 * Calls `fire` once changes have been quiet for `quiet` ms, or `ceiling` ms after the first unanswered
 * change, so a constant stream of changes still fires.
 */
export function settled(quiet: number, ceiling: number, fire: () => void): () => void {
  let quietTimer: ReturnType<typeof setTimeout> | undefined;
  let ceilingTimer: ReturnType<typeof setTimeout> | undefined;
  const go = () => {
    clearTimeout(quietTimer);
    clearTimeout(ceilingTimer);
    quietTimer = ceilingTimer = undefined;
    fire();
  };
  return () => {
    clearTimeout(quietTimer);
    quietTimer = setTimeout(go, quiet);
    ceilingTimer ??= setTimeout(go, ceiling);
  };
}
