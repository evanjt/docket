import { byId } from './flow';
import { releaseOf } from './releases';
import { ROOT } from './route';

const STATES = ['open', 'done', 'dropped'];
export const SORTS = [
  { name: '', label: 'Queue order' },
  { name: 'id', label: 'Id' },
  { name: 'priority', label: 'Priority' },
  { name: 'updated', label: 'Recently updated' },
  { name: 'opened', label: 'Recently opened' },
];

/** The one filter state of a list, kept in the query string; an empty string leaves a field unset. */
export interface Filter {
  /** The key letters of an item, as `B` or `T`. */
  key: string;
  /** The lowest priority wanted: `high` keeps critical and high. */
  priority: string;
  complexity: string;
  release: string;
  /** The plan, story or concept whose items are wanted. */
  under: string;
  state: string;
  sort: string;
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
    state: choice(params.get('state'), STATES),
    sort: choice(params.get('sort'), SORTS.map((s) => s.name)),
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

/** What the stored lists take of a filter: the release, which `/done`, `/dropped` and `/derived` apply before the page is cut, and the plan, story or concept the rows lie under. */
export function pagedParams(f: Filter) {
  return { release: f.release || undefined, under: f.under || undefined };
}

/** What `/search` takes of a filter; the rest is applied to the rows. */
export function searchParams(f: Filter) {
  return { key: f.key || undefined, state: f.state || undefined, under: f.under || undefined };
}

export interface Filterable {
  id: string;
  priority?: string;
  complexity?: string | null;
  state?: string;
  release?: string | null;
}

/** Whether a row passes every field of the filter, for the routes that do not take them all. */
export function applies(r: Filterable, f: Filter, releases: string[], priorities: readonly string[]): boolean {
  if (f.key && !new RegExp(`^${f.key}\\d`, 'i').test(r.id)) return false;
  if (f.priority && r.priority !== undefined && priorities.indexOf(r.priority) > priorities.indexOf(f.priority)) return false;
  if (f.complexity && r.complexity !== f.complexity) return false;
  if (f.state && r.state !== undefined && r.state !== f.state) return false;
  if (f.release && releaseOf(r.release, releases) !== f.release) return false;
  return true;
}

/** The rows in a sort; the queue order is the order the server gave. */
export function sortRows<T extends { id: string; priority?: string; updated_at?: string; opened_at?: string }>(rows: T[], sort: string, priorities: readonly string[]): T[] {
  const tier = (r: T) => priorities.indexOf(r.priority ?? 'normal');
  const newest = (a?: string, z?: string) => (z ?? '').localeCompare(a ?? '');
  if (sort === 'id') return [...rows].sort((a, z) => byId(a.id, z.id));
  if (sort === 'priority') return [...rows].sort((a, z) => tier(a) - tier(z) || byId(a.id, z.id));
  if (sort === 'updated') return [...rows].sort((a, z) => newest(a.updated_at, z.updated_at) || byId(a.id, z.id));
  if (sort === 'opened') return [...rows].sort((a, z) => newest(a.opened_at, z.opened_at) || byId(a.id, z.id));
  return rows;
}

/** The work page listing what is left to do under a plan, story or concept. */
export function remainingHref(slug: string, under: string): string {
  const path = slug.split('/').map(encodeURIComponent).join('/');
  return `${ROOT}${path}/work?list=next&under=${encodeURIComponent(under)}`;
}
