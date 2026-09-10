import { describe, expect, it } from 'vitest';
import { formatOfficialCountdown, formatResetAt } from './reset';

describe('formatResetAt', () => {
  it('formats session resets with time only', () => {
    const label = formatResetAt('4070907000', 'session');

    expect(label).toMatch(/\b(AM|PM)\b/);
    expect(label).not.toMatch(/2099|Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec|,/);
  });
});

describe('formatOfficialCountdown', () => {
  it('formats days and hours correctly like official Antigravity', () => {
    const now = Date.now();
    // 6 days, 20 hours in future
    const sixDaysTwentyHours = new Date(
      now + (6 * 86400 + 20 * 3600 + 10 * 60) * 1000,
    ).toISOString();
    expect(formatOfficialCountdown(sixDaysTwentyHours, now)).toBe('6 days, 20 hours');

    // 1 day, 1 hour in future
    const oneDayOneHour = new Date(now + (1 * 86400 + 1 * 3600) * 1000).toISOString();
    expect(formatOfficialCountdown(oneDayOneHour, now)).toBe('1 day, 1 hour');

    // 2 days in future (0 hours)
    const twoDays = new Date(now + 2 * 86400 * 1000).toISOString();
    expect(formatOfficialCountdown(twoDays, now)).toBe('2 days');

    // 1 hour, 56 minutes in future
    const oneHourFiftySixMins = new Date(now + (1 * 3600 + 56 * 60) * 1000).toISOString();
    expect(formatOfficialCountdown(oneHourFiftySixMins, now)).toBe('1 hour, 56 minutes');

    // 45 minutes in future
    const fortyFiveMins = new Date(now + 45 * 60 * 1000).toISOString();
    expect(formatOfficialCountdown(fortyFiveMins, now)).toBe('45 minutes');

    // Past time
    const past = new Date(now - 1000).toISOString();
    expect(formatOfficialCountdown(past, now)).toBe('now');

    // Null/undefined
    expect(formatOfficialCountdown(null)).toBe('unknown');
    expect(formatOfficialCountdown(undefined)).toBe('unknown');
  });
});
