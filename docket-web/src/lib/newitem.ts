import { releaseOf } from './releases';

export interface NewItemForm {
  key: string;
  title: string;
  body: string;
  priority: string;
  complexity: string;
  release: string;
  openedBy: string;
  group: string;
}

/** The release and opener a new item starts with: those of the item open in the panel. */
export function newItemDefaults(
  from: { id: string; theme?: string | null } | undefined,
  releases: string[],
): Pick<NewItemForm, 'release' | 'openedBy'> {
  if (!from) return { release: '', openedBy: '' };
  return { release: releaseOf(from.theme, releases) ?? '', openedBy: from.id };
}

/** The `new` request, and the `link opened` request to send once the new item has an id. */
export function newItemRequests(slug: string, f: NewItemForm) {
  const opener = f.openedBy.trim().toUpperCase();
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
    link: (id: string) => (opener ? { project: slug, a: [id], kind: 'opened', b: opener } : null),
  };
}
