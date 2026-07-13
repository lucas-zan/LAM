import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultUsageSummaryRequest, useUsageStore } from './usage';
import * as api from '../lib/api';
import type { UsageCallRow, UsageDashboard } from '../lib/types';

vi.mock('../lib/api', () => ({
  getUsageDashboardResponse: vi.fn(),
  getUsageScopes: vi.fn(),
  getUsageOverview: vi.fn(),
  getUsageActivity: vi.fn(),
  getUsageInsights: vi.fn(),
  getUsageCalls: vi.fn(),
  getUsageThreads: vi.fn(),
  getUsageDiagnostics: vi.fn(),
  refreshUsageIndex: vi.fn(),
  tryRefreshUsageIndex: vi.fn(),
}));

vi.mock('./app', () => ({
  useAppStore: {
    getState: () => ({
      setError: vi.fn(),
      setStatus: vi.fn(),
    }),
  },
}));

const dashboard: UsageDashboard = {
  refreshedAt: null,
  scannedFiles: 0,
  parsedEvents: 0,
  skippedEvents: 0,
  totalCalls: 0,
  totalTokens: 0,
  inputTokens: 0,
  cachedInputTokens: 0,
  uncachedInputTokens: 0,
  outputTokens: 0,
  reasoningOutputTokens: 0,
  estimatedCostUsd: 0,
  pricingCoverage: {
    pricedTokens: 0,
    unpricedTokens: 0,
    pricedTokenRatio: 0,
    unknownModels: [],
  },
  diagnostics: {
    parserDiagnostics: {},
    skippedEvents: 0,
    unknownModels: [],
    lowCacheThreads: [],
    highContextCalls: [],
    lastRefreshError: null,
  },
  headlineStats: {
    lifetimeTokens: null,
    peakDailyTokens: null,
    longestRunningTurnSec: null,
    currentStreakDays: null,
    longestStreakDays: null,
    source: 'local_sqlite',
    localTotalTokens: 0,
    codexTotalTokens: null,
    tokenDelta: null,
    tokenDeltaPercent: null,
  },
  activityBuckets: [],
  topThreads: [],
  recentCalls: [],
  insights: null,
  callsPage: null,
  threadsPage: null,
  modelOptions: [],
  effortOptions: [],
  pricingConfidenceOptions: [],
  statusChips: [],
  investigationPresets: [],
};

const callRow = {
  recordId: 'call-1',
  sessionId: 'session-1',
  eventTimestamp: '2026-07-03T01:00:00Z',
  sourceFile: '/tmp/session.jsonl',
  lineNumber: 1,
  inputTokens: 1,
  cachedInputTokens: 0,
  uncachedInputTokens: 1,
  outputTokens: 1,
  reasoningOutputTokens: 0,
  totalTokens: 2,
  cumulativeTotalTokens: 2,
  cacheRatio: 0,
  estimatedCostUsd: 0,
  pricingConfidence: 'priced',
  pricingEstimated: false,
} as UsageCallRow;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flushPromises() {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  vi.clearAllMocks();
  useUsageStore.setState({
    summary: null,
    scopes: [],
    activeScopeId: null,
    refreshing: false,
    sectionLoading: {
      scopes: false,
      overview: false,
      activity: false,
      insights: false,
      calls: false,
      threads: false,
      diagnostics: false,
    },
  });
});

