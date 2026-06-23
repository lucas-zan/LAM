import { describe, expect, it } from 'vitest';
import { formatResetAt } from './reset';

describe('formatResetAt', () => {
  it('formats session resets with time only', () => {
    const label = formatResetAt('4070907000', 'session');

    expect(label).toMatch(/\b(AM|PM)\b/);
    expect(label).not.toMatch(/2099|Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec|,/);
  });
});
