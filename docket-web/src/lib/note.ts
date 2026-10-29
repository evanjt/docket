import type { Row } from './types';
import { ago } from './time';

/** Why a row stands where it does, in the TUI's words: the matched words, the claim, the wait or the turn note. */
export function note(r: Row): string {
  if (r.snip) return r.snip;
  if (r.claim_branch) {
    const host = (r.claim_on ?? r.claim_host ?? '').split('.')[0];
    return host ? `${r.claim_branch} on ${host}` : r.claim_branch;
  }
  if (r.wait_on) return r.wait_on === 'condition' ? `waits on ${r.wait_ref ?? ''}, for the owner` : `waits on ${r.wait_ref ?? ''}`;
  return r.turn_note ?? '';
}

/** A snippet cut at the server's `[` and `]` markers into plain and matched parts. */
export function highlight(snip: string): { text: string; hit: boolean }[] {
  return snip
    .split(/\[([^\]]*)\]/)
    .map((text, i) => ({ text, hit: i % 2 === 1 }))
    .filter((p) => p.text);
}

/** How long ago the claim's job ended, as "ended 5m ago"; empty while it runs or when the end has no time. */
export function ended(r: Pick<Row, 'claim_ended_at'>, now = Date.now() / 1000): string {
  const when = ago(r.claim_ended_at, now);
  return when ? `ended ${when}` : '';
}
