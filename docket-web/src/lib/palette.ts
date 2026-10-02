import type { GraphNode } from './types';

/** How well a text answers what was typed: 0 for not at all, higher for a closer match. */
export function score(typed: string, text: string): number {
  const t = typed.trim().toLowerCase();
  const s = text.toLowerCase();
  if (!t) return 1;
  if (s === t) return 100;
  if (s.startsWith(t)) return 60;
  const at = s.indexOf(t);
  if (at >= 0) return 40 - Math.min(at, 30) / 10;
  const words = t.split(/\s+/);
  return words.every((w) => s.includes(w)) ? 20 : 0;
}

/** The items whose id or title answer what was typed, best first, open ones before closed. */
export function findItems(typed: string, nodes: Iterable<GraphNode>, limit = 8): GraphNode[] {
  const t = typed.trim();
  if (!t) return [];
  const scored: [number, GraphNode][] = [];
  for (const n of nodes) {
    const s = Math.max(score(t, n.id) * 1.5, score(t, n.title));
    if (s > 0) scored.push([s + (n.word === 'done' || n.word === 'dropped' ? 0 : 5), n]);
  }
  return scored.sort((a, b) => b[0] - a[0]).slice(0, limit).map(([, n]) => n);
}
