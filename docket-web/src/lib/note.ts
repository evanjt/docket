import type { Row } from './types';

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
