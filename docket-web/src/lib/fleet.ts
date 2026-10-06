import type { EventRow, LeadState, Machine, Row } from './types';
import { ago } from './time';

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

/** Whether the job behind a claim reported its end, which is when its slot is free again. */
export function jobEnded(claim: Row, reports: EventRow[]): boolean {
  return reports.some(
    (e) => e.kind === 'job_reported' && e.branch === claim.claim_branch && typeof e.data?.end === 'string',
  );
}

/**
 * Each machine with the jobs running on it: a claim counts against the machine its job runs on
 * until the job reports its end, after which it is waiting to land. `reports` are the project's
 * `job_reported` events. A usage limit is in force until its reset, `now` in epoch seconds.
 */
export function machineUse(machines: Machine[], claims: Row[], reports: EventRow[], here: string, now: number): MachineUse[] {
  return machines.map((m) => {
    const on = claims.filter((c) => (c.claim_on ?? '') === m.name);
    const waiting = on.filter((c) => jobEnded(c, reports)).length;
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
