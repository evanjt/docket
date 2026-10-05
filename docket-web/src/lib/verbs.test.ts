import { describe, expect, it } from 'vitest';
import { editRequest, filled, moveRequest, verbs } from './verbs';
import type { Offers } from './types';

const offers = (over: Partial<Offers>): Offers => ({
  id: 'T1', verbs: [], priorities: ['critical', 'high', 'normal', 'low'], levels: ['high', 'medium', 'low'], ...over,
});

describe('verbs', () => {
  it('offers what the server names, in its order, and nothing before it answers', () => {
    const o = offers({
      verbs: [
        { verb: 'start', needs: [] },
        { verb: 'ask', needs: ['note'], branch: 'audit/t1-1' },
        { verb: 'drop', needs: ['why'], branch: 'audit/t1-1' },
      ],
    });
    expect(verbs(o).map((v) => v.verb)).toEqual(['start', 'ask', 'drop']);
    expect(verbs(o)[1].branch).toBe('audit/t1-1');
    expect(verbs(undefined)).toEqual([]);
  });

  it('leaves out a verb this page has no form for', () => {
    expect(verbs(offers({ verbs: [{ verb: 'kill', needs: [] }, { verb: 'reply', needs: ['note'] }] })).map((v) => v.verb)).toEqual(['reply']);
  });
});

describe('filled', () => {
  it('holds a verb back until its needs are written', () => {
    const drop = { verb: 'drop', needs: ['why'] };
    expect(filled(drop, '')).toBe(false);
    expect(filled(drop, '  ')).toBe(false);
    expect(filled(drop, 'replaced')).toBe(true);
  });

  it('lets a verb with no needs go with an empty form', () => {
    expect(filled({ verb: 'release', needs: [] }, '')).toBe(true);
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

describe('moveRequest', () => {
  const common = { project: 'o/p', branch: null };
  it('is the edit verb naming the release, with no field set and nothing carried', () => {
    expect(moveRequest('T1', '1.1', common)).toEqual({ ...common, id: 'T1', release: '1.1', set: [], carry: false });
  });
});
