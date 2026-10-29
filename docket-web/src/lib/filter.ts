import { byId, wordRank } from './flow';
import { releaseOf } from './releases';
import { ROOT, href } from './route';

const STATES = ['open', 'done', 'dropped'];
export const SORTS = [
  { name: '', label: 'Queue order' },
  { name: 'id', label: 'Id' },
  { name: 'priority', label: 'Priority' },
  { name: 'updated', label: 'Recently updated' },
  { name: 'opened', label: 'Recently opened' },
  { name: 'word', label: 'Status word' },
];

export const GROUPS = [
  { name: '', label: 'No grouping' },
  { name: 'word', label: 'Group by status word' },
  { name: 'release', label: 'Group by release' },
  { name: 'area', label: 'Group by area' },
  { name: 'plan', label: 'Group by plan' },
];

/** The one filter state of a list, kept in the query string; an empty string leaves a field unset. */
export interface Filter {
  /** The key letters of an item, as `B` or `T`. */
  key: string;
  /** The lowest priority wanted: `high` keeps critical and high. */
  priority: string;
  complexity: string;
  release: string;
  /** The plan or story whose items are wanted. */
  under: string;
  /** The area's name; the routes do not take it, so rows are cut by it. */
  area: string;
  state: string;
  /** The status word, as `ready` or `in progress`. */
  word: string;
  sort: string;
  /** What the rows are grouped by: a name of `GROUPS`, or none. */
  group: string;
}

/** The server's lists of tiers and levels; both are empty until they are read. */
export interface Scale {
  priorities: readonly string[];
  levels: readonly string[];
}

/** A value among the options; any value while the options are not read yet. */
const choice = (v: string | null, options: readonly string[]) => (v && (!options.length || options.includes(v)) ? v : '');

export function parseFilter(params: URLSearchParams, scale: Scale): Filter {
  return {
    key: (params.get('key') ?? '').replace(/[^A-Za-z]/g, ''),
    priority: choice(params.get('priority'), scale.priorities),
    complexity: choice(params.get('complexity'), scale.levels),
    release: params.get('release') ?? '',
    under: params.get('under') ?? '',
    area: params.get('area') ?? '',
    state: choice(params.get('state'), STATES),
    word: params.get('word') ?? '',
    sort: choice(params.get('sort'), SORTS.map((s) => s.name)),
    group: choice(params.get('group'), GROUPS.map((g) => g.name)),
  };
}

/** The query parameters to write for a filter, `undefined` where a field is unset. */
export function filterChanges(f: Filter): Record<string, string | undefined> {
  return Object.fromEntries(Object.entries(f).map(([k, v]) => [k, v || undefined]));
}

/** What `/next` takes of a filter; its release and state are applied to the rows. */
export function nextParams(f: Filter) {
  return { key: f.key || undefined, priority: f.priority || undefined, complexity: f.complexity || undefined, under: f.under || undefined };
}

/** What the stored lists take of a filter: the release, which `/done`, `/dropped` and `/derived` apply before the page is cut, and the plan or story the rows lie under. */
export function pagedParams(f: Filter) {
  return { release: f.release || undefined, under: f.under || undefined };
}

/** What `/search` takes of a filter: every field but the sort. */
export function searchParams(f: Filter) {
  return {
    key: f.key || undefined,
    state: f.state || undefined,
    under: f.under || undefined,
    word: f.word || undefined,
    release: f.release || undefined,
    area: f.area || undefined,
    priority: f.priority || undefined,
    complexity: f.complexity || undefined,
  };
}

/** The Work page address of a search over a filter; the words are left out when blank. */
export function searchHref(slug: string, text: string, f: Filter): string {
  return href(slug, 'work', { ...filterChanges(f), q: text.trim() || undefined });
}

export interface Filterable {
  id: string;
  priority?: string;
  complexity?: string | null;
  state?: string;
  release?: string | null;
  area?: string | null;
}

/** Whether a row passes every field of the filter, for the routes that do not take them all. */
export function applies(r: Filterable, f: Filter, releases: string[], priorities: readonly string[]): boolean {
  if (f.key && !new RegExp(`^${f.key}\\d`, 'i').test(r.id)) return false;
  if (f.priority && r.priority !== undefined && priorities.indexOf(r.priority) > priorities.indexOf(f.priority)) return false;
  if (f.complexity && r.complexity !== f.complexity) return false;
  if (f.state && r.state !== undefined && r.state !== f.state) return false;
  if (f.area && r.area?.toLowerCase() !== f.area.toLowerCase()) return false;
  if (f.release && releaseOf(r.release, releases) !== f.release) return false;
  return true;
}

/** The rows in a sort; the queue order is the order the server gave. */
export function sortRows<T extends { id: string; word?: string; priority?: string; updated_at?: string; opened_at?: string }>(rows: T[], sort: string, priorities: readonly string[]): T[] {
  const tier = (r: T) => priorities.indexOf(r.priority ?? 'normal');
  const newest = (a?: string, z?: string) => (z ?? '').localeCompare(a ?? '');
  if (sort === 'id') return [...rows].sort((a, z) => byId(a.id, z.id));
  if (sort === 'priority') return [...rows].sort((a, z) => tier(a) - tier(z) || byId(a.id, z.id));
  if (sort === 'updated') return [...rows].sort((a, z) => newest(a.updated_at, z.updated_at) || byId(a.id, z.id));
  if (sort === 'opened') return [...rows].sort((a, z) => newest(a.opened_at, z.opened_at) || byId(a.id, z.id));
  if (sort === 'word') return [...rows].sort((a, z) => wordRank(a.word) - wordRank(z.word));
  return rows;
}

export interface Grouped<T> {
  /** The word, release, area or plan; empty for rows with none, and for the one group of an ungrouped list. */
  name: string;
  rows: T[];
}

/**
 * The rows in groups, each group keeping the order the rows came in. Words come in flow order and
 * releases in release order; areas and plans by name. Rows with no value form the last group.
 */
export function groupRows<T extends { id: string; word?: string; release?: string | null; area?: string | null; plan?: string | null }>(rows: T[], group: string, releases: readonly string[]): Grouped<T>[] {
  const of = (r: T): string =>
    (group === 'word' ? r.word : group === 'release' ? r.release : group === 'area' ? r.area : group === 'plan' ? r.plan : '') ?? '';
  const by = new Map<string, T[]>();
  for (const r of rows) {
    const name = of(r);
    const held = by.get(name);
    if (held) held.push(r);
    else by.set(name, [r]);
  }
  const rank = (name: string): number => (group === 'word' ? wordRank(name) : group === 'release' ? releases.indexOf(name) : 0);
  const names = [...by.keys()].sort((a, z) => {
    if (!a || !z) return Number(!a) - Number(!z);
    if (group === 'plan') return byId(a, z);
    return rank(a) - rank(z) || a.localeCompare(z);
  });
  return names.map((name) => ({ name, rows: by.get(name) ?? [] }));
}

/** The work page listing what is left to do under a plan or story. */
export function remainingHref(slug: string, under: string): string {
  const path = slug.split('/').map(encodeURIComponent).join('/');
  return `${ROOT}${path}/work?list=next&under=${encodeURIComponent(under)}`;
}
