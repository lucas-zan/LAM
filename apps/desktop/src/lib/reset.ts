export type ResetWindowKind = "session" | "weekly";

const RESET_LOCALE = "en-US";

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

function isSameLocalCalendarDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** Absolute reset time, e.g. "Resets 2:08 PM" or "Resets Jun 11, 2026 11:07 AM". */
export function formatResetAt(
  resetAt?: string | null,
  kind: ResetWindowKind = "session",
): string {
  const date = parseResetAt(resetAt);
  if (!date) return "unknown";
  if (date.getTime() <= Date.now()) return "now";

  const timeOnly: Intl.DateTimeFormatOptions = {
    hour: "numeric",
    minute: "2-digit",
  };
  const dateTime: Intl.DateTimeFormatOptions = {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  };

  const useDateTime =
    kind === "weekly" || (kind !== "session" && !isSameLocalCalendarDay(date, new Date()));
  const formatted = date.toLocaleString(
    RESET_LOCALE,
    useDateTime ? dateTime : timeOnly,
  );
  return `${formatted}`;
}

export function formatResetCountdown(
  resetAt?: string | null,
  kind: ResetWindowKind = "session",
): string {
  return formatResetAt(resetAt, kind);
}
