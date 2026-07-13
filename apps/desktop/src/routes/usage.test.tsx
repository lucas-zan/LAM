import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { UsagePage } from './usage';
import type { UsageCallRow, UsageDashboard, UsageScope, UsageWindow } from '../lib/types';

vi.mock('../lib/api', () => ({
  getCallRawContents: vi.fn(),
}));

function callRow(index: number): UsageCallRow {
  return {
    recordId: `record-${index}`,
    sessionId: `session-${index}`,
    threadName: `thread ${index}`,
    eventTimestamp: '2026-07-03T01:00:00Z',
    sourceFile: `/tmp/session-${index}.jsonl`,
    lineNumber: index,
    cwd: '/repo/LAM',
    model: 'gpt-5',
    effort: 'medium',
    inputTokens: 100_000,
    cachedInputTokens: 90_000,
    uncachedInputTokens: 10_000,
    outputTokens: 10_000,
    reasoningOutputTokens: 1_000,
    totalTokens: 110_000,
    cumulativeTotalTokens: 110_000,
    cacheRatio: 0.9,
    estimatedCostUsd: 0.39,
    pricingModel: 'gpt-5',
    pricingConfidence: 'priced',
    pricingEstimated: false,
  };
}

function dashboard(callCount = 8): UsageDashboard {
  const recentCalls = Array.from({ length: Math.min(callCount, 50) }, (_, index) =>
    callRow(index + 1),
  );
  const topThreads = Array.from({ length: Math.min(callCount, 100) }, (_, index) => {
    const call = callRow(index + 1);
    return {
      threadKey: call.recordId,
      threadLabel: call.threadName ?? call.recordId,
      callCount: 1,
      sessionCount: 1,
      totalTokens: call.totalTokens,
      inputTokens: call.inputTokens,
      cachedInputTokens: call.cachedInputTokens,
      uncachedInputTokens: call.uncachedInputTokens,
      outputTokens: call.outputTokens,
      reasoningOutputTokens: call.reasoningOutputTokens,
      latestEventTimestamp: call.eventTimestamp,
      estimatedCostUsd: call.estimatedCostUsd,
      cacheRatio: call.cacheRatio,
      isArchived: false,
    };
  });
  return {
    refreshedAt: '2026-07-03T01:00:00Z',
    scannedFiles: 2,
    parsedEvents: callCount,
    skippedEvents: 0,
    totalCalls: callCount,
    totalTokens: 1_250_000,
    inputTokens: 1_000_000,
    cachedInputTokens: 900_000,
    uncachedInputTokens: 100_000,
    outputTokens: 250_000,
    reasoningOutputTokens: 50_000,
    estimatedCostUsd: 9.19,
    pricingCoverage: {
      pricedTokens: 1_250_000,
      unpricedTokens: 0,
      pricedTokenRatio: 1,
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
      lifetimeTokens: 1_250_000,
      peakDailyTokens: 500_000,
      longestRunningTurnSec: 360,
      currentStreakDays: 2,
      longestStreakDays: 3,
      source: 'local_sqlite',
      localTotalTokens: 1_250_000,
      codexTotalTokens: null,
      tokenDelta: null,
      tokenDeltaPercent: null,
    },
    activityBuckets: [],
    topThreads,
    recentCalls,
    insights: {
      fastModePercent: 0,
      mostUsedReasoning: 'Medium',
      mostUsedReasoningPercent: 1,
      skillsExplored: 0,
      totalSkillsUsed: 0,
      totalThreads: callCount,
    },
    callsPage: {
      rows: recentCalls,
      total: callCount,
      limit: 50,
      offset: 0,
      nextOffset: callCount > 50 ? 50 : null,
    },
    threadsPage: {
      rows: topThreads,
      total: callCount,
      limit: 100,
      offset: 0,
      nextOffset: callCount > 100 ? 100 : null,
    },
    modelOptions: ['gpt-5'],
    effortOptions: ['medium'],
    pricingConfidenceOptions: ['priced'],
    statusChips: [],
    investigationPresets: [],
  };
}

