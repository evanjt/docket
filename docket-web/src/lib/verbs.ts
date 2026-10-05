import type { Offer, Offers } from './types';

/** The verbs an item can take, as the server names them. */
export type Verb =
  | 'answer' | 'reply' | 'retry' | 'start' | 'close' | 'release' | 'resume' | 'ask' | 'wait' | 'reopen' | 'drop';

/** The verbs the server says an item takes now, the ones this page has a form for, in the server's order. */
export function verbs(offers: Offers | undefined): Offer[] {
  return (offers?.verbs ?? []).filter((o) => o.verb in WORDING);
}

/** Whether the text holds what the offer is refused without. */
export function filled(offer: Offer | undefined, text: string): boolean {
  return !offer || offer.needs.length === 0 || text.trim().length > 0;
}

/** What each verb's button says, and what its toast says once it lands. */
export const WORDING: Record<Verb, { label: string; done: string }> = {
  answer: { label: 'Answer', done: 'Answered' },
  reply: { label: 'Reply', done: 'Replied' },
  start: { label: 'Start', done: 'Started' },
  close: { label: 'Close', done: 'Closed' },
  release: { label: 'Unclaim', done: 'Unclaimed' },
  retry: { label: 'Retry', done: 'Retried' },
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
