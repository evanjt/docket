import { releaseOf } from './releases';
import type { Kind } from './types';

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
  /** The area's name; an item under a plan takes the plan's. */
  area: string;
}

/** The item types, each with the key docket files it under, the kind it is read as and whose turn a
 * new one starts on. */
export const ITEM_KEYS: { type: string; key: string; kind: Kind; turn: 'agent' | 'user'; meaning: string }[] = [
  { type: 'task', key: 'T', kind: 'work', turn: 'agent', meaning: 'tasks' },
  { type: 'bug', key: 'B', kind: 'work', turn: 'agent', meaning: 'bugs' },
  { type: 'question', key: 'Q', kind: 'decision', turn: 'user', meaning: 'questions, a decision not a commit' },
  { type: 'investigation', key: 'I', kind: 'research', turn: 'agent', meaning: 'investigations, measured before decided' },
  { type: 'plan', key: 'A', kind: 'audit', turn: 'agent', meaning: 'plans, checked against the tree once all they opened is closed' },
];

/** The kind an item of the stored type is read as; work for a type that is none. */
export function kindOfType(type: string | undefined | null): Kind {
  return ITEM_KEYS.find((t) => t.type === type)?.kind ?? 'work';
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

/** The `new` request, and the `parent` and `link origin` requests to send once the new item has an id.
 * Every item is in an area: a plan's item takes the plan's, any other names one, and with neither the
 * request is refused. */
export function newItemRequests(slug: string, f: NewItemForm, areaOf: (plan: string) => string | null) {
  const plan = f.plan.trim().toUpperCase();
  const origin = f.origin.trim().toUpperCase();
  const area = (plan ? areaOf(plan) : null) ?? f.area.trim();
  return {
    refusal: area ? null : plan ? `${plan} is in no area: name the area the item is in` : 'name the area the item is in, or put it under a plan',
    request: {
      project: slug,
      key: f.key,
      title: f.title.trim(),
      body: f.body.trim() || null,
      priority: f.priority === 'normal' ? null : f.priority,
      complexity: f.complexity || null,
      release: f.release || null,
      group: f.group.trim() || null,
      area: area || null,
    },
    after: (id: string): [string, Record<string, unknown>, string][] => [
      ...(plan ? [['parent', { project: slug, a: [id], plan }, 'Put under the plan'] as [string, Record<string, unknown>, string]] : []),
      ...(origin ? [['link', { project: slug, a: [id], kind: 'origin', b: origin }, 'Linked'] as [string, Record<string, unknown>, string]] : []),
    ],
  };
}
