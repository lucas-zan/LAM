import { describe, expect, it } from 'vitest';
import { compareCalls, sortedThreadCalls, sortThreads } from './usage-dashboard-analysis';
import { uncachedInputTokens } from './usage-dashboard-data';
import { compareValues, formatTimestamp } from './usage-dashboard-format';
import { formatCost } from './usage-pricing';
import { summarizeUsageDiagnostics } from './usage-diagnostics';
import type { UsageCallRow, UsageThreadSummary } from './types';

const call = (overrides: Partial<UsageCallRow>): UsageCallRow => ({
  recordId: 'a',
  sessionId: 's',
  threadName: null,
  eventTimestamp: '2026-06-28T00:00:00Z',
  sourceFile: '/tmp/a.jsonl',
  lineNumber: 1,
  cwd: null,
  model: null,
  effort: null,
  inputTokens: 0,
  cachedInputTokens: 0,
  uncachedInputTokens: 0,
  outputTokens: 0,
  reasoningOutputTokens: 0,
  totalTokens: 0,
  cumulativeTotalTokens: 0,
  cacheRatio: 0,
  ...overrides,
});

const thread = (overrides: Partial<UsageThreadSummary>): UsageThreadSummary => ({
  threadKey: 'thread:a',
  threadLabel: 'a',
  callCount: 1,
  totalTokens: 0,
  inputTokens: 0,
  cachedInputTokens: 0,
  uncachedInputTokens: 0,
  outputTokens: 0,
  latestEventTimestamp: null,
  cacheRatio: 0,
  ...overrides,
});

describe('usage dashboard helpers', () => {
  it('clamps uncached tokens when cached exceeds input', () => {
    expect(uncachedInputTokens(call({ inputTokens: 10, cachedInputTokens: 20 }))).toBe(0);
  });

  it('sorts calls by timestamp then record id', () => {
    expect(compareCalls(call({ recordId: 'b' }), call({ recordId: 'a' }), 'time', 'desc')).toBeGreaterThan(0);
  });

  it('groups thread sorting by thread name before session id', () => {
    const rows = sortThreads([thread({ threadLabel: 'z' }), thread({ threadLabel: 'a' })], 'thread', 'asc');
    expect(rows.map((row) => row.threadLabel)).toEqual(['a', 'z']);
  });

  it('supports reference usage sort keys', () => {
    const keys = [
      'time',
      'duration',
      'gap',
      'attention',
      'thread',
      'initiator',
      'model',
      'effort',
      'total',
      'cached',
      'uncached',
      'output',
      'reasoning',
      'cost',
      'usage',
      'cache',
      'context',
    ];
    const calls = [
      call({ recordId: 'a', totalTokens: 1, cachedInputTokens: 1, eventTimestamp: '2026-06-28T00:00:00Z' }),
      call({ recordId: 'b', totalTokens: 2, cachedInputTokens: 2, eventTimestamp: '2026-06-28T00:00:01Z' }),
    ];

    for (const key of keys) {
      expect(sortedThreadCalls(calls, key, 'desc')).toHaveLength(2);
    }
  });

  it('format helpers tolerate malformed timestamps', () => {
    expect(formatTimestamp('bad')).toBe('bad');
    expect(compareValues('a', 'b')).toBeLessThan(0);
  });

  it('formats backend-estimated costs without a frontend rate card', () => {
    expect(formatCost(12.345)).toBe('$12.35');
    expect(formatCost(null)).toBe('$0.00');
  });

  it('summarizes diagnostics without raw content', () => {
    const diagnostics = summarizeUsageDiagnostics({
      parserDiagnostics: { unknown_event_msg: 2 },
      skippedEvents: 3,
      unknownModels: ['unknown-model'],
      lowCacheThreads: [thread({ threadLabel: 'low-cache', inputTokens: 80_000, cacheRatio: 0.1 })],
      highContextCalls: [call({ recordId: 'context-call', contextWindowPercent: 0.91 })],
      lastRefreshError: null,
    });

    expect(diagnostics).toContain('unknown_event_msg');
    expect(diagnostics).toContain('unknown-model');
    expect(diagnostics).not.toContain('fake prompt');
  });
});
