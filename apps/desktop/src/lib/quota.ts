import type { CodexAccount, UsageQuotaSnapshot } from "./types";

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
  return hasQuotaRemaining(quota.primaryUsedPercent) && hasQuotaRemaining(quota.secondaryUsedPercent);
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
  if (remaining === null) return "N/A";
  return `${remaining}% left`;
}

export function formatQuotaCompactSummary(
  primaryUsed?: number | null,
  secondaryUsed?: number | null,
): string {
  const session = quotaRemainingPercent(primaryUsed);
  const weekly = quotaRemainingPercent(secondaryUsed);
  const sessionText = session === null ? "5h N/A" : `5h ${session}%`;
  const weeklyText = weekly === null ? "7d N/A" : `7d ${weekly}%`;
  return `${sessionText} · ${weeklyText}`;
}

export type QuotaColorState = "safe" | "warn" | "danger" | "empty" | "na";

export function quotaColorState(used?: number | null): QuotaColorState {
  const remaining = quotaRemainingPercent(used);
  if (remaining === null) return "na";
  if (remaining === 0) return "empty";
  if (remaining <= 25) return "danger";
  if (remaining < 75) return "warn";
  return "safe";
}
