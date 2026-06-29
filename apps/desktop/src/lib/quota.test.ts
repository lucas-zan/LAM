import { describe, expect, it } from 'vitest';
import { planTypeLabel, quotaDisplayWindows, accountHasAvailableQuota, resetCreditDisplay } from './quota';
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

describe('accountHasAvailableQuota', () => {
  it('returns true if all present quotas have remaining volume', () => {
    expect(accountHasAvailableQuota(baseQuota)).toBe(true);

    expect(
      accountHasAvailableQuota({
        ...baseQuota,
        primaryUsedPercent: 20,
        secondaryUsedPercent: null,
      }),
    ).toBe(true);
  });

  it('returns false if any present quota is exhausted', () => {
    expect(
      accountHasAvailableQuota({
        ...baseQuota,
        primaryUsedPercent: 100,
        secondaryUsedPercent: 50,
      }),
    ).toBe(false);

    expect(
      accountHasAvailableQuota({
        ...baseQuota,
        primaryUsedPercent: 100,
        secondaryUsedPercent: null,
      }),
    ).toBe(false);
  });

  it('returns false if no quota data is present', () => {
    expect(
      accountHasAvailableQuota({
        ...baseQuota,
        primaryUsedPercent: null,
        secondaryUsedPercent: null,
      }),
    ).toBe(false);
  });
});

describe('resetCreditDisplay', () => {
  it('renders hollow unknown-expiry dots when count exists without expiry', () => {
    expect(
      resetCreditDisplay({
        ...baseQuota,
        resetCreditCount: 2,
        resetCreditExpiresAt: null,
        resetCreditExpirySource: 'unknown',
      }),
    ).toMatchObject({
      dots: [
        { key: 'main-0', color: 'unknown' },
        { key: 'main-1', color: 'unknown' },
      ],
      overflow: 0,
      title: '2 reset credits; expiry unknown',
    });
  });

  it('caps visible dots and surfaces manual expiry source', () => {
    const expiresAt = new Date(Date.now() + 26 * 86_400_000).toISOString();

    const display = resetCreditDisplay({
      ...baseQuota,
      resetCreditCount: 7,
      resetCreditExpiresAt: expiresAt,
      resetCreditExpirySource: 'manual_config',
    });

    expect(display?.dots).toHaveLength(5);
    expect(display?.dots[0].color).toBe('blue');
    expect(display?.overflow).toBe(2);
    expect(display?.title).toContain('manual expiry');
  });

  it('uses nearest per-credit expiry for dot phase and displays API time in Shanghai', () => {
    const display = resetCreditDisplay({
      ...baseQuota,
      resetCreditCount: 2,
      resetCreditExpiresAt: '2026-08-01T00:00:00Z',
      resetCreditExpirySource: 'api',
      resetCreditDetails: [
        { id: 'later', expiresAt: '2026-08-01T00:00:00Z', source: 'api' },
        { id: 'soon', expiresAt: '2026-07-01T00:00:00Z', source: 'api' },
      ],
    });

    expect(display?.title).toContain('2026-07-01T08:00:00+08:00');
  });

  it('hides reset dots for zero or absent count', () => {
    expect(resetCreditDisplay({ ...baseQuota, resetCreditCount: 0 })).toBeNull();
    expect(resetCreditDisplay(baseQuota)).toBeNull();
  });
});
