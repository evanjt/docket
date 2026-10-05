import type { Status } from './types';

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
