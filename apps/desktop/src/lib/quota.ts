import type { CodexAccount, UsageQuotaSnapshot } from './types';

export type QuotaWindowVariant = 'session' | 'weekly' | 'monthly';

export type QuotaDisplayWindow = {
  key: 'primary' | 'secondary';
  label: string;
  shortLabel: string;
  usedPercent: number | null | undefined;
  resetAt: string | null | undefined;
  variant: QuotaWindowVariant;
};

export type ResetCreditDot = {
  key: string;
  color: 'blue' | 'green' | 'yellow' | 'red' | 'black' | 'unknown';
};

export type ResetCreditDisplay = {
  dots: ResetCreditDot[];
  overflow: number;
  title: string;
};

export function quotaRemainingPercent(used?: number | null): number | null {
  if (used === null || used === undefined) return null;
  return Math.max(0, 100 - used);
}

export function hasQuotaRemaining(used?: number | null): boolean {
  const remaining = quotaRemainingPercent(used);
  return remaining !== null && remaining > 0;
}

export function accountHasAvailableQuota(quota?: UsageQuotaSnapshot): boolean {
  if (!quota) return false;

  const pUsed = quota.primaryUsedPercent;
  const sUsed = quota.secondaryUsedPercent;

  const hasP = pUsed !== null && pUsed !== undefined;
  const hasS = sUsed !== null && sUsed !== undefined;

  if (hasP && hasS) {
    return hasQuotaRemaining(pUsed) && hasQuotaRemaining(sUsed);
  }
  if (hasP) {
    return hasQuotaRemaining(pUsed);
  }
  if (hasS) {
    return hasQuotaRemaining(sUsed);
  }
  return false;
}

export function filterQuotaSnapshotsForAccounts(
  accounts: CodexAccount[],
  quotas: UsageQuotaSnapshot[],
): UsageQuotaSnapshot[] {
  const accountIds = new Set(accounts.map((account) => account.id));
  return quotas.filter((quota) => accountIds.has(quota.profileId));
}

export function filterQuotaSnapshotsForProfileIds(
  profileIds: string[],
  quotas: UsageQuotaSnapshot[],
): UsageQuotaSnapshot[] {
  const accountIds = new Set(profileIds);
  return quotas.filter((quota) => accountIds.has(quota.profileId));
}

export function countAccountsWithQuotaData(
  accounts: CodexAccount[],
  quotas: UsageQuotaSnapshot[],
): number {
  return filterQuotaSnapshotsForAccounts(accounts, quotas).filter(
    (quota) => quota.primaryUsedPercent !== null && quota.primaryUsedPercent !== undefined,
  ).length;
}

export function countAccountsWithAvailableQuota(
  accounts: CodexAccount[],
  quotas: UsageQuotaSnapshot[],
): number {
  return accounts.filter((account) => {
    const quota = quotas.find((item) => item.profileId === account.id);
    return accountHasAvailableQuota(quota);
  }).length;
}

export function mergeQuotaSnapshots(
  current: UsageQuotaSnapshot[],
  snapshot: UsageQuotaSnapshot,
): UsageQuotaSnapshot[] {
  const next = new Map(current.map((item) => [item.profileId, item]));
  next.set(snapshot.profileId, snapshot);
  return Array.from(next.values());
}

export function planTypeLabel(planType?: string | null): string | null {
  const trimmed = planType?.trim();
  return trimmed ? trimmed.toUpperCase() : null;
}

export function quotaDisplayWindows(quota?: UsageQuotaSnapshot | null): QuotaDisplayWindow[] {
  if (!quota) return [];
  const windows: QuotaDisplayWindow[] = [];
  if (quota.primaryUsedPercent !== null && quota.primaryUsedPercent !== undefined) {
    windows.push({
      key: 'primary',
      ...quotaWindowLabels(quota.primaryWindowDurationMins, 'primary'),
      usedPercent: quota.primaryUsedPercent,
      resetAt: quota.resetAt,
    });
  }
  if (quota.secondaryUsedPercent !== null && quota.secondaryUsedPercent !== undefined) {
    windows.push({
      key: 'secondary',
      ...quotaWindowLabels(quota.secondaryWindowDurationMins, 'secondary'),
      usedPercent: quota.secondaryUsedPercent,
      resetAt: quota.secondaryResetAt,
    });
  }
  return windows;
}

