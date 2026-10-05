import { releaseOf } from './releases';

export interface NewItemForm {
  key: string;
  title: string;
  body: string;
  priority: string;
  complexity: string;
  release: string;
  plan: string;
  origin: string;
  group: string;
}

/** The kinds that hold children. */
const PLANS = ['audit', 'story', 'package'];

/** The release, plan and origin a new item starts with, from the item open in the panel: a plan holds
 * the new item, and anything else is what spawned it. */
export function newItemDefaults(
  from: { id: string; release?: string | null; kind?: string } | undefined,
  releases: string[],
): Pick<NewItemForm, 'release' | 'plan' | 'origin'> {
  if (!from) return { release: '', plan: '', origin: '' };
  const release = releaseOf(from.release, releases) ?? '';
  return PLANS.includes(from.kind ?? '') ? { release, plan: from.id, origin: '' } : { release, plan: '', origin: from.id };
}

/** The `new` request, and the `parent` and `link origin` requests to send once the new item has an id. */
export function newItemRequests(slug: string, f: NewItemForm) {
  const plan = f.plan.trim().toUpperCase();
  const origin = f.origin.trim().toUpperCase();
  return {
    request: {
      project: slug,
      key: f.key,
      title: f.title.trim(),
      body: f.body.trim() || null,
      priority: f.priority === 'normal' ? null : f.priority,
      complexity: f.complexity || null,
      release: f.release || null,
      group: f.group.trim() || null,
    },
    after: (id: string): [string, Record<string, unknown>, string][] => [
      ...(plan ? [['parent', { project: slug, a: [id], plan }, 'Put under the plan'] as [string, Record<string, unknown>, string]] : []),
      ...(origin ? [['link', { project: slug, a: [id], kind: 'origin', b: origin }, 'Linked'] as [string, Record<string, unknown>, string]] : []),
    ],
  };
}
