import { describe, expect, it } from 'vitest';
import { fieldText } from './searchbox';

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
