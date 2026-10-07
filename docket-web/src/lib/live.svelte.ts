import { BASE, Refused, headers } from './api';
import { here } from './here.svelte';
import { reading, aborted, supersede } from './reads';
import { concerns, paced, parse, settled } from './sse';

/** Bumped after every change the server reports, and after every write from this page. */
export const live = $state({ version: 0, graph: 0, connected: false });

const SETTLE_MS = 1000;
const CEILING_MS = 5000;
const GRAPH_MS = 60_000;
const RETRY_MS = [1000, 2000, 5000, 10000];

const reloadGraph = paced(GRAPH_MS, () => live.graph++);

/** Reads again everything on screen, once a burst of changes has settled, and at least every `CEILING_MS`. */
export const refresh = settled(SETTLE_MS, CEILING_MS, () => {
  clock.now = Date.now() / 1000;
  live.version++;
  reloadGraph();
});

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
          } else if (e.event === 'change' && concerns(e.data, here.slug)) {
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
 * `version` is the counter whose moves read it again.
 */
export function resource<T>(
  fetcher: () => Promise<T> | null,
  version: () => number = () => live.version,
): Resource<T> {
  const state = $state<{ data: T | undefined; error: string | null; loading: boolean }>({
    data: undefined,
    error: null,
    loading: true,
  });
  let ticket = 0;
  const next = supersede();
  $effect(() => {
    void version();
    const signal = next();
    const pending = reading(signal, fetcher);
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
        if (mine !== ticket || aborted(e)) return;
        state.error = e instanceof Refused || e instanceof Error ? e.message : String(e);
        state.loading = false;
      },
    );
    return next.stop;
  });
  return state;
}

/** The time the page reads ages against, moved on each half minute. */
export const clock = $state({ now: Date.now() / 1000 });

export function tick(): () => void {
  const timer = setInterval(() => (clock.now = Date.now() / 1000), 30_000);
  return () => clearInterval(timer);
}
