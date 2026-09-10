export type ResetWindowKind = 'session' | 'weekly' | 'monthly';

const RESET_LOCALE = 'en-US';

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
export function formatResetAt(resetAt?: string | null, kind: ResetWindowKind = 'session'): string {
  const date = parseResetAt(resetAt);
  if (!date) return 'unknown';
  if (date.getTime() <= Date.now()) return 'now';

  const timeOnly: Intl.DateTimeFormatOptions = {
    hour: 'numeric',
    minute: '2-digit',
  };
  const dateTime: Intl.DateTimeFormatOptions = {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  };

  const useDateTime =
    kind !== 'session' &&
    (kind === 'weekly' || kind === 'monthly' || !isSameLocalCalendarDay(date, new Date()));
  const formatted = date.toLocaleString(RESET_LOCALE, useDateTime ? dateTime : timeOnly);
  return `${formatted}`;
}

/**
 * Formats relative countdown to reset time in official Antigravity style:
 * e.g. "6 days, 20 hours", "1 hour, 56 minutes", "45 minutes", "30 seconds", or "now"
 */
export function formatOfficialCountdown(
  resetAt?: string | number | Date | null,
  referenceNow: number = Date.now(),
): string {
  if (!resetAt) return 'unknown';
  const date =
    typeof resetAt === 'object' && resetAt instanceof Date
      ? resetAt
      : parseResetAt(String(resetAt));
  if (!date) return 'unknown';

  const diffMs = date.getTime() - referenceNow;
  if (diffMs <= 0) return 'now';

  const diffSecs = Math.floor(diffMs / 1000);
  const days = Math.floor(diffSecs / 86400);
  const hours = Math.floor((diffSecs % 86400) / 3600);
  const minutes = Math.floor((diffSecs % 3600) / 60);
  const seconds = diffSecs % 60;

  if (days > 0) {
    const dayStr = `${days} ${days === 1 ? 'day' : 'days'}`;
    if (hours > 0) {
      return `${dayStr}, ${hours} ${hours === 1 ? 'hour' : 'hours'}`;
    }
    return dayStr;
  }

  if (hours > 0) {
    const hourStr = `${hours} ${hours === 1 ? 'hour' : 'hours'}`;
    if (minutes > 0) {
      return `${hourStr}, ${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`;
    }
    return hourStr;
  }

  if (minutes > 0) {
    return `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`;
  }

  return `${seconds} ${seconds === 1 ? 'second' : 'seconds'}`;
}

export function formatResetCountdown(
  resetAt?: string | null,
  kind: ResetWindowKind = 'session',
): string {
  return formatResetAt(resetAt, kind);
}
