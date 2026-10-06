import type { EventRow, Graph, GraphNode, Kind, Progress } from './types';
import { day, epoch } from './time';

/** The flow read left to right, then the words that sit beside it. */
export const FLOW = ['ready', 'in progress'] as const;
export const ASIDE = ['blocked', 'waiting on owner', 'parked'] as const;
export const CLOSED = ['done', 'dropped'];

/** What each word means, as the TUI's help words it. */
export const MEANING: Record<string, string> = {
  ready: 'waiting for an agent to take it',
  'in progress': 'a job or a person is working on it now',
  'under way': 'a plan whose tickets are open: not running work itself',
  'audit due': 'a plan whose tickets are all closed, waiting for its audit',
  blocked: 'held by a dependency that is not yet satisfied',
  'waiting on owner': 'waiting on you: something only you can do',
  parked: 'set aside, in no queue',
  done: 'closed',
  dropped: 'dropped',
};

/** The groupings a plans page shows, plans first: the simple model keeps only plans in the queue. */
export const GROUPINGS: { kind: Kind; title: string }[] = [
  { kind: 'audit', title: 'Plans' },
  { kind: 'story', title: 'Stories' },
  { kind: 'package', title: 'Packages' },
];

/** The custom property that colours a word; a word of several is joined with dashes. */
export function wordVar(word: string): string {
  return `var(--w-${word.replace(/ /g, '-')})`;
}

export function isClosed(word: string): boolean {
  return CLOSED.includes(word);
}

export interface Board {
  nodes: Map<string, GraphNode>;
  byRid: Map<number, GraphNode>;
  /** The children of each plan: the edges `child parent plan`, read from the plan. */
  children: Map<string, string[]>;
  /** The plan each item is under, as a list of one. */
  parents: Map<string, string[]>;
  /** What spawned each item. */
  origins: Map<string, string[]>;
  /** What each item spawned. */
  spawned: Map<string, string[]>;
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
  const b: Board = {
    nodes, byRid, children: new Map(), parents: new Map(), origins: new Map(), spawned: new Map(), related: new Map(), cites: new Map(),
  };
  for (const e of g.edges) {
    if (e.kind === 'parent') {
      push(b.children, e.to, e.from);
      push(b.parents, e.from, e.to);
    } else if (e.kind === 'origin') {
      push(b.origins, e.from, e.to);
      push(b.spawned, e.to, e.from);
    } else if (e.kind === 'related') {
      push(b.related, e.from, e.to);
      push(b.related, e.to, e.from);
    } else if (e.kind === 'cites') {
      push(b.cites, e.from, e.to);
    }
  }
  return b;
}

/** What a bar draws: done of the items not dropped, and those claimed now. */
export interface Tally {
  done: number;
  total: number;
  live: number;
}

const NONE: Tally = { done: 0, total: 0, live: 0 };

function bar(p: Progress | null | undefined): Tally {
  return p ? { done: p.done, total: p.counted, live: p.live } : NONE;
}

/** What an item holds, closed of all and claimed now, as the server counts it; nothing for a ticket. */
export function tally(b: Board, id: string): Tally {
  return bar(b.nodes.get(id)?.progress);
}

export interface PlanRow {
  node: GraphNode;
  tally: Tally;
  /** The server holds the plan due for its audit. */
  due: boolean;
}

/** The word drawn beside a plan: none while it is under way, where its tally is the whole state. */
export function planWord(row: PlanRow): string | null {
  return row.node.word === 'under way' ? null : row.node.word;
}

/** The plans whose word is under way and those whose word is audit due, as the server words them. */
export function planCount(b: Board): { open: number; auditDue: number } {
  let open = 0;
  let auditDue = 0;
  for (const n of b.nodes.values()) {
    if (n.kind !== 'audit') continue;
    if (n.word === 'under way') open++;
    else if (n.word === 'audit due') auditDue++;
  }
  return { open, auditDue };
}

/** The open items of one kind with their progress, the due ones first, then the nearest done. */
export function plans(b: Board, kind: Kind): PlanRow[] {
  const rows: PlanRow[] = [];
  for (const node of b.nodes.values()) {
    if (node.kind !== kind || isClosed(node.word)) continue;
    rows.push({ node, tally: bar(node.progress), due: node.due === true });
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
