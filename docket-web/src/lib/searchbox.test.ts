import { describe, expect, it } from 'vitest';
import { escapeTo, fieldText, searchOrigin } from './searchbox';

describe('fieldText', () => {
  it('keeps a trailing space when the address catches up with the trimmed text', () => {
    expect(fieldText('garden ', 'garden')).toBe('garden ');
  });

  it('keeps surrounding whitespace on a trimmed match', () => {
    expect(fieldText('  garden  ', 'garden')).toBe('  garden  ');
  });

  it('replaces the field when the address moves elsewhere', () => {
    expect(fieldText('garden', 'lantern')).toBe('lantern');
  });

  it('empties the field when the address is cleared', () => {
    expect(fieldText('garden ', '')).toBe('');
  });

  it('leaves an unchanged field alone', () => {
    expect(fieldText('garden', 'garden')).toBe('garden');
  });
});

describe('searchOrigin', () => {
  it('keeps the page a search was typed on', () => {
    expect(searchOrigin(null, 'overview', '/ui/p/plans')).toBe('/ui/p/plans');
  });

  it('holds the page it came from while the results take more typing', () => {
    expect(searchOrigin('/ui/p/plans', 'work', '/ui/p/work?q=lantern')).toBe('/ui/p/plans');
  });

  it('keeps none for a search begun on the results', () => {
    expect(searchOrigin(null, 'work', '/ui/p/work')).toBeNull();
  });
});

describe('escapeTo', () => {
  it('returns to the page the search left', () => {
    expect(escapeTo('/ui/p/plans', '/ui/p')).toBe('/ui/p/plans');
  });

  it('falls back to the overview', () => {
    expect(escapeTo(null, '/ui/p')).toBe('/ui/p');
  });
});