describe('useUsageStore', () => {
  it('commits first-screen usage sections independently as each request resolves', async () => {
    const scopes = deferred<Awaited<ReturnType<typeof api.getUsageScopes>>>();
    const overview = deferred<UsageDashboard>();
    const activity = deferred<Awaited<ReturnType<typeof api.getUsageActivity>>>();
    vi.mocked(api.getUsageScopes).mockReturnValue(scopes.promise);
    vi.mocked(api.getUsageOverview).mockReturnValue(overview.promise);
    vi.mocked(api.getUsageActivity).mockReturnValue(activity.promise);

    const pending = useUsageStore.getState().loadUsageSummary();

    expect(useUsageStore.getState().sectionLoading.scopes).toBe(true);
    expect(useUsageStore.getState().sectionLoading.overview).toBe(true);
    expect(useUsageStore.getState().sectionLoading.activity).toBe(true);

    scopes.resolve({
      scopes: [
        { id: 'total', label: 'Total', kind: 'total', isDefault: true },
        { id: 'workspace:main', label: 'main', kind: 'workspace', isDefault: false },
      ],
      activeScopeId: 'total',
    });
    await flushPromises();

    expect(useUsageStore.getState().activeScopeId).toBe('total');
    expect(useUsageStore.getState().scopes.map((scope) => scope.label)).toEqual(['Total', 'main']);
    expect(useUsageStore.getState().sectionLoading.scopes).toBe(false);
    expect(useUsageStore.getState().sectionLoading.overview).toBe(true);
    expect(useUsageStore.getState().sectionLoading.activity).toBe(true);
    expect(useUsageStore.getState().summary).toBeNull();

    overview.resolve({ ...dashboard, totalCalls: 12 });
    await flushPromises();

    expect(useUsageStore.getState().summary?.totalCalls).toBe(12);
    expect(useUsageStore.getState().sectionLoading.overview).toBe(false);
    expect(useUsageStore.getState().sectionLoading.activity).toBe(true);

    activity.resolve([
      { date: '2026-07-03', calls: 2, tokens: 20, cumulativeCalls: 2, cumulativeTokens: 20 },
    ]);
    await pending;

    expect(useUsageStore.getState().summary?.activityBuckets).toEqual([
      { date: '2026-07-03', calls: 2, tokens: 20, cumulativeCalls: 2, cumulativeTokens: 20 },
    ]);
    expect(useUsageStore.getState().sectionLoading.scopes).toBe(false);
    expect(useUsageStore.getState().sectionLoading.overview).toBe(false);
    expect(useUsageStore.getState().sectionLoading.activity).toBe(false);
  });

  it('does not let a late overview response clear independently loaded activity buckets', async () => {
    const scopes = deferred<Awaited<ReturnType<typeof api.getUsageScopes>>>();
    const overview = deferred<UsageDashboard>();
    const activity = deferred<Awaited<ReturnType<typeof api.getUsageActivity>>>();
    vi.mocked(api.getUsageScopes).mockReturnValue(scopes.promise);
    vi.mocked(api.getUsageOverview).mockReturnValue(overview.promise);
    vi.mocked(api.getUsageActivity).mockReturnValue(activity.promise);

    const pending = useUsageStore.getState().loadUsageSummary();
    scopes.resolve({ scopes: [], activeScopeId: 'total' });
    activity.resolve([
      { date: '2026-07-03', calls: 4, tokens: 40, cumulativeCalls: 4, cumulativeTokens: 40 },
    ]);
    await flushPromises();

    expect(useUsageStore.getState().summary?.activityBuckets).toEqual([
      { date: '2026-07-03', calls: 4, tokens: 40, cumulativeCalls: 4, cumulativeTokens: 40 },
    ]);

    overview.resolve({ ...dashboard, activityBuckets: [] });
    await pending;

    expect(useUsageStore.getState().summary?.activityBuckets).toEqual([
      { date: '2026-07-03', calls: 4, tokens: 40, cumulativeCalls: 4, cumulativeTokens: 40 },
    ]);
  });

  it('ignores stale summary responses after a newer scoped request starts', async () => {
    const oldOverview = deferred<UsageDashboard>();
    const oldActivity = deferred<Awaited<ReturnType<typeof api.getUsageActivity>>>();
    const newOverview = deferred<UsageDashboard>();
    const newActivity = deferred<Awaited<ReturnType<typeof api.getUsageActivity>>>();
    vi.mocked(api.getUsageScopes).mockResolvedValue({
      scopes: [
        { id: 'total', label: 'Total', kind: 'total', isDefault: true },
        { id: 'workspace:main', label: 'main', kind: 'workspace', isDefault: false },
      ],
      activeScopeId: 'total',
    });
    vi.mocked(api.getUsageOverview)
      .mockReturnValueOnce(oldOverview.promise)
      .mockReturnValueOnce(newOverview.promise);
    vi.mocked(api.getUsageActivity)
      .mockReturnValueOnce(oldActivity.promise)
      .mockReturnValueOnce(newActivity.promise);

    const oldReq = { ...defaultUsageSummaryRequest(), scopeId: 'workspace:old' };
    const first = useUsageStore.getState().loadUsageSummary(oldReq);
    const second = useUsageStore.getState().loadUsageSummary({
      ...oldReq,
      scopeId: 'workspace:new',
    });

    newOverview.resolve({ ...dashboard, totalCalls: 9 });
    newActivity.resolve([
      { date: '2026-07-04', calls: 9, tokens: 90, cumulativeCalls: 9, cumulativeTokens: 90 },
    ]);
    await second;

    oldOverview.resolve({ ...dashboard, totalCalls: 1 });
    oldActivity.resolve([
      { date: '2026-07-03', calls: 1, tokens: 10, cumulativeCalls: 1, cumulativeTokens: 10 },
    ]);
    await first;

    expect(useUsageStore.getState().summary?.totalCalls).toBe(9);
    expect(useUsageStore.getState().summary?.activityBuckets).toEqual([
      { date: '2026-07-04', calls: 9, tokens: 90, cumulativeCalls: 9, cumulativeTokens: 90 },
    ]);
  });

  it('clears stale summary data immediately when selecting another usage scope', () => {
    useUsageStore.setState({
      summary: { ...dashboard, totalCalls: 12 },
      activeScopeId: 'workspace:old',
    });

    useUsageStore.getState().selectUsageScope('workspace:new');

    expect(useUsageStore.getState().activeScopeId).toBe('workspace:new');
    expect(useUsageStore.getState().summary).toBeNull();
  });

  it('loads usage dashboard response scopes and active dashboard', async () => {
    vi.mocked(api.getUsageScopes).mockResolvedValue({
      scopes: [
        { id: 'total', label: 'Total', kind: 'total', isDefault: true },
        { id: 'workspace:main', label: 'main', kind: 'workspace', isDefault: false },
      ],
      activeScopeId: 'total',
    });
    vi.mocked(api.getUsageOverview).mockResolvedValue(dashboard);
    vi.mocked(api.getUsageActivity).mockResolvedValue([]);

    await useUsageStore.getState().loadUsageSummary();

    expect(api.getUsageScopes).toHaveBeenCalled();
    expect(api.getUsageOverview).toHaveBeenCalled();
    expect(api.getUsageActivity).toHaveBeenCalled();
    expect(api.getUsageInsights).not.toHaveBeenCalled();
    expect(api.getUsageCalls).not.toHaveBeenCalled();
    expect(api.getUsageThreads).not.toHaveBeenCalled();
    expect(api.getUsageDiagnostics).not.toHaveBeenCalled();
    expect(useUsageStore.getState().summary?.totalCalls).toBe(dashboard.totalCalls);
    expect(useUsageStore.getState().activeScopeId).toBe('total');
    expect(useUsageStore.getState().scopes.map((scope) => scope.label)).toEqual(['Total', 'main']);
  });

  it('refreshes only the requested usage section', async () => {
    const calls: string[] = [];
    vi.mocked(api.tryRefreshUsageIndex).mockImplementation(async () => {
      calls.push('index');
      return {
        scannedFiles: 1,
        parsedFiles: 1,
        parsedEvents: 1,
        insertedOrUpdatedEvents: 1,
        skippedEvents: 0,
        dbPath: '/tmp/usage.sqlite3',
        parserDiagnostics: {},
      };
    });
    vi.mocked(api.getUsageCalls).mockResolvedValue({
      rows: [callRow],
      total: 1,
      limit: 50,
      offset: 0,
      nextOffset: null,
    });
    vi.mocked(api.getUsageCalls).mockImplementation(async () => {
      calls.push('calls');
      return {
        rows: [callRow],
        total: 1,
        limit: 50,
        offset: 0,
        nextOffset: null,
      };
    });
    useUsageStore.setState({ summary: { ...dashboard, recentCalls: [] } });

    const pending = useUsageStore.getState().refreshUsageSection('calls');
    expect(useUsageStore.getState().sectionLoading.calls).toBe(true);
    await pending;

    expect(calls).toEqual(['index', 'calls']);
    expect(api.tryRefreshUsageIndex).toHaveBeenCalledTimes(1);
    expect(api.refreshUsageIndex).not.toHaveBeenCalled();
    expect(api.getUsageCalls).toHaveBeenCalledTimes(1);
    expect(api.getUsageOverview).not.toHaveBeenCalled();
    expect(api.getUsageActivity).not.toHaveBeenCalled();
    expect(api.getUsageThreads).not.toHaveBeenCalled();
    expect(useUsageStore.getState().sectionLoading.calls).toBe(false);
    expect(useUsageStore.getState().summary?.recentCalls).toHaveLength(1);
    expect(useUsageStore.getState().summary?.callsPage?.total).toBe(1);
  });

  it('refreshes grouped usage sections after one index refresh', async () => {
    const calls: string[] = [];
    vi.mocked(api.tryRefreshUsageIndex).mockImplementation(async () => {
      calls.push('index');
      return {
        scannedFiles: 1,
        parsedFiles: 1,
        parsedEvents: 1,
        insertedOrUpdatedEvents: 1,
        skippedEvents: 0,
        dbPath: '/tmp/usage.sqlite3',
        parserDiagnostics: {},
      };
    });
    vi.mocked(api.getUsageInsights).mockImplementation(async () => {
      calls.push('insights');
      return {
        fastModePercent: 0.25,
        mostUsedReasoning: 'Medium',
        mostUsedReasoningPercent: 0.75,
        skillsExplored: 2,
        totalSkillsUsed: 3,
        totalThreads: 80,
      };
    });
    vi.mocked(api.getUsageOverview).mockImplementation(async () => {
      calls.push('overview');
      return { ...dashboard, totalCalls: 12 };
    });
    vi.mocked(api.getUsageActivity).mockImplementation(async () => {
      calls.push('activity');
      return [
        { date: '2026-07-03', calls: 2, tokens: 20, cumulativeCalls: 2, cumulativeTokens: 20 },
      ];
    });
    useUsageStore.setState({ summary: { ...dashboard } });

    await useUsageStore.getState().refreshUsageSections(['insights', 'overview', 'activity']);

    expect(api.tryRefreshUsageIndex).toHaveBeenCalledTimes(1);
    expect(api.refreshUsageIndex).not.toHaveBeenCalled();
    expect(calls[0]).toBe('index');
    expect(calls.slice(1).sort()).toEqual(['activity', 'insights', 'overview']);
    expect(useUsageStore.getState().summary?.insights?.totalThreads).toBe(80);
    expect(useUsageStore.getState().summary?.totalCalls).toBe(12);
    expect(useUsageStore.getState().summary?.activityBuckets).toHaveLength(1);
  });

  it('loads usage insights without loading calls or threads', async () => {
    vi.mocked(api.getUsageInsights).mockResolvedValue({
      fastModePercent: 0.25,
      mostUsedReasoning: 'Medium',
      mostUsedReasoningPercent: 0.75,
      skillsExplored: 2,
      totalSkillsUsed: 3,
      totalThreads: 80,
    });
    useUsageStore.setState({ summary: { ...dashboard, recentCalls: [], topThreads: [] } });

    await useUsageStore.getState().loadUsageSection('insights');

    expect(api.getUsageInsights).toHaveBeenCalledTimes(1);
    expect(api.getUsageCalls).not.toHaveBeenCalled();
    expect(api.getUsageThreads).not.toHaveBeenCalled();
    expect(useUsageStore.getState().summary?.insights?.totalThreads).toBe(80);
  });

  it('keeps a newer section request loading when a stale request settles', async () => {
    const stale = deferred<Awaited<ReturnType<typeof api.getUsageCalls>>>();
    const current = deferred<Awaited<ReturnType<typeof api.getUsageCalls>>>();
    vi.mocked(api.getUsageCalls)
      .mockReturnValueOnce(stale.promise)
      .mockReturnValueOnce(current.promise);

    const first = useUsageStore.getState().loadUsageSection('calls');
    const second = useUsageStore.getState().loadUsageSection('calls');
    const emptyPage = { rows: [], total: 0, limit: 50, offset: 0, nextOffset: null };

    stale.resolve(emptyPage);
    await first;
    expect(useUsageStore.getState().sectionLoading.calls).toBe(true);

    current.resolve(emptyPage);
    await second;
    expect(useUsageStore.getState().sectionLoading.calls).toBe(false);
  });

  it('selects the active usage scope without loading data', () => {
    useUsageStore.getState().selectUsageScope('workspace:main');

    expect(useUsageStore.getState().activeScopeId).toBe('workspace:main');
    expect(api.getUsageScopes).not.toHaveBeenCalled();
    expect(api.getUsageOverview).not.toHaveBeenCalled();
    expect(api.getUsageActivity).not.toHaveBeenCalled();
  });
});
