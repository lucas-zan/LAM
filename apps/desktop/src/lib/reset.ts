export function parseResetAt(resetAt?: string | null): Date | null {
  if (!resetAt) return null;
  const trimmed = resetAt.trim();
  if (!trimmed) return null;
  if (/^\d+$/.test(trimmed)) {
    const raw = Number(trimmed);
    if (!Number.isFinite(raw)) return null;
    const millis = raw > 1_000_000_000_000 ? raw : raw * 1000;
    return new Date(millis);
  }
  const asDate = new Date(trimmed);
  return Number.isNaN(asDate.getTime()) ? null : asDate;
}

export function formatResetCountdown(resetAt?: string | null): string {
  const date = parseResetAt(resetAt);
  if (!date) return "Resets: unknown";
  const diffMs = date.getTime() - Date.now();
  if (diffMs <= 0) return "Resets now";
  const totalMinutes = Math.floor(diffMs / 60000);
  const days = Math.floor(totalMinutes / (24 * 60));
  const hours = Math.floor((totalMinutes % (24 * 60)) / 60);
  const mins = totalMinutes % 60;
  if (days > 0) return `Resets in ${days}d ${hours}h`;
  if (hours > 0) return `Resets in ${hours}h ${mins}m`;
  return `Resets in ${mins}m`;
}
