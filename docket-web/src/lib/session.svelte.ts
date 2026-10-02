import { Refused, api, post, setKey, storedKey } from './api';
import { refresh } from './live.svelte';
import type { ProjectRow, Whoami } from './types';

export const session = $state({
  key: storedKey(),
  me: null as Whoami | null,
  projects: [] as ProjectRow[],
  error: null as string | null,
  checking: false,
});

/** Tries a key against `/whoami`, keeping it only when the server knows it. */
export async function signIn(key: string | null): Promise<boolean> {
  if (!key) return false;
  session.checking = true;
  setKey(key);
  try {
    session.me = await api.whoami();
    session.projects = (await api.projects()).sort((a, b) => a.slug.localeCompare(b.slug));
    session.key = key;
    session.error = null;
    return true;
  } catch (e) {
    session.error = e instanceof Error ? e.message : String(e);
    if (e instanceof Refused && e.status === 401) {
      setKey(null);
      session.key = null;
    }
    return false;
  } finally {
    session.checking = false;
  }
}

export function signOut() {
  setKey(null);
  session.key = null;
  session.me = null;
  session.projects = [];
}

export async function reloadProjects() {
  try {
    session.projects = (await api.projects()).sort((a, b) => a.slug.localeCompare(b.slug));
  } catch {
    // The rail keeps what it had.
  }
}

export interface Toast {
  id: number;
  text: string;
  tone: 'ok' | 'refused';
}

export const toasts = $state<Toast[]>([]);
let next = 0;

export function toast(text: string, tone: Toast['tone'] = 'ok') {
  const id = ++next;
  toasts.push({ id, text, tone });
  setTimeout(() => {
    const at = toasts.findIndex((t) => t.id === id);
    if (at >= 0) toasts.splice(at, 1);
  }, tone === 'refused' ? 8000 : 3500);
}

/** One write verb. A refusal is shown in the server's words and changes nothing; a landed write reads the page again. */
export async function act<T = unknown>(verb: string, body: object, done: string): Promise<T | null> {
  try {
    const answer = await post<T>(verb, body);
    toast(done);
    refresh();
    return answer;
  } catch (e) {
    toast(e instanceof Error ? e.message : String(e), 'refused');
    return null;
  }
}

export function project(slug: string): ProjectRow | undefined {
  return session.projects.find((p) => p.slug === slug);
}