export function resetCreditDisplay(quota?: UsageQuotaSnapshot | null): ResetCreditDisplay | null {
  const count = quota?.resetCreditCount ?? 0;
  if (!quota || count <= 0) return null;
  const visible = Math.min(count, 5);
  const expiresAt = sortedResetCreditDetails(quota)
    .find((credit) => credit.expiresAt)?.expiresAt ?? quota.resetCreditExpiresAt;
  const color = resetCreditColor(expiresAt);
  const source =
    expiresAt && quota.resetCreditExpirySource === 'manual_config'
      ? `manual expiry ${expiresAt}`
      : expiresAt
        ? `expires ${expiresAt}`
        : 'expiry unknown';
  return {
    dots: Array.from({ length: visible }, (_, index) => ({ key: `${quota.profileId}-${index}`, color })),
    overflow: Math.max(0, count - visible),
    title: `${count} reset credits; ${source}`,
  };
}

export function sortedResetCreditDetails(quota?: UsageQuotaSnapshot | null) {
  return [...(quota?.resetCreditDetails ?? [])].sort((a, b) => {
    const aTime = resetCreditTime(a.expiresAt);
    const bTime = resetCreditTime(b.expiresAt);
    if (aTime !== bTime) return aTime - bTime;
    return (a.id ?? '').localeCompare(b.id ?? '');
  });
}

function resetCreditTime(expiresAt?: string | null): number {
  if (!expiresAt) return Number.POSITIVE_INFINITY;
  const parsed = Date.parse(expiresAt);
  return Number.isFinite(parsed) ? parsed : Number.POSITIVE_INFINITY;
}

function resetCreditColor(expiresAt?: string | null): ResetCreditDot['color'] {
  if (!expiresAt) return 'unknown';
  const parsed = Date.parse(expiresAt);
  if (!Number.isFinite(parsed)) return 'unknown';
  const days = Math.ceil((parsed - Date.now()) / 86_400_000);
  if (days > 24) return 'blue';
  if (days >= 19) return 'green';
  if (days >= 13) return 'yellow';
  if (days >= 7) return 'red';
  return 'black';
}

function quotaWindowLabels(
  durationMins: number | null | undefined,
  slot: 'primary' | 'secondary',
): Pick<QuotaDisplayWindow, 'label' | 'shortLabel' | 'variant'> {
  if (durationMins === 300) {
    return { label: 'Session (5h)', shortLabel: '5h', variant: 'session' };
  }
  if (durationMins === 10080) {
    return { label: 'Weekly (7d)', shortLabel: 'weekly', variant: 'weekly' };
  }
  if (durationMins && durationMins >= 28 * 24 * 60 && durationMins <= 32 * 24 * 60) {
    return { label: 'Monthly', shortLabel: 'monthly', variant: 'monthly' };
  }
  if (durationMins) {
    return quotaDurationLabels(durationMins);
  }
  return slot === 'secondary'
    ? { label: 'Weekly (7d)', shortLabel: 'weekly', variant: 'weekly' }
    : { label: 'Session (5h)', shortLabel: '5h', variant: 'session' };
}

function quotaDurationLabels(
  durationMins: number,
): Pick<QuotaDisplayWindow, 'label' | 'shortLabel' | 'variant'> {
  if (durationMins % 1440 === 0) {
    const days = durationMins / 1440;
    return { label: `${days}d`, shortLabel: `${days}d`, variant: 'weekly' };
  }
  if (durationMins % 60 === 0) {
    const hours = durationMins / 60;
    return { label: `${hours}h`, shortLabel: `${hours}h`, variant: 'session' };
  }
  return { label: `${durationMins}m`, shortLabel: `${durationMins}m`, variant: 'session' };
}

/** Mean 5h-window remaining % across accounts that have primary quota data. */
export function averagePrimaryRemainingPercent(quotas: UsageQuotaSnapshot[]): number | null {
  const values = quotas
    .map((quota) => quotaRemainingPercent(quota.primaryUsedPercent))
    .filter((value): value is number => value !== null);
  if (!values.length) return null;
  return Math.round(values.reduce((sum, value) => sum + value, 0) / values.length);
}

export function formatQuotaRemainingLabel(used?: number | null): string {
  const remaining = quotaRemainingPercent(used);
  if (remaining === null) return 'N/A';
  return `${remaining}% left`;
}

export function formatQuotaCompactSummary(
  primaryUsed?: number | null,
  secondaryUsed?: number | null,
): string {
  const session = quotaRemainingPercent(primaryUsed);
  const weekly = quotaRemainingPercent(secondaryUsed);
  const sessionText = session === null ? '5h N/A' : `5h ${session}%`;
  const weeklyText = weekly === null ? '7d N/A' : `7d ${weekly}%`;
  return `${sessionText} · ${weeklyText}`;
}

export type QuotaColorState = 'safe' | 'warn' | 'danger' | 'empty' | 'na';

export function quotaColorState(used?: number | null): QuotaColorState {
  const remaining = quotaRemainingPercent(used);
  if (remaining === null) return 'na';
  if (remaining === 0) return 'empty';
  if (remaining < 25) return 'danger';
  if (remaining < 70) return 'warn';
  return 'safe';
}
