import type { GraphNode } from './types';

/** The releases not yet shipped, as the server's `releases` fact lists them in order, the current first. */
export function releaseList(skills: Record<string, string> | undefined): string[] {
  return (skills?.releases ?? '').split(/\s+/).filter(Boolean);
}

/** The release an item is in when it is one not yet shipped; nothing for the backlog or a shipped release. */
export function releaseOf(release: string | null | undefined, releases: string[]): string | null {
  return release && releases.includes(release) ? release : null;
}

export interface ReleaseRow {
  name: string;
  current: boolean;
  /** Tickets closed as done. */
  done: number;
  /** Tickets in the release, done and open; dropped ones are left out. */
  total: number;
  /** Tickets being worked now. */
  live: number;
  /** Open tickets by their word. */
  words: Record<string, number>;
}

/** Each release with its tickets counted: a ticket is an item of the work kind. */
export function releaseRows(nodes: Iterable<GraphNode>, releases: string[]): ReleaseRow[] {
  const rows = releases.map((name, i) => ({ name, current: i === 0, done: 0, total: 0, live: 0, words: {} as Record<string, number> }));
  const at = new Map(rows.map((r) => [r.name, r]));
  for (const n of nodes) {
    if (n.kind !== 'work' || n.state === 'dropped') continue;
    const r = at.get(releaseOf(n.release, releases) ?? '');
    if (!r) continue;
    r.total++;
    if (n.state === 'done') {
      r.done++;
      continue;
    }
    if (n.word === 'building') r.live++;
    r.words[n.word] = (r.words[n.word] ?? 0) + 1;
  }
  return rows;
}

/** The Release row an item's detail shows, for every kind: its release and whether that is the current one. */
export function releaseRow(release: string | null | undefined, releases: string[]): { name: string; current: boolean } | null {
  const name = releaseOf(release, releases);
  return name === null ? null : { name, current: name === releases[0] };
}

/** The ids of the items an item holds, from a `/deps` reply. */
export function holdsOf(deps: { holds?: { id: string }[] } | undefined): string[] {
  return (deps?.holds ?? []).map((h) => h.id);
}
