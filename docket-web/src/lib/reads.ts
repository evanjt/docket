/** Starts a read and aborts the one before it; `stop` aborts the last. */
export function supersede(): { (): AbortSignal; stop: () => void } {
  let current: AbortController | undefined;
  const next = () => {
    current?.abort();
    current = new AbortController();
    return current.signal;
  };
  next.stop = () => current?.abort();
  return next;
}

/** Whether `e` is a read being aborted, which is not a failure to show. */
export function aborted(e: unknown): boolean {
  return e instanceof DOMException && e.name === 'AbortError';
}

let ambient: AbortSignal | undefined;

/** Runs `start`, which begins reads that stop when `signal` aborts: a read begun inside it takes `signal`. */
export function reading<T>(signal: AbortSignal, start: () => T): T {
  const outer = ambient;
  ambient = signal;
  try {
    return start();
  } finally {
    ambient = outer;
  }
}

/** The signal of the `reading` call this one runs inside, if any. */
export function current(): AbortSignal | undefined {
  return ambient;
}
