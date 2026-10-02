import { describe, expect, it } from 'vitest';
import { href, resolve } from './route';

const SLUGS = ['acme/widgets', 'acme', 'gizmo'];

describe('resolve', () => {
  it('reads the root as home', () => {
    expect(resolve('/ui/', SLUGS)).toEqual({ page: 'home' });
    expect(resolve('/ui', SLUGS)).toEqual({ page: 'home' });
  });

  it('takes the longest known slug', () => {
    expect(resolve('/ui/acme/widgets', SLUGS)).toEqual({ page: 'project', slug: 'acme/widgets', tab: 'overview' });
    expect(resolve('/ui/acme/widgets/work', SLUGS)).toEqual({ page: 'project', slug: 'acme/widgets', tab: 'work' });
  });

  it('reads a one-segment slug followed by a tab', () => {
    expect(resolve('/ui/gizmo/plans', SLUGS)).toEqual({ page: 'project', slug: 'gizmo', tab: 'plans' });
  });

  it('falls back to a shorter slug when the rest is a tab', () => {
    expect(resolve('/ui/acme/yours', SLUGS)).toEqual({ page: 'project', slug: 'acme', tab: 'yours' });
  });

  it('names a path it cannot place', () => {
    expect(resolve('/ui/nobody/here', SLUGS)).toEqual({ page: 'unknown', path: 'nobody/here' });
    expect(resolve('/ui/acme/widgets/nowhere', SLUGS).page).toBe('unknown');
  });
});

describe('href', () => {
  it('keeps the slash of a slug and drops the overview tab', () => {
    expect(href('acme/widgets')).toBe('/ui/acme/widgets');
    expect(href('acme/widgets', 'work', { list: 'next', i: 'T5', q: undefined })).toBe('/ui/acme/widgets/work?list=next&i=T5');
  });

  it('round-trips through resolve', () => {
    expect(resolve(href('gizmo', 'activity'), SLUGS)).toEqual({ page: 'project', slug: 'gizmo', tab: 'activity' });
  });
});
