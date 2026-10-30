import type { LeadState, Machine, Row } from './types';
import { ago, duration, epoch } from './time';
import { landing } from './note';

export interface MachineUse {
  name: string;
  slots: number;
  /** Claims whose job has not reported its end: the slots in use. */
  used: number;
  /** Claims whose job reported its end and that no lead has landed yet. */
  waiting: number;
  runners: string[];
  /** The runners under a usage limit still in force, each with its reset. */
  limited: { runner: string; until: string }[];
  here: boolean;
}

/** Whether the job behind a claim ended, which is when its slot is free again: the row reads to land or job failed. */
export function jobEnded(claim: Row): boolean {
  return claim.word === 'to land' || claim.word === 'job failed';
}

/**
 * Each machine with the jobs running on it: a claim counts against the machine its job runs on
 * until its row reads to land or job failed, after which it waits to land or to be given back. A usage limit is in force until
 * its reset, `now` in epoch seconds.
 */
export function machineUse(machines: Machine[], claims: Row[], here: string, now: number): MachineUse[] {
  return machines.map((m) => {
    const on = claims.filter((c) => (c.claim_on ?? '') === m.name);
    const waiting = on.filter(jobEnded).length;
    return {
      name: m.name,
      slots: m.slots,
      used: on.length - waiting,
      waiting,
      runners: m.runners,
      limited: Object.entries(m.limits ?? {})
        .filter(([, until]) => Date.parse(until) / 1000 > now)
        .map(([runner, until]) => ({ runner, until })),
      here: m.name === here,
    };
  });
}

/** The lead claim in one line, as of `now` in epoch seconds. */
export function leadLine(state: LeadState | null | undefined, now: number): string {
  const lead = state?.lead;
  if (!lead) return 'No lead. A /lead session takes the lead and dispatches the queue.';
  const when = ago(lead.renewed_at, now) || lead.renewed_at;
  if (state.lapsed) return `The lead lapsed: ${lead.session} on ${lead.host} last renewed ${when}.`;
  return `Led by ${lead.session} on ${lead.host}, renewed ${when}.`;
}

/** The job behind a claim, as `runner job`, or nothing for a claim made by hand. */
export function jobLine(row: Row): string {
  return [row.claim_runner, row.claim_job].filter(Boolean).join(' ');
}

/** The words a claim shows under, in the order the Running now list groups them, as `docket wip` does. */
export const CLAIM_WORDS = ['in progress', 'to land', 'job failed'] as const;

/** Claims under their word, in the order of `CLAIM_WORDS` and any other word after; a word with no claim is left out. */
export function claimGroups(claims: Row[]): { word: string; rows: Row[] }[] {
  const words: string[] = [...CLAIM_WORDS];
  for (const c of claims) if (!words.includes(c.word)) words.push(c.word);
  return words.map((word) => ({ word, rows: claims.filter((c) => c.word === word) })).filter((g) => g.rows.length);
}

/**
 * The short line beside a claim: its machine, then for a running job how long it has run, for an ended one where its
 * build stands on its way to landing or how long since it ended. `now` is in epoch seconds.
 */
export function claimMeta(claim: Row, now: number): string {
  const host = (claim.claim_on ?? claim.claim_host ?? '').split('.')[0];
  const from = epoch(claim.word === 'in progress' ? claim.claim_since : (claim.claim_ended_at ?? claim.claim_since));
  const when = landing(claim, now) || (from ? duration(now - from) : '');
  return [host, when].filter(Boolean).join(' · ');
}

/** The whole claim for a tooltip: its branch, the machine and the job behind it. */
export function claimDetail(claim: Row): string {
  const host = (claim.claim_on ?? claim.claim_host ?? '').split('.')[0];
  const where = [claim.claim_branch, host && `on ${host}`].filter(Boolean).join(' ');
  return [where, jobLine(claim)].filter(Boolean).join(', ');
}
