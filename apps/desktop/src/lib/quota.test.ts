import { describe, expect, it } from 'vitest';
import { planTypeLabel, quotaDisplayWindows } from './quota';
import type { UsageQuotaSnapshot } from './types';

const baseQuota: UsageQuotaSnapshot = {
  profileId: 'main',
  source: 'app_server_rate_limits',
  fetchedAt: 1,
  staleness: 'fresh',
  planType: 'team',
  activityTokens: null,
  primaryUsedPercent: 40,
  primaryWindowDurationMins: 300,
  secondaryUsedPercent: 57,
  secondaryWindowDurationMins: 10080,
  remainingPercent: 60,
  resetAt: '1782109286',
  secondaryResetAt: '1782352982',
  alerts: [],
  suggestedActions: [],
};

describe('quotaDisplayWindows', () => {
  it('returns 5h and weekly windows for standard quota snapshots', () => {
    expect(quotaDisplayWindows(baseQuota)).toEqual([
      {
        key: 'primary',
        label: 'Session (5h)',
        shortLabel: '5h',
        usedPercent: 40,
        resetAt: '1782109286',
        variant: 'session',
      },
      {
        key: 'secondary',
        label: 'Weekly (7d)',
        shortLabel: 'weekly',
        usedPercent: 57,
        resetAt: '1782352982',
        variant: 'weekly',
      },
    ]);
  });

  it('returns only a monthly window for monthly-only quota snapshots', () => {
    expect(
      quotaDisplayWindows({
        ...baseQuota,
        primaryUsedPercent: 11,
        primaryWindowDurationMins: 43800,
        secondaryUsedPercent: null,
        secondaryWindowDurationMins: null,
        remainingPercent: 89,
        resetAt: '1784724636',
        secondaryResetAt: null,
      }),
    ).toEqual([
      {
        key: 'primary',
        label: 'Monthly',
        shortLabel: 'monthly',
        usedPercent: 11,
        resetAt: '1784724636',
        variant: 'monthly',
      },
    ]);
  });
});

describe('planTypeLabel', () => {
  it('normalizes non-empty plan types for compact badges', () => {
    expect(planTypeLabel(' team ')).toBe('TEAM');
    expect(planTypeLabel('pro')).toBe('PRO');
  });

  it('hides missing or blank plan types', () => {
    expect(planTypeLabel(null)).toBeNull();
    expect(planTypeLabel('   ')).toBeNull();
  });
});
