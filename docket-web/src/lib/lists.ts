import type { Board } from './flow';
import type { GraphNode, Kind, Status } from './types';

const STEP = 100;

/** Rows asked of a list route after `more` Show more clicks; a chosen release reads the whole list. */
export function pageSize(more: number, release: boolean): number {
  return release ? 5000 : STEP * (more + 1);
}

/** The total a list holds, where the status counts it; null where they do not. */
export function countOf(list: string, status?: Pick<Status, 'by_word'>): number | null {
  if (list !== 'done' && list !== 'dropped') return null;
  return status?.by_word[list] ?? null;
}

/** "N of M" when the rows shown are a cut of a known total, else the bare count. */
export function caption(shown: number, total: number | null): string {
  return total !== null && total > shown ? `${shown} of ${total}` : `${shown}`;
}

const WORD_LISTS: Record<string, string> = {
  building: 'wip',
  ready: 'next',
  blocked: 'waiting',
  parked: 'todo',
  done: 'done',
  dropped: 'dropped',
};

/** The list route whose rows carry the reasons and priorities of one word; null where no list covers it. */
export function wordList(word: string): string | null {
  return WORD_LISTS[word] ?? null;
}

const GROUPING_KINDS: Kind[] = ['audit', 'story', 'package', 'concept', 'idea'];

/** The graph nodes of one word that are queue items: plans, packages and standing kinds are shown on the Plans page. */
export function queueNodes(b: Board, word: string): GraphNode[] {
  return [...b.nodes.values()].filter((n) => n.word === word && !GROUPING_KINDS.includes(n.kind));
}
