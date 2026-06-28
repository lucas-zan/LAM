import type { UsageCallRow, UsageThreadSummary } from './types';
import { cachedInputTokens, outputTokens, threadLabel, uncachedInputTokens } from './usage-dashboard-data';
import { compareValues, textValue } from './usage-dashboard-format';

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function rowAttentionScore(row: UsageCallRow): number {
  const tokenScore = clamp(Number(row.totalTokens || 0) / 2500, 0, 36);
  const lowCacheScore = Number(row.inputTokens || 0) > 0 ? clamp((0.5 - Number(row.cacheRatio || 0)) * 70, 0, 35) : 0;
  return tokenScore + lowCacheScore;
}

export function threadAttentionScore(group: UsageThreadSummary): number {
  const tokenScore = clamp(Number(group.totalTokens || 0) / 3500, 0, 42);
  const lowCacheScore = clamp((0.55 - Number(group.cacheRatio || 0)) * 70, 0, 38);
  return tokenScore + lowCacheScore;
}

function callSortValue(row: UsageCallRow, key: string) {
  if (key === 'attention') return rowAttentionScore(row);
  if (key === 'cache') return Number(row.cacheRatio || 0);
  if (key === 'context') return Number(row.contextWindowPercent || 0);
  if (key === 'cost') return Number(row.estimatedCostUsd || 0);
  if (key === 'duration') return Number(row.threadCallIndex || 0);
  if (key === 'effort') return textValue(row.effort);
  if (key === 'gap') return row.previousRecordId ? 1 : 0;
  if (key === 'initiator') return textValue(row.callInitiator);
  if (key === 'model') return textValue(row.model);
  if (key === 'cached') return cachedInputTokens(row);
  if (key === 'uncached') return uncachedInputTokens(row);
  if (key === 'output') return outputTokens(row);
  if (key === 'reasoning') return Number(row.reasoningOutputTokens || 0);
  if (key === 'thread') return textValue(threadLabel(row));
  if (key === 'time') return String(row.eventTimestamp || '');
  if (key === 'usage') return Number(row.usageCredits || 0);
  return Number(row.totalTokens || 0);
}

export function compareCalls(a: UsageCallRow, b: UsageCallRow, sortKey = 'time', sortDirection: 'asc' | 'desc' = 'desc'): number {
  const comparison = compareValues(callSortValue(a, sortKey), callSortValue(b, sortKey));
  const primary = sortDirection === 'asc' ? comparison : -comparison;
  if (primary !== 0) return primary;
  const timeFallback = String(b.eventTimestamp || '').localeCompare(String(a.eventTimestamp || ''));
  if (timeFallback !== 0) return timeFallback;
  return String(a.recordId || '').localeCompare(String(b.recordId || ''));
}

export function sortedThreadCalls(calls: UsageCallRow[], sortKey = 'time', sortDirection: 'asc' | 'desc' = 'desc'): UsageCallRow[] {
  return calls.slice().sort((a, b) => compareCalls(a, b, sortKey, sortDirection));
}

export function sortThreads(groups: UsageThreadSummary[], sortKey = 'total', sortDirection: 'asc' | 'desc' = 'desc'): UsageThreadSummary[] {
  return groups.slice().sort((a, b) => {
    const values: Record<string, [unknown, unknown]> = {
      attention: [threadAttentionScore(a), threadAttentionScore(b)],
      cache: [a.cacheRatio, b.cacheRatio],
      cost: [a.estimatedCostUsd || 0, b.estimatedCostUsd || 0],
      context: [a.maxContextWindowPercent || 0, b.maxContextWindowPercent || 0],
      duration: [a.callCount, b.callCount],
      effort: ['', ''],
      gap: [a.archivedCallCount || 0, b.archivedCallCount || 0],
      initiator: [textValue(a.callInitiatorSummary), textValue(b.callInitiatorSummary)],
      model: ['', ''],
      reasoning: [a.reasoningOutputTokens || 0, b.reasoningOutputTokens || 0],
      thread: [textValue(a.threadLabel), textValue(b.threadLabel)],
      time: [a.latestEventTimestamp || '', b.latestEventTimestamp || ''],
      total: [a.totalTokens, b.totalTokens],
      cached: [a.cachedInputTokens, b.cachedInputTokens],
      uncached: [a.uncachedInputTokens, b.uncachedInputTokens],
      output: [a.outputTokens, b.outputTokens],
      usage: [a.usageCredits || 0, b.usageCredits || 0],
    };
    const [left, right] = values[sortKey] ?? values.total;
    const comparison = compareValues(left, right);
    const primary = sortDirection === 'asc' ? comparison : -comparison;
    if (primary !== 0) return primary;
    return String(b.latestEventTimestamp || '').localeCompare(String(a.latestEventTimestamp || ''));
  });
}

export function lowCacheThreads(groups: UsageThreadSummary[]): UsageThreadSummary[] {
  return groups.filter((group) => group.cacheRatio < 0.2 && group.inputTokens >= 50_000);
}
