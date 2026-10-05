import type {
  Offers,
  Context, Count, Derived, EventRow, Facts, Graph, LeadState, Machine, ProjectRow, Row, Shown, Status, Whoami,
} from './types';

const KEY = 'docket.key';
/** The API's root: the dev server proxies /api to the server, a build is served by the server itself. */
export const BASE = import.meta.env.DEV ? '/api' : '';

export class Refused extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

export function storedKey(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function storeKey(key: string | null) {
  try {
    if (key) localStorage.setItem(KEY, key);
    else localStorage.removeItem(KEY);
  } catch {
    // A browser refusing storage keeps the key for this tab only.
  }
}

let key = storedKey();

export function setKey(k: string | null) {
  key = k;
  storeKey(k);
}

export function headers(): Record<string, string> {
  return key ? { Authorization: `Bearer ${key}` } : {};
}

/** The words of a refusal: `refused` on a 409, `error` on the rest. */
export function reason(status: number, text: string): string {
  try {
    const body = JSON.parse(text);
    if (typeof body?.refused === 'string') return body.refused;
    if (typeof body?.error === 'string') return body.error;
  } catch {
    // Not JSON: the text itself.
  }
  if (status === 401) return 'the server does not know this key';
  return text.trim() || `the server answered ${status}`;
}

async function answer<T>(resp: Response): Promise<T> {
  const text = await resp.text();
  if (!resp.ok) throw new Refused(resp.status, reason(resp.status, text));
  return JSON.parse(text) as T;
}

export function query(params: Record<string, string | number | undefined | null>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== '') q.set(k, String(v));
  }
  const s = q.toString();
  return s ? `?${s}` : '';
}

export async function get<T>(path: string, params: Record<string, string | number | undefined | null> = {}): Promise<T> {
  let resp: Response;
  try {
    resp = await fetch(`${BASE}${path}${query(params)}`, { headers: headers() });
  } catch {
    throw new Refused(0, 'the server could not be reached');
  }
  return answer<T>(resp);
}

/** A write verb, `POST /do/{verb}`. */
export async function post<T = unknown>(verb: string, body: object): Promise<T> {
  let resp: Response;
  try {
    resp = await fetch(`${BASE}/do/${verb}`, {
      method: 'POST',
      headers: { ...headers(), 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    throw new Refused(0, 'the server could not be reached');
  }
  return answer<T>(resp);
}

type Params = Record<string, string | undefined>;

const of = (project: string) => ({ project });

/** The most rows one page of a stored list carries. */
const PAGE = 1000;

/** Every row of a stored list matching a filter, a page at a time: the list routes page by default. */
export async function all<T>(path: string, filter: object, sort: string, most = Infinity): Promise<T[]> {
  const out: T[] = [];
  while (out.length < most) {
    const page = await get<T[]>(path, {
      filter: JSON.stringify(filter),
      sort,
      range: `[${out.length},${out.length + PAGE - 1}]`,
    });
    out.push(...page);
    if (page.length < PAGE) return out;
  }
  return out;
}

export const api = {
  whoami: () => get<Whoami>('/whoami'),
  projects: () => all<ProjectRow>('/projects', {}, '["slug","ASC"]'),
  counts: () => get<Count[]>('/counts'),
  status: (p: string) => get<Status>('/status', of(p)),
  next: (p: string, n = 100, filter: Params = {}) => get<Row[]>('/next', { project: p, n, ...filter }),
  list: (route: string, p: string, n?: number, filter: Params = {}) => get<Row[]>(`/${route}`, { project: p, n, ...filter }),
  derived: (p: string, n?: number, filter: Params = {}) => get<Derived[]>('/derived', { project: p, n, ...filter }),
  search: (p: string, q: string, n = 200, filter: Params = {}) => get<Row[]>('/search', { project: p, q, n, state: 'any', ...filter }),
  show: (p: string, id: string) => get<Shown>(`/show/${encodeURIComponent(id)}`, of(p)),
  offers: (p: string, id: string) => get<Offers>(`/offers/${encodeURIComponent(id)}`, of(p)),
  log: (p: string, id: string) => get<EventRow[]>(`/log/${encodeURIComponent(id)}`, of(p)),
  context: (p: string, id: string) => get<Context>(`/context/${encodeURIComponent(id)}`, of(p)),
  graph: (p: string) => get<Graph>('/graph', of(p)),
  facts: (p: string) => get<Facts>('/facts', of(p)),
  machines: () => get<{ machines: Machine[] }>('/machines').then((m) => m.machines),
  lead: (p: string) => get<LeadState>('/lead', of(p)),
  events: (p: string, n: number, kinds?: string[]) =>
    get<EventRow[]>('/events', {
      filter: JSON.stringify(kinds ? { project: p, kind: kinds } : { project: p }),
      sort: '["seq","DESC"]',
      range: `[0,${Math.max(n, 1) - 1}]`,
    }),
  /** A project's events of some kinds at or after a stamp, newest first, `most` at the outside. */
  since: (p: string, at: string, kinds: string[], most: number) =>
    all<EventRow>('/events', { project: p, kind: kinds, at_gte: at }, '["seq","DESC"]', most),
};
