export const TABS = ['overview', 'work', 'plans', 'areas', 'yours', 'activity', 'settings'] as const;
export type Tab = (typeof TABS)[number];

export type Route =
  | { page: 'home' }
  | { page: 'project'; slug: string; tab: Tab }
  | { page: 'unknown'; path: string };

export const ROOT = '/ui/';

/**
 * The page a path under `/ui/` names. A slug may hold a slash, so the longest run of leading segments
 * that is a known slug is the project, and what follows it the tab.
 */
export function resolve(pathname: string, slugs: string[]): Route {
  const under = pathname === '/ui' ? ROOT : pathname;
  const rest = under.startsWith(ROOT) ? under.slice(ROOT.length) : under.replace(/^\/+/, '');
  const parts = rest.split('/').filter(Boolean).map(decodeURIComponent);
  if (parts.length === 0) return { page: 'home' };
  const known = new Set(slugs);
  for (let n = parts.length; n > 0; n--) {
    const slug = parts.slice(0, n).join('/');
    if (!known.has(slug)) continue;
    const after = parts.slice(n);
    if (after.length === 0) return { page: 'project', slug, tab: 'overview' };
    if (after.length === 1 && (TABS as readonly string[]).includes(after[0])) {
      return { page: 'project', slug, tab: after[0] as Tab };
    }
  }
  return { page: 'unknown', path: rest };
}

/** The path of a project's tab, with its query. */
export function href(slug: string, tab: Tab = 'overview', params: Record<string, string | undefined> = {}): string {
  const path = slug.split('/').map(encodeURIComponent).join('/');
  const base = tab === 'overview' ? `${ROOT}${path}` : `${ROOT}${path}/${tab}`;
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v) q.set(k, v);
  const s = q.toString();
  return s ? `${base}?${s}` : base;
}
