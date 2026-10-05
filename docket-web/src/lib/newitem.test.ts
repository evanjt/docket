import { describe, expect, it } from 'vitest';
import { ITEM_KEYS, kindOfType, newItemDefaults, newItemRequests } from './newitem';

const releases = ['7.0', '6.2'];
const form = { key: 'T', title: ' Add it ', body: '', priority: 'normal', complexity: '', release: '', plan: '', origin: '', group: '', area: '' };

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

const areas = { A1: 'kites' };
const areaOf = (id: string) => areas[id as keyof typeof areas] ?? null;

describe('newItemRequests', () => {
  it('files under a plan in 6.2 with that release, then sets the parent and the origin', () => {
    const r = newItemRequests('x/y', { ...form, release: '6.2', plan: 'a1', origin: 'q2', group: ' ui ' }, areaOf);
    expect(r.request).toMatchObject({ project: 'x/y', key: 'T', title: 'Add it', release: '6.2', group: 'ui', priority: null });
    expect(r.after('T9')).toEqual([
      ['parent', { project: 'x/y', a: ['T9'], plan: 'A1' }, 'Put under the plan'],
      ['link', { project: 'x/y', a: ['T9'], kind: 'origin', b: 'Q2' }, 'Linked'],
    ]);
  });

  it('sends no release, group, parent or link when they are blank', () => {
    const r = newItemRequests('x/y', { ...form, area: 'lanterns' }, areaOf);
    expect(r.request).toMatchObject({ release: null, group: null, area: 'lanterns' });
    expect(r.after('T9')).toEqual([]);
  });

  it('sends the plan\'s area when the item goes under a plan', () => {
    const r = newItemRequests('x/y', { ...form, plan: 'A1' }, areaOf);
    expect(r.refusal).toBeNull();
    expect(r.request).toMatchObject({ area: 'kites' });
  });

  it('refuses an item with no plan and no area, naming the area', () => {
    expect(newItemRequests('x/y', form, areaOf).refusal).toMatch(/area/);
    expect(newItemRequests('x/y', { ...form, plan: 'A9' }, areaOf).refusal).toMatch(/area/);
  });
});

describe('ITEM_KEYS', () => {
  it('lists the five keys docket files under', () => {
    expect(ITEM_KEYS.map((k) => k.key)).toEqual(['T', 'B', 'Q', 'I', 'A']);
  });
});

describe('kindOfType', () => {
  it('reads the kind from the item type, work for none', () => {
    expect(kindOfType('question')).toBe('decision');
    expect(kindOfType('plan')).toBe('audit');
    expect(kindOfType('investigation')).toBe('research');
    expect(kindOfType('bug')).toBe('work');
    expect(kindOfType(undefined)).toBe('work');
  });
});
