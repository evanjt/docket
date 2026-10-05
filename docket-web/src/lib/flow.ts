import type { EventRow, Graph, GraphNode, Kind, Progress } from './types';
import { day, epoch } from './time';

/** The flow read left to right, then the words that sit beside it. */
export const FLOW = ['ready', 'building', 'checking'] as const;
export const ASIDE = ['blocked', 'parked'] as const;
export const CLOSED = ['done', 'dropped'];

/** What each word means, as the TUI's help words it. */
export const MEANING: Record<string, string> = {
  ready: 'waiting for an agent to take it',
  building: 'a job or a person is working on it now, or a plan whose tickets are open',
  checking: 'an old package under its review',
  blocked: 'waiting on another item or a condition',
  parked: 'waiting on you: something only you can do',
  done: 'closed',
  dropped: 'dropped',
  standing: 'a concept or idea, open for good',
};

/** Tiers and levels in order, for the filters, the sort and the new-item form; the item panel reads the server's. */
export const PRIORITIES = ['critical', 'high', 'normal', 'low'] as const;
export const LEVELS = ['high', 'medium', 'low'] as const;

/** The groupings a plans page shows, plans first: the simple model keeps only plans in the queue. */
export const GROUPINGS: { kind: Kind; title: string }[] = [
  { kind: 'audit', title: 'Plans' },
  { kind: 'story', title: 'Stories' },
  { kind: 'package', title: 'Packages' },
  { kind: 'concept', title: 'Concepts' },
  { kind: 'idea', title: 'Central ideas' },
];

export function isClosed(word: string): boolean {
  return CLOSED.includes(word);
}

export interface Board {
  nodes: Map<string, GraphNode>;
  byRid: Map<number, GraphNode>;
  /** Items each item opened: the edges `child opened parent`, read from the parent. */
  children: Map<string, string[]>;
  /** What opened each item. */
  parents: Map<string, string[]>;
  related: Map<string, string[]>;
  cites: Map<string, string[]>;
}

function push(m: Map<string, string[]>, k: string, v: string) {
  const list = m.get(k);
  if (list) {
    if (!list.includes(v)) list.push(v);
  } else m.set(k, [v]);
}

export function board(g: Graph): Board {
  const nodes = new Map(g.nodes.map((n) => [n.id, n]));
  const byRid = new Map<number, GraphNode>();
  for (const n of g.nodes) if (typeof n.rid === 'number') byRid.set(n.rid, n);
  const b: Board = { nodes, byRid, children: new Map(), parents: new Map(), related: new Map(), cites: new Map() };
  for (const e of g.edges) {
    if (e.kind === 'opened') {
      push(b.children, e.to, e.from);
      push(b.parents, e.from, e.to);
    } else if (e.kind === 'related') {
      push(b.related, e.from, e.to);
      push(b.related, e.to, e.from);
    } else if (e.kind === 'cites') {
      push(b.cites, e.from, e.to);
    }
  }
  return b;
}

export type Tally = Progress;

const NONE: Tally = { done: 0, total: 0, live: 0 };

/** What an item holds, closed of all and claimed now, as the server counts it; nothing for a ticket. */
export function tally(b: Board, id: string): Tally {
  return b.nodes.get(id)?.progress ?? NONE;
}

export interface PlanRow {
  node: GraphNode;
  tally: Tally;
  /** The server holds the plan due for its audit. */
  due: boolean;
}

/** The open items of one kind with their progress, the due ones first, then the nearest done. */
export function plans(b: Board, kind: Kind): PlanRow[] {
  const rows: PlanRow[] = [];
  for (const node of b.nodes.values()) {
    if (node.kind !== kind || isClosed(node.word)) continue;
    rows.push({ node, tally: node.progress ?? NONE, due: node.due === true });
  }
  const share = (r: PlanRow) => (r.tally.total ? r.tally.done / r.tally.total : -1);
  return rows.sort((a, z) => Number(z.due) - Number(a.due) || share(z) - share(a) || byId(a.node.id, z.node.id));
}

/** Ids in key order, then by number: `T9` before `T10`. */
export function byId(a: string, z: string): number {
  const [, ka, na] = /^([A-Z]+)(\d+)$/.exec(a) ?? [, a, '0'];
  const [, kz, nz] = /^([A-Z]+)(\d+)$/.exec(z) ?? [, z, '0'];
  return (ka ?? '').localeCompare(kz ?? '') || Number(na) - Number(nz);
}

export interface Day {
  day: string;
  opened: number;
  closed: number;
  dropped: number;
}

/**
 * Opens and closes per local day, for the `days` days ending today. Days older than the oldest event read
 * are left out, so a page of events that does not reach back the whole window never shows a false zero.
 */
export function daily(events: EventRow[], days: number, now: number, complete: boolean): Day[] {
  const out: Day[] = [];
  for (let i = days - 1; i >= 0; i--) out.push({ day: day(now - i * 86400), opened: 0, closed: 0, dropped: 0 });
  const index = new Map(out.map((d, i) => [d.day, i]));
  let oldest = Infinity;
  for (const e of events) {
    const t = epoch(e.at);
    if (t === null) continue;
    oldest = Math.min(oldest, t);
    const i = index.get(day(t));
    if (i === undefined) continue;
    if (e.kind === 'opened') out[i].opened++;
    else if (e.kind === 'closed') out[i].closed++;
    else if (e.kind === 'dropped') out[i].dropped++;
  }
  if (complete || oldest === Infinity) return complete ? out : [];
  const first = day(oldest);
  return out.filter((d) => d.day > first);
}

export interface Move {
  at: string;
  verbs: [string, string[]][];
}

/** Events, newest first, gathered by the minute they fell in: each verb with the ids it moved. */
export function moves(events: EventRow[], idOf: (rid: number) => string | undefined): Move[] {
  const out: Move[] = [];
  let current: { minute: number; at: string; verbs: Map<string, string[]> } | null = null;
  for (const e of events) {
    const t = epoch(e.at);
    if (t === null || typeof e.rid !== 'number') continue;
    const id = idOf(e.rid);
    if (!id) continue;
    const minute = Math.floor(t / 60);
    if (!current || current.minute !== minute) {
      if (current) out.push({ at: current.at, verbs: [...current.verbs] });
      current = { minute, at: e.at, verbs: new Map() };
    }
    const ids = current.verbs.get(e.kind) ?? [];
    if (!ids.includes(id)) ids.push(id);
    current.verbs.set(e.kind, ids);
  }
  if (current) out.push({ at: current.at, verbs: [...current.verbs] });
  return out;
}

/** The event kinds that move an item, as the TUI reads them. */
export const MOVE_KINDS = [
  'opened', 'reopened', 'closed', 'dropped', 'claimed', 'released', 'claim_lost',
  'asked', 'replied', 'decided', 'waited', 'resumed',
];

/** The word a verb leads to, for its colour, as the TUI tints it. */
export function verbWord(kind: string): string | null {
  switch (kind) {
    case 'closed': return 'done';
    case 'released': case 'replied': case 'resumed': case 'reopened': case 'opened': return 'ready';
    case 'claimed': return 'building';
    case 'asked': return 'parked';
    case 'decided': return 'checking';
    case 'waited': return 'blocked';
    case 'dropped': case 'claim_lost': return 'dropped';
    default: return null;
  }
}
