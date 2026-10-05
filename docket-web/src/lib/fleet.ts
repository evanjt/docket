import type { LeadState, Machine, Row } from './types';
import { ago } from './time';

export interface MachineUse {
  name: string;
  slots: number;
  used: number;
  runners: string[];
  /** The runners under a usage limit still in force, each with its reset. */
  limited: { runner: string; until: string }[];
  here: boolean;
}

/**
 * Each machine with the claims running on it: a claim counts against the machine its job runs on.
 * A usage limit is in force until its reset, `now` in epoch seconds.
 */
export function machineUse(machines: Machine[], claims: Row[], here: string, now: number): MachineUse[] {
  return machines.map((m) => ({
    name: m.name,
    slots: m.slots,
    used: claims.filter((c) => (c.claim_on ?? '') === m.name).length,
    runners: m.runners,
    limited: Object.entries(m.limits ?? {})
      .filter(([, until]) => Date.parse(until) / 1000 > now)
      .map(([runner, until]) => ({ runner, until })),
    here: m.name === here,
  }));
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
