import { describe, expect, it } from 'vitest';
import { newItemDefaults, newItemRequests } from './newitem';

const releases = ['1.0.0', '0.4.1'];
const form = { key: 'T', title: ' Add it ', body: '', priority: 'normal', complexity: '', release: '', openedBy: '', group: '' };

describe('newItemDefaults', () => {
  it('takes the release and the opener from the open item', () => {
    expect(newItemDefaults({ id: 'P3', theme: '0.4.1' }, releases)).toMatchObject({ release: '0.4.1', openedBy: 'P3' });
  });

  it('leaves both empty with no item open', () => {
    expect(newItemDefaults(undefined, releases)).toMatchObject({ release: '', openedBy: '' });
  });
});

describe('newItemRequests', () => {
  it('files from an open plan in 0.4.1 with that release and an opened link', () => {
    const r = newItemRequests('x/y', { ...form, release: '0.4.1', openedBy: 'p3', group: ' ui ' });
    expect(r.request).toMatchObject({ project: 'x/y', key: 'T', title: 'Add it', release: '0.4.1', group: 'ui', priority: null });
    expect(r.link('T9')).toEqual({ project: 'x/y', a: ['T9'], kind: 'opened', b: 'P3' });
  });

  it('sends no release, group or link when they are blank', () => {
    const r = newItemRequests('x/y', form);
    expect(r.request).toMatchObject({ release: null, group: null });
    expect(r.link('T9')).toBeNull();
  });
});
