import { describe, expect, it } from 'vitest';
import { editRequest, verbs } from './verbs';
import type { Shown } from './types';

const item = (over: Partial<Shown>): Shown => ({
  id: 'T1', key: 'T', num: 1, project: 'o/p', title: 't', state: 'open', word: 'ready', priority: 'normal',
  tags: [], body: '', opened_at: '', updated_at: '', related: [], opened: [], cites: [], ...over,
});

describe('verbs', () => {
  it('offers start on a ready ticket', () => {
    expect(verbs(item({}), 'work', true)).toEqual(['start', 'ask', 'wait', 'drop']);
  });

  it('offers close and release on a claimed one', () => {
    expect(verbs(item({ word: 'building', claim_branch: 'b' }), 'work', true)).toEqual([
      'close', 'release', 'ask', 'wait', 'drop',
    ]);
  });

  it('offers reply on a parked ticket and answer on an open question', () => {
    expect(verbs(item({ word: 'parked' }), 'work', true)[0]).toBe('reply');
    expect(verbs(item({ word: 'parked' }), 'decision', true)[0]).toBe('answer');
  });

  it('keeps answer from an agent key', () => {
    expect(verbs(item({ word: 'parked' }), 'decision', false)).not.toContain('answer');
  });

  it('offers resume on a blocked one and reopen alone on a closed one', () => {
    expect(verbs(item({ word: 'blocked' }), 'work', true)).toEqual(['resume', 'drop']);
    expect(verbs(item({ word: 'done', state: 'done' }), 'work', true)).toEqual(['reopen']);
  });

  it('offers nothing on a standing item', () => {
    expect(verbs(item({ word: 'standing' }), 'concept', true)).toEqual([]);
  });
});

describe('editRequest', () => {
  const opened = { title: 'Old', body: 'first', updated_at: 'u1' };
  const common = { project: 'o/p', branch: null };

  it('sends no body on a title-only edit after the item changed under the form', () => {
    expect(editRequest(opened, 'New', 'first', 'T1', common)).toEqual({
      ...common, id: 'T1', set: [{ field: 'title', value: 'New' }], body: null, expect_updated_at: null,
    });
  });

  it('sends the edited body with the stamp the form opened on', () => {
    expect(editRequest(opened, 'Old', 'second', 'T1', common)).toEqual({
      ...common, id: 'T1', set: [], body: 'second', expect_updated_at: 'u1',
    });
  });

  it('sends nothing when nothing changed', () => {
    expect(editRequest(opened, 'Old', 'first', 'T1', common)).toBeNull();
  });
});
