import type { Kind, Shown } from './types';

/** The verbs that fit an item as it stands, in the order its panel offers them. */
export type Verb =
  | 'answer' | 'reply' | 'start' | 'close' | 'release' | 'resume' | 'ask' | 'wait' | 'reopen' | 'drop';

export function verbs(item: Shown, kind: Kind, owner: boolean): Verb[] {
  const word = item.word;
  if (word === 'done' || word === 'dropped') return ['reopen'];
  if (word === 'standing') return [];
  const out: Verb[] = [];
  const question = kind === 'decision';
  if (question && owner && !item.decision) out.push('answer');
  if (word === 'parked' && !question) out.push('reply');
  if (item.claim_branch) out.push('close', 'release');
  else if (word === 'ready') out.push('start');
  if (word === 'blocked') out.push('resume');
  if (word !== 'parked' && word !== 'blocked') out.push('ask', 'wait');
  out.push('drop');
  return out;
}

/** What each verb's button says, and what its toast says once it lands. */
export const WORDING: Record<Verb, { label: string; done: string }> = {
  answer: { label: 'Answer', done: 'Answered' },
  reply: { label: 'Reply', done: 'Replied' },
  start: { label: 'Start', done: 'Started' },
  close: { label: 'Close', done: 'Closed' },
  release: { label: 'Release', done: 'Released' },
  resume: { label: 'Resume', done: 'Resumed' },
  ask: { label: 'Park for me', done: 'Parked' },
  wait: { label: 'Wait', done: 'Waiting' },
  reopen: { label: 'Reopen', done: 'Reopened' },
  drop: { label: 'Drop', done: 'Dropped' },
};

/** What the edit form held when it opened. */
export interface Opened {
  title: string;
  body: string;
  updated_at: string;
}

/**
 * The edit verb's request, or null when nothing changed. The title and body are compared with the
 * copy taken when the form opened, not the live item, and a body carries that copy's `updated_at`
 * so the server refuses it against a newer row.
 */
export function editRequest(
  opened: Opened,
  title: string,
  body: string,
  id: string,
  common: { project: string; branch: string | null },
): object | null {
  const set = title.trim() && title.trim() !== opened.title ? [{ field: 'title', value: title.trim() }] : [];
  const changed = body !== opened.body;
  if (!set.length && !changed) return null;
  return { ...common, id, set, body: changed ? body : null, expect_updated_at: changed ? opened.updated_at : null };
}
