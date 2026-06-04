export function quotaRemainingPercent(used?: number | null): number | null {
  if (used === null || used === undefined) return null;
  return Math.max(0, 100 - used);
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
