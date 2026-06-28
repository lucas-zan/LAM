const number = new Intl.NumberFormat();
const compact = new Intl.NumberFormat([], { notation: 'compact', maximumFractionDigits: 1 });
const dateTime = new Intl.DateTimeFormat([], {
  month: 'short',
  day: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
});

export function formatNumber(value: number | null | undefined): string {
  return number.format(Math.round(Number(value || 0)));
}

export function formatCompactNumber(value: number | null | undefined): string {
  return compact.format(Number(value || 0));
}

export function formatPercent(value: number | null | undefined): string {
  return `${(Number(value || 0) * 100).toFixed(1)}%`;
}

export function parsedTimestamp(value: string | null | undefined): Date | null {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

export function formatTimestamp(value: string | null | undefined, fallback = 'Unknown'): string {
  const date = parsedTimestamp(value);
  return date ? dateTime.format(date) : value || fallback;
}

export function formatDuration(value: number | null | undefined, fallback = '-'): string {
  if (value === null || value === undefined || !Number.isFinite(Number(value)) || Number(value) < 0) return fallback;
  const seconds = Math.round(Number(value));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remaining = seconds % 60;
  if (minutes < 60) return remaining ? `${minutes}m ${remaining}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes ? `${hours}h ${remainingMinutes}m` : `${hours}h`;
}

export function textValue(value: unknown): string {
  return String(value || '').toLowerCase();
}

export function compareValues(left: unknown, right: unknown): number {
  if (typeof left === 'number' || typeof right === 'number') {
    return Number(left || 0) - Number(right || 0);
  }
  return String(left || '').localeCompare(String(right || ''));
}

export function sortLabel(key: string): string {
  return {
    attention: 'Needs attention',
    cache: 'Cache',
    model: 'Model',
    cached: 'Cached',
    uncached: 'Uncached',
    output: 'Output',
    thread: 'Thread',
    time: 'Time',
    total: 'Tokens',
  }[key] || 'Sort';
}
