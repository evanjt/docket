import { describe, expect, it } from 'vitest';
import { newItemDefaults, newItemRequests } from './newitem';

const releases = ['7.0', '6.2'];
const form = { key: 'T', title: ' Add it ', body: '', priority: 'normal', complexity: '', release: '', plan: '', origin: '', group: '' };

describe('newItemDefaults', () => {
  it('puts a new item under the open plan, in its release', () => {
    expect(newItemDefaults({ id: 'P3', release: '6.2', kind: 'audit' }, releases)).toEqual({ release: '6.2', plan: 'P3', origin: '' });
  });

  it('takes an open item that is no plan as the origin', () => {
    expect(newItemDefaults({ id: 'Q2', release: null, kind: 'decision' }, releases)).toMatchObject({ plan: '', origin: 'Q2' });
  });

  it('leaves them empty with no item open', () => {
    expect(newItemDefaults(undefined, releases)).toEqual({ release: '', plan: '', origin: '' });
  });
});

describe('newItemRequests', () => {
  it('files under a plan in 6.2 with that release, then sets the parent and the origin', () => {
    const r = newItemRequests('x/y', { ...form, release: '6.2', plan: 'p3', origin: 'q2', group: ' ui ' });
    expect(r.request).toMatchObject({ project: 'x/y', key: 'T', title: 'Add it', release: '6.2', group: 'ui', priority: null });
    expect(r.after('T9')).toEqual([
      ['parent', { project: 'x/y', a: ['T9'], plan: 'P3' }, 'Put under the plan'],
      ['link', { project: 'x/y', a: ['T9'], kind: 'origin', b: 'Q2' }, 'Linked'],
    ]);
  });

  it('sends no release, group, parent or link when they are blank', () => {
    const r = newItemRequests('x/y', form);
    expect(r.request).toMatchObject({ release: null, group: null });
    expect(r.after('T9')).toEqual([]);
  });
});
