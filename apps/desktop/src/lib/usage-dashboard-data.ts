import type { UsageCallRow } from './types';

export function rowInputTokens(row: UsageCallRow): number {
  return Number(row.inputTokens || 0);
}

export function cachedInputTokens(row: UsageCallRow): number {
  return Number(row.cachedInputTokens || 0);
}

export function uncachedInputTokens(row: UsageCallRow): number {
  return Number(row.uncachedInputTokens || Math.max(rowInputTokens(row) - cachedInputTokens(row), 0));
}

export function outputTokens(row: UsageCallRow): number {
  return Number(row.outputTokens || 0);
}

export function resolveThreadAttachment(row: UsageCallRow) {
  if (row.threadName) return { key: `thread:${row.threadName}`, label: row.threadName };
  return { key: `session:${row.sessionId || 'unknown'}`, label: row.sessionId || 'Unknown thread' };
}

export function chronological(a: UsageCallRow, b: UsageCallRow): number {
  const timeCompare = String(a.eventTimestamp || '').localeCompare(String(b.eventTimestamp || ''));
  if (timeCompare !== 0) return timeCompare;
  return Number(a.cumulativeTotalTokens || 0) - Number(b.cumulativeTotalTokens || 0);
}

export function buildCallAdjacencyIndex(rows: UsageCallRow[]) {
  const threadRows = new Map<string, UsageCallRow[]>();
  rows.forEach((row) => {
    const key = resolveThreadAttachment(row).key;
    threadRows.set(key, [...(threadRows.get(key) ?? []), row]);
  });
  const adjacency = new Map<string, { calls: UsageCallRow[]; index: number; previous: UsageCallRow | null; next: UsageCallRow | null }>();
  threadRows.forEach((calls) => {
    calls.sort(chronological);
    calls.forEach((row, index) => {
      adjacency.set(row.recordId, {
        calls,
        index,
        previous: index > 0 ? calls[index - 1] : null,
        next: index < calls.length - 1 ? calls[index + 1] : null,
      });
    });
  });
  return adjacency;
}

export function adjacentThreadCalls(rows: UsageCallRow[], row: UsageCallRow, adjacency = buildCallAdjacencyIndex(rows)) {
  return adjacency.get(row.recordId) ?? { calls: [row], index: 0, previous: null, next: null };
}

export function compactListSummary(values: Array<string | null | undefined>, fallback = 'Unknown'): string {
  const unique = [...new Set(values.filter(Boolean) as string[])].sort();
  if (!unique.length) return fallback;
  return unique.length === 1 ? unique[0] : `${unique[0]} +${unique.length - 1}`;
}

export function threadLabel(row: UsageCallRow): string {
  return resolveThreadAttachment(row).label;
}

export function modelSummary(rows: UsageCallRow[]): string {
  return compactListSummary(rows.map((row) => row.model));
}
