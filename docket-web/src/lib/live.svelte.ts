import { BASE, Refused, headers } from './api';
import { parse } from './sse';

/** Bumped after every change the server reports, and after every write from this page. */
export const live = $state({ version: 0, connected: false });

const SETTLE_MS = 250;
const RETRY_MS = [1000, 2000, 5000, 10000];

let timer: ReturnType<typeof setTimeout> | undefined;

/** Reads again everything on screen, once a burst of changes has settled. */
export function refresh() {
  clearTimeout(timer);
  timer = setTimeout(() => {
    clock.now = Date.now() / 1000;
    live.version++;
  }, SETTLE_MS);
}

/** Follows `/changes` until `signal` aborts, reconnecting after a drop. */
export async function follow(signal: AbortSignal) {
  let attempt = 0;
  while (!signal.aborted) {
    try {
      const resp = await fetch(`${BASE}/changes`, { headers: headers(), signal });
      if (!resp.ok || !resp.body) throw new Error(String(resp.status));
      const reader = resp.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = '';
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        const { events, rest } = parse(buffer + value);
        buffer = rest;
        for (const e of events) {
          if (e.event === 'hello') {
            if (!live.connected && attempt > 0) refresh();
            live.connected = true;
            attempt = 0;
          } else if (e.event === 'change') {
            refresh();
          }
        }
      }
    } catch {
      // Dropped or refused: wait and connect again.
    }
    live.connected = false;
    if (signal.aborted) return;
    const wait = RETRY_MS[Math.min(attempt, RETRY_MS.length - 1)];
    attempt++;
    await new Promise((r) => setTimeout(r, wait));
  }
}

export interface Resource<T> {
  readonly data: T | undefined;
  readonly error: string | null;
  readonly loading: boolean;
}

/**
 * What `fetcher` answers, read again whenever its reactive inputs or `live.version` move. The last answer
 * stays shown while the next is read. A fetcher answering `null` reads nothing.
 */
export function resource<T>(fetcher: () => Promise<T> | null): Resource<T> {
  const state = $state<{ data: T | undefined; error: string | null; loading: boolean }>({
    data: undefined,
    error: null,
    loading: true,
  });
  let ticket = 0;
  $effect(() => {
    void live.version;
    const pending = fetcher();
    const mine = ++ticket;
    if (!pending) {
      state.loading = false;
      return;
    }
    state.loading = true;
    pending.then(
      (d) => {
        if (mine !== ticket) return;
        state.data = d;
        state.error = null;
        state.loading = false;
      },
      (e: unknown) => {
        if (mine !== ticket) return;
        state.error = e instanceof Refused || e instanceof Error ? e.message : String(e);
        state.loading = false;
      },
    );
  });
  return state;
}

/** The time the page reads ages against, moved on each half minute. */
export const clock = $state({ now: Date.now() / 1000 });

export function tick(): () => void {
  const timer = setInterval(() => (clock.now = Date.now() / 1000), 30_000);
  return () => clearInterval(timer);
}
