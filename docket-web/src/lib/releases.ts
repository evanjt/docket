import type { Claim, Forecast, Problem, ReleaseCounts } from './types';
import { href } from './route';

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
  /** Items closed as done. */
  done: number;
  /** Items in the release, done and open; dropped ones are left out. */
  total: number;
  /** Items being worked now. */
  live: number;
  /** Open items by their word, and those a later release holds up. */
  words: Record<string, number>;
}

/** A release's row as `/metrics?scope=releases` counts it. */
export function releaseTally(m: ReleaseCounts, current: boolean): ReleaseRow {
  return {
    name: m.name,
    current,
    done: m.closed,
    total: m.closed + m.open,
    live: m.in_progress,
    words: { ready: m.ready, 'in progress': m.in_progress, 'plans under way': m.under_way, 'waiting on owner': m.waiting_owner, blocked: m.blocked, 'held later': m.held_later },
  };
}

/** The line a forecast prints: when the release clears, or that it is not converging. */
export function forecastLine(f: Forecast): string {
  if (f.open === 0) return 'nothing open';
  const dates = f.converging && f.p50 && f.p85 ? `clear by P50 ${f.p50}, P85 ${f.p85}` : 'not converging: closes do not outrun opens';
  const target = f.target === null || f.late === null ? '' : f.late ? `; late for the target ${f.target}` : `; on course for the target ${f.target}`;
  return `${f.open} open, ${dates}${target}`;
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

/** The problems of one release, as the check reports them. */
export interface ReleaseProblems {
  release: string;
  problems: Problem[];
}

/** The held-later problems of `/check` grouped by the release of the held item, the listed releases first in their order. */
export function checkByRelease(problems: Problem[], releases: string[]): ReleaseProblems[] {
  const groups = new Map<string, Problem[]>();
  for (const p of problems) {
    if (p.kind !== 'held_later' || !p.release) continue;
    groups.set(p.release, [...(groups.get(p.release) ?? []), p]);
  }
  const rank = (name: string) => {
    const at = releases.indexOf(name);
    return at < 0 ? releases.length : at;
  };
  return [...groups.entries()]
    .sort(([a], [b]) => rank(a) - rank(b))
    .map(([release, held]) => ({ release, problems: held }));
}

/** The held-later problems an item is in, as the held item or the one holding it. */
export function inversionsOf(problems: Problem[], id: string): Problem[] {
  return problems.filter((p) => p.kind === 'held_later' && (p.id === id || p.by === id));
}

/** The flag of each claim that has gone quiet, by item id. */
export function staleClaims(claims: Claim[] | undefined): Map<string, string> {
  return new Map((claims ?? []).filter((c) => c.flag).map((c) => [c.id, c.flag as string]));
}

const PHRASES: Record<string, [string, string]> = {
  integrity: ['integrity failure', 'integrity failures'],
  foreign_keys: ['foreign key violation', 'foreign key violations'],
  cycle: ['cycle', 'cycles'],
  held_later: ['release inversion', 'release inversions'],
  held_gate: ['hold on closed work', 'holds on closed work'],
  stale_wait: ['stale wait', 'stale waits'],
  no_body: ['item with no body', 'items with no body'],
};

/** One count of the check: its kind, the line `docket_core::check::phrase` words it as, and the Work list of its items. */
export interface CheckCount {
  kind: string;
  line: string;
  href: string;
}

/** The problems of `/check` counted per kind, in the order each kind first appears. */
export function checkCounts(problems: Problem[], slug: string): CheckCount[] {
  const counts = new Map<string, number>();
  for (const p of problems) counts.set(p.kind, (counts.get(p.kind) ?? 0) + 1);
  return [...counts.entries()].map(([kind, n]) => {
    const [one, many] = PHRASES[kind] ?? ['other problem', 'other problems'];
    return { kind, line: `${n} ${n === 1 ? one : many}`, href: href(slug, 'work', { check: kind }) };
  });
}

/** The ids of the items the problems of one kind name, each once, in order. */
export function checkIds(problems: Problem[], kind: string): string[] {
  const ids: string[] = [];
  for (const p of problems) if (p.kind === kind && p.id && !ids.includes(p.id)) ids.push(p.id);
  return ids;
}