const scopes: UsageScope[] = [
  { id: 'total', label: 'Total', kind: 'total', isDefault: true },
  { id: 'workspace:main', label: 'main', kind: 'workspace', isDefault: false },
];

const usageWindow: UsageWindow = { preset: 'all', from: null, to: null };

function activityBucket(date: string, calls: number, tokens: number) {
  return {
    date,
    calls,
    tokens,
    cumulativeCalls: calls,
    cumulativeTokens: tokens,
  };
}

function dashboardWithActivity(buckets: UsageDashboard['activityBuckets']) {
  return {
    ...dashboard(4),
    activityBuckets: buckets,
  };
}

function renderUsage(props: Partial<React.ComponentProps<typeof UsagePage>> = {}) {
  return render(
    <UsagePage
      authMode="pat"
      summary={dashboard(80)}
      scopes={scopes}
      activeScopeId="total"
      refreshing={false}
      usageWindow={usageWindow}
      includeArchivedUsage={false}
      usageTab="insights"
      setUsageTab={vi.fn()}
      setUsagePreset={vi.fn()}
      setUsageWindow={vi.fn()}
      setIncludeArchivedUsage={vi.fn()}
      setUsageScope={vi.fn()}
      sectionLoading={{
        scopes: false,
        overview: false,
        activity: false,
        insights: false,
        calls: false,
        threads: false,
        diagnostics: false,
      }}
      loadUsageSection={vi.fn()}
      refreshUsageSections={vi.fn()}
      refreshUsageSection={vi.fn()}
      refreshUsage={vi.fn()}
      {...props}
    />,
  );
}

