import { epoch } from './time';
import type { Count } from './types';

const KEY = 'docket.projectOrder';

export type Order = 'recent' | 'organisation';

const stamp = (slug: string, last: Map<string, number | null>) => last.get(slug) ?? null;

/** Slugs newest `last_event` first; those never moved, or without counts, last; ties by slug. */
export function byRecent(slugs: string[], counts: Count[]): string[] {
  const last = new Map(counts.map((c) => [c.slug, epoch(c.last_event)]));
  return [...slugs].sort((a, b) => {
    const x = stamp(a, last);
    const y = stamp(b, last);
    if (x === y) return a.localeCompare(b);
    if (x === null) return 1;
    if (y === null) return -1;
    return y - x;
  });
}

/**
 * Slugs under the part before their slash, each group ordered by `byRecent`, the groups by their
 * newest project, the prefix-less group last.
 */
export function byOrganisation(slugs: string[], counts: Count[]): [string, string[]][] {
  const groups = new Map<string, string[]>();
  for (const slug of slugs) {
    const at = slug.indexOf('/');
    const owner = at > 0 ? slug.slice(0, at) : '';
    groups.set(owner, [...(groups.get(owner) ?? []), slug]);
  }
  const ordered = [...groups].map(([owner, members]) => [owner, byRecent(members, counts)] as [string, string[]]);
  const head = byRecent(
    ordered.map(([, m]) => m[0]),
    counts,
  );
  return ordered.sort(([ao, am], [bo, bm]) => {
    if (ao === '' || bo === '') return ao === '' ? 1 : -1;
    return head.indexOf(am[0]) - head.indexOf(bm[0]);
  });
}

export function loadOrder(storage: Pick<Storage, 'getItem'> = localStorage): Order {
  try {
    const v = storage.getItem(KEY);
    if (v === 'organisation') return v;
  } catch {
    // Storage refused: show the recent order.
  }
  return 'recent';
}

export function saveOrder(order: Order, storage: Pick<Storage, 'setItem'> = localStorage) {
  try {
    storage.setItem(KEY, order);
  } catch {
    // Storage refused: the choice holds for this tab.
  }
}