describe('UsagePage', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-03T10:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('uses full tokens wording without abbreviated tok labels', () => {
    renderUsage({ summary: dashboard(4) });

    expect(screen.getAllByText('1.3M tokens').length).toBeGreaterThan(0);
    expect(screen.queryByText(/tok\\b/i)).toBeNull();
  });

  it('auto syncs the active usage sections every two minutes while mounted', () => {
    const refreshUsageSections = vi.fn();
    renderUsage({ refreshUsageSections });

    act(() => {
      vi.advanceTimersByTime(119_999);
    });
    expect(refreshUsageSections).not.toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(1);
    });

    expect(refreshUsageSections).toHaveBeenCalledWith(
      ['insights', 'overview', 'activity'],
      expect.objectContaining({ scopeId: 'total' }),
    );
  });

  it('skips auto sync while usage is already refreshing', () => {
    const refreshUsageSections = vi.fn();
    renderUsage({ refreshing: true, refreshUsageSections });

    act(() => {
      vi.advanceTimersByTime(120_000);
    });

    expect(refreshUsageSections).not.toHaveBeenCalled();
  });

  it('renders accessible tabs and switches the visible active tab state', () => {
    const setUsageTab = vi.fn();
    const { rerender } = renderUsage({ usageTab: 'insights', setUsageTab });

    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((tab) => tab.textContent)).toContain('Calls');
    fireEvent.click(screen.getByRole('tab', { name: 'Calls' }));
    expect(setUsageTab).toHaveBeenCalledWith('calls');

    rerender(
      <UsagePage
        authMode="pat"
        summary={dashboard(4)}
        scopes={scopes}
        activeScopeId="total"
        refreshing={false}
        usageWindow={usageWindow}
        includeArchivedUsage={false}
        usageTab="calls"
        setUsageTab={setUsageTab}
        setUsagePreset={vi.fn()}
        setUsageWindow={vi.fn()}
        setIncludeArchivedUsage={vi.fn()}
        setUsageScope={vi.fn()}
        sectionLoading={{
          scopes: false,
          overview: false,
          activity: false,
          insights: false,
          calls: false,
          threads: false,
          diagnostics: false,
        }}
        loadUsageSection={vi.fn()}
        refreshUsageSection={vi.fn()}
        refreshUsage={vi.fn()}
      />,
    );

    expect(screen.getByRole('tab', { name: 'Calls' }).getAttribute('aria-selected')).toBe('true');
  });

  it('renders usage scopes as a segmented tab control', () => {
    const setUsageScope = vi.fn();
    renderUsage({ activeScopeId: 'total', setUsageScope });

    const scopeTabs = screen.getByRole('tablist', { name: 'Usage scope' });
    expect(scopeTabs.classList.contains('usageScopeTabs--segmented')).toBe(true);

    const totalTab = within(scopeTabs).getByRole('tab', { name: 'Total' });
    expect(totalTab.getAttribute('aria-selected')).toBe('true');
    expect(totalTab.classList.contains('usageScopeTab--active')).toBe(true);

    fireEvent.click(within(scopeTabs).getByRole('tab', { name: 'main' }));
    expect(setUsageScope).toHaveBeenCalledWith('workspace:main');
  });

  it('renders aggregate activity insights instead of deriving them from loaded rows', () => {
    renderUsage({
      summary: {
        ...dashboard(4),
        recentCalls: [],
        topThreads: [],
        insights: {
          fastModePercent: 0.25,
          mostUsedReasoning: 'Medium',
          mostUsedReasoningPercent: 0.75,
          skillsExplored: 2,
          totalSkillsUsed: 3,
          totalThreads: 80,
        },
      },
      usageTab: 'insights',
    });

    const panel = screen.getByText('Activity insights').closest('section');
    expect(panel).toBeTruthy();
    const insights = within(panel as HTMLElement);
    expect(insights.getByText('Fast Mode')).toBeTruthy();
    expect(insights.getByText('25.0%')).toBeTruthy();
    expect(insights.getByText('Most used reasoning')).toBeTruthy();
    expect(insights.getByText('Medium · 75.0%')).toBeTruthy();
    expect(insights.getByText('Skills explored')).toBeTruthy();
    expect(insights.getByText('2')).toBeTruthy();
    expect(insights.getByText('Total skills used')).toBeTruthy();
    expect(insights.getByText('3')).toBeTruthy();
    expect(insights.getByText('Total threads')).toBeTruthy();
    expect(insights.getByText('80')).toBeTruthy();
    expect(screen.queryByText('Diagnostics brief')).toBeNull();
  });

  it('does not blank when activity data renders before overview data', () => {
    expect(() =>
      renderUsage({
        summary: {
          activityBuckets: [activityBucket('2026-07-03', 1, 100)],
        } as UsageDashboard,
      }),
    ).not.toThrow();
    expect(screen.getByTestId('usage-activity-2026-07-03')).toBeTruthy();
  });

  it('caps expensive calls table rendering and shows a clear note', () => {
    renderUsage({ summary: dashboard(80), usageTab: 'calls' });

    const table = screen.getByRole('table');
    expect(within(table).getAllByRole('row')).toHaveLength(52);
    expect(screen.getByText('Showing 1-50 of 80 matching calls.')).toBeTruthy();
  });

  it('caps expensive threads table rendering and shows a clear note', () => {
    renderUsage({ summary: dashboard(140), usageTab: 'threads' });

    const table = screen.getByRole('table');
    expect(within(table).getAllByRole('row')).toHaveLength(102);
    expect(screen.getByText('Showing 1-100 of 140 threads.')).toBeTruthy();
  });

  it('keeps all-time activity visible through today when recent days have no buckets', () => {
    renderUsage({
      summary: dashboardWithActivity([
        activityBucket('2026-06-29', 3, 300),
        activityBucket('2026-06-30', 4, 400),
      ]),
      usageWindow: { preset: 'all', from: null, to: null },
    });

    expect(screen.getByTestId('usage-activity-2026-06-30')).toBeTruthy();
    expect(screen.getByTestId('usage-activity-2026-07-03')).toBeTruthy();
  });

  it('renders the ending month label when all-time activity ends in a short current month', () => {
    vi.setSystemTime(new Date('2026-07-06T10:00:00Z'));

    renderUsage({
      summary: dashboardWithActivity([
        activityBucket('2026-06-30', 4, 400),
        activityBucket('2026-07-06', 5, 500),
      ]),
      usageWindow: { preset: 'all', from: null, to: null },
    });

    const monthLabels = document.querySelectorAll('.usageHeatmapMonthLabel');
    expect(Array.from(monthLabels).map((label) => label.textContent)).toEqual(
      expect.arrayContaining(['Jun', 'Jul']),
    );
    expect(Array.from(monthLabels).filter((label) => label.textContent === 'Jul')).toHaveLength(2);
  });

  it('renders custom activity ranges without fixed one-year padding', () => {
    renderUsage({
      summary: dashboardWithActivity([
        activityBucket('2026-06-30', 7, 700),
        activityBucket('2026-07-01', 1, 100),
        activityBucket('2026-07-02', 2, 200),
        activityBucket('2026-07-03', 3, 300),
      ]),
      usageWindow: { preset: 'custom', from: '2026-07-01', to: '2026-07-03' },
    });

    expect(screen.queryByTestId('usage-activity-2026-06-30')).toBeNull();
    expect(screen.getByTestId('usage-activity-2026-07-01')).toBeTruthy();
    expect(screen.getByTestId('usage-activity-2026-07-02')).toBeTruthy();
    expect(screen.getByTestId('usage-activity-2026-07-03')).toBeTruthy();
  });

  it('uses daily and weekly token tooltip semantics for activity cells', () => {
    renderUsage({
      summary: dashboardWithActivity([
        activityBucket('2026-05-17', 1, 46_100_000),
        activityBucket('2026-05-18', 1, 100_000_000),
        activityBucket('2026-05-19', 1, 141_800_000),
      ]),
      usageWindow: { preset: 'custom', from: '2026-05-17', to: '2026-05-19' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'Tokens' }));
    expect(screen.getByTestId('usage-activity-2026-05-17').getAttribute('data-tooltip')).toBe(
      '46.1M tokens on May 17',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Weekly' }));
    expect(screen.getByTestId('usage-activity-2026-05-17').getAttribute('data-tooltip')).toBe(
      '287.9M tokens on week of May 17, 2026',
    );
  });

  it('resets calls pagination when the query changes', () => {
    const loadUsageSection = vi.fn();
    renderUsage({
      summary: dashboard(80),
      usageTab: 'calls',
      loadUsageSection,
    });

    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(loadUsageSection).toHaveBeenLastCalledWith(
      'calls',
      expect.objectContaining({ offset: 50 }),
    );

    fireEvent.change(screen.getByPlaceholderText('Thread, cwd, model'), {
      target: { value: 'needle' },
    });

    expect(loadUsageSection).toHaveBeenLastCalledWith(
      'calls',
      expect.objectContaining({ offset: 0, search: 'needle' }),
    );
  });

  it('does not reload calls only because the parent callback identity changed', () => {
    const initialLoader = vi.fn();
    const rerenderLoader = vi.fn();
    const { rerender } = renderUsage({
      usageTab: 'calls',
      loadUsageSection: initialLoader,
    });

    expect(initialLoader).toHaveBeenCalledTimes(1);
    expect(initialLoader).toHaveBeenCalledWith(
      'calls',
      expect.objectContaining({ limit: 50, offset: 0 }),
    );

    rerender(
      <UsagePage
        authMode="pat"
        summary={dashboard(80)}
        scopes={scopes}
        activeScopeId="total"
        refreshing={false}
        usageWindow={usageWindow}
        includeArchivedUsage={false}
        usageTab="calls"
        setUsageTab={vi.fn()}
        setUsagePreset={vi.fn()}
        setUsageWindow={vi.fn()}
        setIncludeArchivedUsage={vi.fn()}
        setUsageScope={vi.fn()}
        sectionLoading={{
          scopes: false,
          overview: false,
          activity: false,
          insights: false,
          calls: false,
          threads: false,
          diagnostics: false,
        }}
        loadUsageSection={rerenderLoader}
        refreshUsageSection={vi.fn()}
        refreshUsage={vi.fn()}
      />,
    );

    expect(rerenderLoader).not.toHaveBeenCalled();
  });
});
