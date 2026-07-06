import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { IconRefresh, IconUsage } from '../components/icons';
import { UIButton } from '../components/ui-button';
import type {
  UsageActivityBucket,
  UsageCallRow,
  UsageDashboard,
  UsageDashboardRequest,
  UsageScope,
  UsageSection,
  UsageSectionLoading,
  UsageWindow,
  UsageWindowPreset,
} from '../lib/types';
import { sortThreads } from '../lib/usage-dashboard-analysis';
import {
  formatCompactNumber,
  formatPercent,
  formatTimestamp,
  formatNumber,
} from '../lib/usage-dashboard-format';
import { formatCost } from '../lib/usage-pricing';
import { getCallRawContents, type CallRawContents } from '../lib/api';

type UsageTab = 'insights' | 'calls' | 'threads' | 'diagnostics';
type LoadLimit = 5000 | 10000 | 20000 | 'all';
type HeatmapMetric = 'calls' | 'tokens';
type HeatmapMode = 'daily' | 'weekly' | 'cumulative';
type ActivityRange = { startDate: Date; endDate: Date };
type HeatmapCell =
  | { isEmpty: true; key: string; value: number; title: string; label: string }
  | { isEmpty: false; key: string; date: string; value: number; title: string; label: string };

const visibleCallRowLimit = 50;
const visibleThreadRowLimit = 100;
const usageAutoSyncIntervalMs = 120_000;
const idleSectionLoading: UsageSectionLoading = {
  scopes: false,
  overview: false,
  activity: false,
  insights: false,
  calls: false,
  threads: false,
  diagnostics: false,
};

const loadLimitOptions: Array<[LoadLimit, string]> = [
  [5000, '5,000 calls'],
  [10000, '10,000 calls'],
  [20000, '20,000 calls'],
  ['all', 'All calls'],
];

const timePresetOptions: Array<[UsageWindowPreset, string]> = [
  ['all', 'All time'],
  ['today', 'Today'],
  ['this-week', 'This week'],
  ['last-7-days', 'Last 7 days'],
  ['this-month', 'This month'],
  ['custom', 'Custom range'],
];

const sortOptions = [
  ['time', 'Time'],
  ['duration', 'Duration'],
  ['gap', 'Gap'],
  ['attention', 'Attention'],
  ['thread', 'Thread'],
  ['initiator', 'Initiator'],
  ['model', 'Model'],
  ['effort', 'Effort'],
  ['total', 'Total'],
  ['cached', 'Cached'],
  ['uncached', 'Uncached'],
  ['output', 'Output'],
  ['reasoning', 'Reasoning'],
  ['cost', 'Cost'],
  ['usage', 'Usage'],
  ['cache', 'Cache'],
  ['context', 'Context'],
] as const;

type Props = {
  authMode: 'oauth' | 'pat';
  summary: UsageDashboard | null;
  scopes: UsageScope[];
  activeScopeId: string | null;
  refreshing: boolean;
  usageWindow: UsageWindow;
  includeArchivedUsage: boolean;
  usageTab: UsageTab;
  setUsageTab: (tab: UsageTab) => void;
  setUsagePreset: (preset: UsageWindowPreset) => void;
  setUsageWindow: (updater: (current: UsageWindow) => UsageWindow) => void;
  setIncludeArchivedUsage: (include: boolean) => void;
  setUsageScope: (scopeId: string) => void;
  sectionLoading: UsageSectionLoading;
  loadUsageSection: (section: UsageSection, req?: UsageDashboardRequest) => void;
  refreshUsageSections?: (sections: UsageSection[], req?: UsageDashboardRequest) => void;
  refreshUsageSection: (section: UsageSection, req?: UsageDashboardRequest) => void;
  refreshUsage: () => void;
};

function threadName(call: UsageCallRow) {
  return call.threadName ?? call.sessionId;
}

function durationLabel(call: UsageCallRow) {
  const prev = call.previousRecordId ? 'linked' : 'first';
  return prev === 'first' ? 'first call' : 'thread call';
}

function renderInitiator(initiator: string | null | undefined) {
  const val = String(initiator || '').toLowerCase();
  if (val === 'user') {
    return <span className="initiator-puck initiator-user">User</span>;
  }
  if (val === 'codex') {
    return <span className="initiator-puck initiator-codex">Codex</span>;
  }
  return <span className="initiator-puck initiator-unknown">Unknown</span>;
}

function renderModel(model: string | null | undefined) {
  const name = model || 'unknown';
  return <span className="usageModelTag">{name}</span>;
}

function renderEffort(effort: string | null | undefined) {
  const val = effort || 'unknown';
  return <span className={`usageEffortTag usageEffortTag--${val.toLowerCase()}`}>{val}</span>;
}

function renderCacheRatio(ratio: number) {
  const percent = formatPercent(ratio);
  let className = 'usageCacheTag';
  if (ratio >= 0.8) {
    className += ' usageCacheTag--high';
  } else if (ratio >= 0.3) {
    className += ' usageCacheTag--medium';
  } else {
    className += ' usageCacheTag--low';
  }
  return <span className={className}>{percent}</span>;
}

function formatDuration(seconds: number | null | undefined) {
  if (!seconds || seconds <= 0) return '0m';
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.round((seconds % 3600) / 60);
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

function formatTokenDelta(delta: number | null | undefined, percent: number | null | undefined) {
  if (delta === null || delta === undefined) return null;
  const sign = delta > 0 ? '+' : '';
  return `${sign}${formatCompactNumber(delta)} (${formatPercent(percent ?? 0)})`;
}

function tokenLabel(value: number | null | undefined) {
  return `${formatCompactNumber(value)} tokens`;
}

function formatWholePercent(value: number) {
  return `${Math.round(value * 100)}%`;
}

function titleLabel(value: string) {
  return value
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join(' ');
}

function skillLabel(call: UsageCallRow) {
  return [call.subagentType, call.agentRole, call.agentNickname]
    .map((value) => String(value ?? '').trim())
    .find(Boolean);
}

function activityInsightRows(calls: UsageCallRow[], threads: UsageDashboard['topThreads']) {
  const totalCalls = calls.length;
  const fastEfforts = new Set(['fast', 'low', 'minimal', 'none']);
  const fastCount = calls.filter((call) =>
    fastEfforts.has(String(call.effort ?? '').trim().toLowerCase()),
  ).length;
  const effortCounts = new Map<string, { label: string; count: number }>();
  const skills = new Set<string>();
  let skillUses = 0;

  calls.forEach((call) => {
    const effort = String(call.effort ?? '').trim();
    if (effort) {
      const key = effort.toLowerCase();
      const current = effortCounts.get(key) ?? { label: titleLabel(effort), count: 0 };
      effortCounts.set(key, { ...current, count: current.count + 1 });
    }
    const skill = skillLabel(call);
    if (skill) {
      skillUses += 1;
      skills.add(skill.toLowerCase());
    }
  });

  const topEffort = [...effortCounts.values()].sort(
    (left, right) => right.count - left.count || left.label.localeCompare(right.label),
  )[0];
  return [
    ['Fast Mode', totalCalls ? formatWholePercent(fastCount / totalCalls) : '-'],
    [
      'Most used reasoning',
      topEffort && totalCalls
        ? `${topEffort.label} · ${formatWholePercent(topEffort.count / totalCalls)}`
        : '-',
    ],
    ['Skills explored', formatNumber(skills.size)],
    ['Total skills used', formatNumber(skillUses)],
    ['Total threads', formatNumber(threads.length)],
  ];
}

function diagnosticRows(diagnostics: UsageDashboard['diagnostics'], summary: UsageDashboard | null) {
  const parserRows = Object.entries(diagnostics.parserDiagnostics)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => [titleLabel(key), formatNumber(value)]);
  const unknownModels = summary?.pricingCoverage?.unknownModels ?? diagnostics.unknownModels ?? [];
  return [
    ['Skipped events', formatNumber(diagnostics.skippedEvents)],
    ...parserRows,
    ['Unknown models', unknownModels.length ? unknownModels.join(', ') : 'None'],
    ['Priced token coverage', formatPercent(summary?.pricingCoverage?.pricedTokenRatio ?? 0)],
    ['Parsed events', formatNumber(summary?.parsedEvents ?? 0)],
    ['Last refresh error', diagnostics.lastRefreshError ?? 'None'],
  ];
}

function heatmapValue(bucket: UsageActivityBucket, metric: HeatmapMetric, mode: HeatmapMode) {
  if (mode === 'cumulative') {
    return metric === 'calls' ? bucket.cumulativeCalls : bucket.cumulativeTokens;
  }
  return metric === 'calls' ? bucket.calls : bucket.tokens;
}

function _heatmapBuckets(buckets: UsageActivityBucket[], metric: HeatmapMetric, mode: HeatmapMode) {
  if (mode !== 'weekly') {
    return buckets.map((bucket) => ({
      label: bucket.date.slice(5),
      value: heatmapValue(bucket, metric, mode),
      title: `${bucket.date}: ${formatCompactNumber(heatmapValue(bucket, metric, mode))} ${metric}`,
    }));
  }
  const weekly = new Map<string, number>();
  for (const bucket of buckets) {
    const date = new Date(`${bucket.date}T00:00:00Z`);
    const monday = new Date(date);
    const day = monday.getUTCDay() || 7;
    monday.setUTCDate(monday.getUTCDate() - day + 1);
    const key = monday.toISOString().slice(0, 10);
    weekly.set(key, (weekly.get(key) ?? 0) + (metric === 'calls' ? bucket.calls : bucket.tokens));
  }
  return Array.from(weekly.entries()).map(([date, value]) => ({
    label: date.slice(5),
    value,
    title: `Week of ${date}: ${formatCompactNumber(value)} ${metric}`,
  }));
}

function heatmapLevel(value: number, max: number) {
  if (value <= 0 || max <= 0) return 0;
  return Math.max(1, Math.ceil((value / max) * 4));
}

function dayString(date: Date) {
  return date.toISOString().slice(0, 10);
}

function utcDay(value: string) {
  return new Date(`${value}T00:00:00Z`);
}

function validDay(value: string | null | undefined) {
  if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
  const date = utcDay(value);
  return Number.isNaN(date.getTime()) ? null : value;
}

function addUtcDays(date: Date, days: number) {
  const next = new Date(date);
  next.setUTCDate(next.getUTCDate() + days);
  return next;
}

function startOfUtcWeek(date: Date) {
  const start = new Date(date);
  const day = start.getUTCDay() || 7;
  start.setUTCDate(start.getUTCDate() - day + 1);
  return start;
}

function startOfUtcActivityWeek(date: Date) {
  const start = new Date(date);
  start.setUTCDate(start.getUTCDate() - start.getUTCDay());
  return start;
}

function formatActivityDay(value: string, includeYear = false) {
  const date = utcDay(value);
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    ...(includeYear ? { year: 'numeric' as const } : {}),
    timeZone: 'UTC',
  }).format(date);
}

function resolveActivityRange(
  usageWindow: UsageWindow,
  buckets: UsageActivityBucket[],
): ActivityRange {
  const today = utcDay(dayString(new Date()));
  const dates = buckets.map((bucket) => bucket.date).sort();
  const firstBucket = dates[0] ?? null;
  const lastBucket = dates[dates.length - 1] ?? null;

  if (usageWindow.preset === 'custom') {
    const from = validDay(usageWindow.from) ?? firstBucket ?? dayString(today);
    const to =
      validDay(usageWindow.to) ?? validDay(usageWindow.from) ?? lastBucket ?? dayString(today);
    const startDate = utcDay(from <= to ? from : to);
    const endDate = utcDay(from <= to ? to : from);
    return { startDate, endDate };
  }

  if (usageWindow.preset === 'today') {
    return { startDate: today, endDate: today };
  }

  if (usageWindow.preset === 'last-7-days') {
    return { startDate: addUtcDays(today, -6), endDate: today };
  }

  if (usageWindow.preset === 'this-week') {
    return { startDate: startOfUtcWeek(today), endDate: today };
  }

  if (usageWindow.preset === 'this-month') {
    return {
      startDate: new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), 1)),
      endDate: today,
    };
  }

  const end = lastBucket && lastBucket > dayString(today) ? utcDay(lastBucket) : today;
  return { startDate: addUtcDays(end, -364), endDate: end };
}

function heatmapColumnCount(cellCount: number) {
  return Math.max(1, Math.ceil(cellCount / 7));
}

export function UsagePage({
  summary,
  scopes,
  activeScopeId,
  refreshing,
  usageWindow,
  includeArchivedUsage,
  usageTab,
  setUsageTab,
  setUsagePreset,
  setUsageWindow,
  setIncludeArchivedUsage,
  setUsageScope,
  sectionLoading,
  loadUsageSection,
  refreshUsageSections,
  refreshUsageSection,
  refreshUsage,
}: Props) {
  const [search, setSearch] = useState('');
  const [showFilters, setShowFilters] = useState(() => {
    const runtime = globalThis as typeof globalThis & {
      process?: { env?: { NODE_ENV?: string } };
    };
    return runtime.process?.env?.NODE_ENV === 'test';
  });
  const [model, setModel] = useState('');
  const [effort, setEffort] = useState('');
  const [pricingConfidence, setPricingConfidence] = useState('');
  const [sortKey, setSortKey] = useState('time');
  const [loadLimit, setLoadLimit] = useState<LoadLimit>(5000);
  const [callOffset, setCallOffset] = useState(0);
  const [threadOffset, setThreadOffset] = useState(0);
  const [heatmapMetric, setHeatmapMetric] = useState<HeatmapMetric>('calls');
  const [heatmapMode, setHeatmapMode] = useState<HeatmapMode>('daily');
  const [selectedRecordId, setSelectedRecordId] = useState<string | null>(null);
  const [activeDetailCall, setActiveDetailCall] = useState<UsageCallRow | null>(null);
  const [rawContents, setRawContents] = useState<CallRawContents | null>(null);
  const [isLoadingRaw, setIsLoadingRaw] = useState(false);
  const loading = sectionLoading ?? idleSectionLoading;
  const loadSection = loadUsageSection ?? (() => undefined);
  const refreshSections =
    refreshUsageSections ??
    ((sections: UsageSection[], req?: UsageDashboardRequest) => {
      sections.forEach((section) => refreshUsageSection(section, req));
    });
  const refreshSection = refreshUsageSection ?? (() => undefined);
  const loadSectionRef = useRef(loadSection);
  useEffect(() => {
    loadSectionRef.current = loadSection;
  }, [loadSection]);
  useEffect(() => {
    setCallOffset(0);
    setThreadOffset(0);
  }, [activeScopeId, effort, includeArchivedUsage, model, pricingConfidence, search, sortKey, usageWindow]);
  const detailRequest = useMemo<UsageDashboardRequest>(
    () => ({
      window: usageWindow,
      includeArchived: includeArchivedUsage,
      scopeId: activeScopeId,
      accountId: null,
      search: search.trim() || null,
      model: model || null,
      effort: effort || null,
      pricingConfidence: pricingConfidence || null,
      sortKey,
      sortDirection: 'desc',
      limit:
        usageTab === 'calls'
          ? visibleCallRowLimit
          : usageTab === 'threads'
            ? visibleThreadRowLimit
            : loadLimit === 'all'
              ? null
              : loadLimit,
      offset: usageTab === 'calls' ? callOffset : usageTab === 'threads' ? threadOffset : 0,
    }),
    [
      activeScopeId,
      callOffset,
      effort,
      includeArchivedUsage,
      loadLimit,
      model,
      pricingConfidence,
      search,
      sortKey,
      threadOffset,
      usageTab,
      usageWindow,
    ],
  );

  useEffect(() => {
    if (usageTab === 'insights') {
      loadSectionRef.current('insights', detailRequest);
    } else if (usageTab === 'calls') {
      loadSectionRef.current('calls', detailRequest);
    } else if (usageTab === 'threads') {
      loadSectionRef.current('threads', detailRequest);
    } else if (usageTab === 'diagnostics') {
      loadSectionRef.current('diagnostics', detailRequest);
    }
  }, [detailRequest, usageTab]);

  const activeRefreshSection: UsageSection =
    usageTab === 'calls'
      ? 'calls'
      : usageTab === 'threads'
        ? 'threads'
        : usageTab === 'diagnostics'
          ? 'diagnostics'
          : 'insights';
  const activeRefreshSections = useMemo<UsageSection[]>(
    () => (usageTab === 'insights' ? ['insights', 'overview', 'activity'] : [activeRefreshSection]),
    [activeRefreshSection, usageTab],
  );
  const activeRefreshLoading =
    loading[activeRefreshSection] || (usageTab === 'insights' && loading.activity);
  const refreshSectionsRef = useRef(refreshSections);
  const detailRequestRef = useRef(detailRequest);
  const activeRefreshSectionsRef = useRef(activeRefreshSections);
  const refreshingRef = useRef(refreshing);
  const loadingRef = useRef(loading);
  useEffect(() => {
    refreshSectionsRef.current = refreshSections;
    detailRequestRef.current = detailRequest;
    activeRefreshSectionsRef.current = activeRefreshSections;
    refreshingRef.current = refreshing;
    loadingRef.current = loading;
  }, [activeRefreshSections, detailRequest, loading, refreshSections, refreshing]);
  useEffect(() => {
    const interval = window.setInterval(() => {
      const sections = activeRefreshSectionsRef.current;
      const isActiveSectionLoading = sections.some((section) => loadingRef.current[section]);
      if (refreshingRef.current || isActiveSectionLoading) {
        return;
      }
      refreshSectionsRef.current(sections, detailRequestRef.current);
    }, usageAutoSyncIntervalMs);
    return () => window.clearInterval(interval);
  }, []);
  const refreshActiveSection = () => {
    if (usageTab === 'insights') {
      refreshSections(activeRefreshSections, detailRequest);
      return;
    }
    refreshSection(activeRefreshSection, detailRequest);
  };

  const calls = summary?.recentCalls ?? [];

  const selectedCall = calls.find((call) => call.recordId === selectedRecordId) ?? calls[0] ?? null;
  const visibleCalls = calls;
  const callsPage = summary?.callsPage ?? null;
  const threads = useMemo(() => {
    const rows = sortThreads(summary?.topThreads ?? [], 'total', 'desc');
    return rows;
  }, [summary?.topThreads]);
  const visibleThreads = threads;
  const threadsPage = summary?.threadsPage ?? null;
  const diagnostics = summary?.diagnostics ?? {
    parserDiagnostics: {},
    skippedEvents: 0,
    unknownModels: [],
    lowCacheThreads: [],
    highContextCalls: [],
    lastRefreshError: null,
  };
  const insightRows = useMemo(() => {
    const insights = summary?.insights;
    if (!insights) return activityInsightRows([], []);
    return [
      ['Fast Mode', insights.fastModePercent === null || insights.fastModePercent === undefined ? '-' : formatPercent(insights.fastModePercent)],
      [
        'Most used reasoning',
        insights.mostUsedReasoning
          ? `${insights.mostUsedReasoning} · ${formatPercent(insights.mostUsedReasoningPercent ?? 0)}`
          : '-',
      ],
      ['Skills explored', formatNumber(insights.skillsExplored)],
      ['Total skills used', formatNumber(insights.totalSkillsUsed)],
      ['Total threads', formatNumber(insights.totalThreads)],
    ];
  }, [summary?.insights]);
  const diagnosticsRows = useMemo(
    () => diagnosticRows(diagnostics, summary),
    [diagnostics, summary],
  );
  const heatmapCells = useMemo(() => {
    const buckets = summary?.activityBuckets ?? [];
    const { startDate, endDate } = resolveActivityRange(usageWindow, buckets);

    const bucketMap = new Map<string, (typeof buckets)[number]>();
    buckets.forEach((b) => bucketMap.set(b.date, b));

    const fullBuckets: Array<{
      date: string;
      calls: number;
      tokens: number;
      cumulativeCalls: number;
      cumulativeTokens: number;
    }> = [];

    let cumCalls = 0;
    let cumTokens = 0;
    for (const bucket of buckets) {
      if (bucket.date < dayString(startDate)) {
        cumCalls = bucket.cumulativeCalls;
        cumTokens = bucket.cumulativeTokens;
      }
    }

    const cur = new Date(startDate);
    while (cur <= endDate) {
      const dateStr = cur.toISOString().slice(0, 10);
      const existing = bucketMap.get(dateStr);
      if (existing) {
        cumCalls = existing.cumulativeCalls;
        cumTokens = existing.cumulativeTokens;
        fullBuckets.push({
          date: dateStr,
          calls: existing.calls,
          tokens: existing.tokens,
          cumulativeCalls: cumCalls,
          cumulativeTokens: cumTokens,
        });
      } else {
        fullBuckets.push({
          date: dateStr,
          calls: 0,
          tokens: 0,
          cumulativeCalls: cumCalls,
          cumulativeTokens: cumTokens,
        });
      }
      cur.setUTCDate(cur.getUTCDate() + 1);
    }

    const firstDay = startDate.getUTCDay();
    const items: HeatmapCell[] = [];
    for (let i = 0; i < firstDay; i++) {
      items.push({
        isEmpty: true,
        key: `empty-${i}`,
        value: 0,
        title: '',
        label: '',
      });
    }

    fullBuckets.forEach((b) => {
      const metricLabel = heatmapMetric === 'calls' ? 'calls' : 'tokens';
      const date = new Date(`${b.date}T00:00:00Z`);
      const weekStart = startOfUtcActivityWeek(date);
      const weekStartKey = dayString(weekStart);
      const value = (() => {
        if (heatmapMode === 'cumulative') {
          return heatmapMetric === 'calls' ? b.cumulativeCalls : b.cumulativeTokens;
        }
        if (heatmapMode !== 'weekly') {
          return heatmapMetric === 'calls' ? b.calls : b.tokens;
        }
        let weekVal = 0;
        const temp = new Date(weekStart);
        for (let j = 0; j < 7; j++) {
          const tStr = temp.toISOString().slice(0, 10);
          const tExist = bucketMap.get(tStr);
          if (tExist) {
            weekVal += heatmapMetric === 'calls' ? tExist.calls : tExist.tokens;
          }
          temp.setUTCDate(temp.getUTCDate() + 1);
        }
        return weekVal;
      })();
      const title =
        heatmapMode === 'weekly'
          ? `${formatCompactNumber(value)} ${metricLabel} on week of ${formatActivityDay(weekStartKey, true)}`
          : `${formatCompactNumber(value)} ${metricLabel} on ${formatActivityDay(b.date)}`;

      items.push({
        isEmpty: false,
        key: b.date,
        date: b.date,
        value,
        title,
        label: b.date.slice(5),
      });
    });

    return items;
  }, [summary?.activityBuckets, heatmapMetric, heatmapMode, usageWindow]);

  const maxHeatmapValue = useMemo(() => {
    const vals = heatmapCells.filter((c) => !c.isEmpty).map((c) => c.value);
    return vals.length > 0 ? Math.max(0, ...vals) : 0;
  }, [heatmapCells]);

  const monthLabels = useMemo(() => {
    const labels: Array<{ text: string; colIndex: number }> = [];
    const buckets = summary?.activityBuckets ?? [];
    const { startDate, endDate } = resolveActivityRange(usageWindow, buckets);
    const monthNames = [
      'Jan',
      'Feb',
      'Mar',
      'Apr',
      'May',
      'Jun',
      'Jul',
      'Aug',
      'Sep',
      'Oct',
      'Nov',
      'Dec',
    ];

    const firstDay = startDate.getUTCDay();
    let lastMonth = -1;
    let lastColIndex = -999;
    const dayCount = Math.floor((endDate.getTime() - startDate.getTime()) / 86_400_000) + 1;
    const columns = heatmapColumnCount(
      firstDay + dayCount,
    );

    for (let col = 0; col < columns; col++) {
      const dayOffset = col * 7 - firstDay;
      const colDate = new Date(startDate);
      colDate.setUTCDate(colDate.getUTCDate() + dayOffset);
      if (colDate > endDate) break;

      const month = colDate.getUTCMonth();
      if (month !== lastMonth) {
        if (col - lastColIndex >= 5) {
          lastMonth = month;
          lastColIndex = col;
          labels.push({
            text: monthNames[month],
            colIndex: col,
          });
        }
      }
    }
    const endMonthLabel = monthNames[endDate.getUTCMonth()];
    const endColIndex = Math.max(
      0,
      Math.min(columns - 1, Math.floor((firstDay + dayCount - 1) / 7)),
    );
    if (labels[labels.length - 1]?.text !== endMonthLabel) {
      labels.push({
        text: endMonthLabel,
        colIndex: endColIndex,
      });
    }
    return labels;
  }, [summary?.activityBuckets, usageWindow]);
  const heatmapColumns = heatmapColumnCount(heatmapCells.length);
  const headlineStats = summary?.headlineStats ?? {
    lifetimeTokens: summary?.totalTokens ?? null,
    peakDailyTokens: null,
    longestRunningTurnSec: null,
    currentStreakDays: null,
    longestStreakDays: null,
    source: 'local_sqlite',
    localTotalTokens: summary?.totalTokens ?? 0,
    codexTotalTokens: null,
    tokenDelta: null,
    tokenDeltaPercent: null,
  };
  const parityLabel = formatTokenDelta(headlineStats.tokenDelta, headlineStats.tokenDeltaPercent);

  const hasActiveFilters = useMemo(() => {
    return (
      search !== '' ||
      model !== '' ||
      effort !== '' ||
      pricingConfidence !== '' ||
      usageWindow.preset !== 'all' ||
      usageWindow.from !== null ||
      usageWindow.to !== null ||
      includeArchivedUsage !== false ||
      sortKey !== 'time'
    );
  }, [search, model, effort, pricingConfidence, usageWindow, includeArchivedUsage, sortKey]);

  const handleResetFilters = () => {
    setSearch('');
    setModel('');
    setEffort('');
    setPricingConfidence('');
    setUsagePreset('all');
    setIncludeArchivedUsage(false);
    setSortKey('time');
  };

  return (
    <section className="usageDashboardPage">
      <header className="usageDashboardHeader">
        <div className="usageDashboardTitle">
          <span className="usageDashboardLogo" aria-hidden>
            <IconUsage size={20} />
          </span>
          <div>
            <h2>Usage</h2>
            <p>
              Updated {formatTimestamp(summary?.refreshedAt, 'never')} ·{' '}
              {summary?.scannedFiles ?? 0} files · {summary?.totalCalls ?? 0} calls
              {parityLabel ? <> · Codex parity: {parityLabel}</> : null}
            </p>
          </div>
        </div>
        <div className="usageDashboardControls">
          <label className="usageControlLabelInline">
            <span>Load limit</span>
            <select
              value={String(loadLimit)}
              onChange={(event) =>
                setLoadLimit(
                  event.target.value === 'all' ? 'all' : (Number(event.target.value) as LoadLimit),
                )
              }
            >
              {loadLimitOptions.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <UIButton
            type="button"
            size="md"
            onClick={refreshActiveSection}
            disabled={refreshing || activeRefreshLoading}
          >
            <IconRefresh size={14} />{' '}
            {refreshing || activeRefreshLoading ? 'Refreshing' : 'Refresh'}
          </UIButton>
        </div>
      </header>

      {scopes.length > 1 ? (
        <div
          className="usageScopeTabs usageScopeTabs--segmented"
          role="tablist"
          aria-label="Usage scope"
        >
          {scopes.map((scope, index) => {
            const presetColors = ['#3b82f6', '#a855f7', '#10b981', '#f59e0b', '#ec4899', '#06b6d4'];
            const color =
              scope.id === 'all'
                ? 'var(--accent)'
                : (presetColors[(index - 1) % presetColors.length] ?? 'var(--accent)');
            return (
              <button
                key={scope.id}
                type="button"
                role="tab"
                aria-selected={scope.id === activeScopeId}
                className={`usageScopeTab ${scope.id === activeScopeId ? 'usageScopeTab--active active' : ''}`}
                style={
                  {
                    '--tab-color': color,
                    '--tab-glow': `color-mix(in srgb, ${color} 12%, transparent)`,
                  } as CSSProperties
                }
                onClick={() => setUsageScope(scope.id)}
              >
                {scope.label}
              </button>
            );
          })}
        </div>
      ) : null}

      <div className="usageToolbar">
        <div className="usageStatusChips">
          {(
            summary?.statusChips ?? [
              { label: 'Pricing source', value: 'local rate card' },
              { label: 'Privacy mode', value: 'aggregate only' },
              { label: 'Parser diagnostics', value: String(summary?.skippedEvents ?? 0) },
            ]
          ).map((chip) => (
            <span key={chip.label}>
              {chip.label}: <strong>{chip.value}</strong>
            </span>
          ))}
          <span>
            Scope: <strong>{includeArchivedUsage ? 'All history' : 'Active'}</strong>
          </span>
        </div>

        <button
          type="button"
          className={`usageFilterToggleBtn ${showFilters ? 'isActive' : ''}`}
          onClick={() => setShowFilters(!showFilters)}
        >
          <svg
            width="13"
            height="13"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            {showFilters ? (
              <path d="M18 6L6 18M6 6l12 12" />
            ) : (
              <path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6" />
            )}
          </svg>
          <span>{showFilters ? 'Hide Filters' : 'Filters'}</span>
        </button>
      </div>

      {showFilters && (
        <div className="usageFilterPanel">
          <label>
            Search
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Thread, cwd, model"
            />
          </label>
          <label>
            Model
            <select value={model} onChange={(event) => setModel(event.target.value)}>
              <option value="">All models</option>
              {(summary?.modelOptions ?? []).map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label>
            Reasoning effort
            <select value={effort} onChange={(event) => setEffort(event.target.value)}>
              <option value="">All efforts</option>
              {(summary?.effortOptions ?? []).map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label>
            Pricing confidence
            <select
              value={pricingConfidence}
              onChange={(event) => setPricingConfidence(event.target.value)}
            >
              <option value="">All pricing</option>
              {(summary?.pricingConfidenceOptions ?? []).map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label>
            Time preset
            <select
              value={usageWindow.preset}
              onChange={(event) => setUsagePreset(event.target.value as UsageWindowPreset)}
            >
              {timePresetOptions.map(([preset, label]) => (
                <option key={preset} value={preset}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Custom start
            <input
              type="date"
              value={usageWindow.from ?? ''}
              onChange={(event) =>
                setUsageWindow((current) => ({
                  ...current,
                  preset: 'custom',
                  from: event.target.value || null,
                }))
              }
            />
          </label>
          <label>
            Custom end
            <input
              type="date"
              value={usageWindow.to ?? ''}
              onChange={(event) =>
                setUsageWindow((current) => ({
                  ...current,
                  preset: 'custom',
                  to: event.target.value || null,
                }))
              }
            />
          </label>
          <label>
            Sort
            <select value={sortKey} onChange={(event) => setSortKey(event.target.value)}>
              {sortOptions.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            History
            <select
              value={includeArchivedUsage ? 'all' : 'active'}
              onChange={(event) => setIncludeArchivedUsage(event.target.value === 'all')}
            >
              <option value="active">Active sessions only</option>
              <option value="all">All history</option>
            </select>
          </label>
          <label>
            Actions
            <UIButton
              type="button"
              variant="default"
              size="sm"
              className="filterResetBtn"
              disabled={!hasActiveFilters}
              onClick={handleResetFilters}
            >
              Reset Filters
            </UIButton>
          </label>
        </div>
      )}

      <div className="usageOverviewCardsGrid">
        <div className={`usageMetricCard ${refreshing || activeRefreshLoading ? 'isRefreshing' : ''}`}>
          <div className="cardHeader">
            <span className="cardIcon">
              <svg
                width="13"
                height="13"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <rect x="2" y="2" width="20" height="8" rx="2" />
                <rect x="2" y="14" width="20" height="8" rx="2" />
                <line x1="6" y1="6" x2="6.01" y2="6" />
                <line x1="6" y1="18" x2="6.01" y2="18" />
              </svg>
            </span>
            <h4>Token Volumes</h4>
          </div>
          <div className="cardStats">
            <div className="statItem main">
              <span>Total Tokens</span>
              <strong>{tokenLabel(summary?.totalTokens)}</strong>
            </div>
            <div className="statSubGrid">
              <div className="statItem">
                <span>Lifetime</span>
                <strong>{formatCompactNumber(headlineStats.lifetimeTokens)}</strong>
              </div>
              <div className="statItem">
                <span>Cached Input</span>
                <strong>{formatCompactNumber(summary?.cachedInputTokens)}</strong>
              </div>
              <div className="statItem">
                <span>Uncached</span>
                <strong>{formatCompactNumber(summary?.uncachedInputTokens)}</strong>
              </div>
              <div className="statItem">
                <span>Reasoning</span>
                <strong>{formatCompactNumber(summary?.reasoningOutputTokens)}</strong>
              </div>
            </div>
          </div>
        </div>

        <div className={`usageMetricCard ${refreshing || activeRefreshLoading ? 'isRefreshing' : ''}`}>
          <div className="cardHeader">
            <span className="cardIcon">
              <svg
                width="13"
                height="13"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
              </svg>
            </span>
            <h4>Productivity & Streaks</h4>
          </div>
          <div className="cardStats">
            <div className="statItem main">
              <span>Peak Daily Tokens</span>
              <strong>{tokenLabel(headlineStats.peakDailyTokens)}</strong>
            </div>
            <div className="statSubGrid">
              <div className="statItem">
                <span>Longest Task</span>
                <strong>{formatDuration(headlineStats.longestRunningTurnSec)}</strong>
              </div>
              <div className="statItem">
                <span>Current Streak</span>
                <strong>{formatCompactNumber(headlineStats.currentStreakDays)} days</strong>
              </div>
              <div className="statItem">
                <span>Longest Streak</span>
                <strong>{formatCompactNumber(headlineStats.longestStreakDays)} days</strong>
              </div>
              <div className="statItem">
                <span>Total Calls</span>
                <strong>{formatCompactNumber(summary?.totalCalls ?? 0)}</strong>
              </div>
            </div>
          </div>
        </div>

        <div className={`usageMetricCard ${refreshing || activeRefreshLoading ? 'isRefreshing' : ''}`}>
          <div className="cardHeader">
            <span className="cardIcon">
              <svg
                width="13"
                height="13"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <line x1="12" y1="1" x2="12" y2="23" />
                <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
              </svg>
            </span>
            <h4>Cost & Credits</h4>
          </div>
          <div className="cardStats">
            <div className="statItem main">
              <span>Estimated Cost</span>
              <strong className="costAccent">{formatCost(summary?.estimatedCostUsd)}</strong>
            </div>
            <div className="statSubGrid statSubGrid--2x1">
              <div className="statItem">
                <span>Codex Credits</span>
                <strong>{formatCost(summary?.estimatedCostUsd)}</strong>
              </div>
            </div>
          </div>
        </div>
      </div>

      <section className="usagePanel usageHeatmapPanel">
        <div className="usageHeatmapHeader">
          <h3>{heatmapMetric === 'tokens' ? 'Token activity' : 'Call activity'}</h3>
          <div className="usageSegmentedControls">
            <div className="usageTabs usageTabs--compact" role="tablist" aria-label="Activity mode">
              {(['daily', 'weekly', 'cumulative'] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  className={heatmapMode === mode ? 'active' : ''}
                  onClick={() => setHeatmapMode(mode)}
                >
                  {mode === 'daily' ? 'Daily' : mode === 'weekly' ? 'Weekly' : 'Cumulative'}
                </button>
              ))}
            </div>
            <div
              className="usageTabs usageTabs--compact"
              role="tablist"
              aria-label="Activity metric"
            >
              {(['calls', 'tokens'] as const).map((metric) => (
                <button
                  key={metric}
                  type="button"
                  className={heatmapMetric === metric ? 'active' : ''}
                  onClick={() => setHeatmapMetric(metric)}
                >
                  {metric === 'calls' ? 'Calls' : 'Tokens'}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="usageHeatmapContent">
          {loading.activity ? <p className="usageSectionLoading">Loading activity...</p> : null}
          <div className="usageHeatmapMain">
            <div className="usageHeatmapGrid" aria-label="Usage activity heatmap">
              {heatmapCells.length ? (
                heatmapCells.map((cell) => {
                  if (cell.isEmpty) {
                    return (
                      <span key={cell.key} className="usageHeatmapCell usageHeatmapCell--empty" />
                    );
                  }
                  return (
                    <span
                      key={cell.key}
                      className={`usageHeatmapCell usageHeatmapCell--${heatmapLevel(cell.value, maxHeatmapValue)}`}
                      data-date={cell.date}
                      data-testid={`usage-activity-${cell.date}`}
                      data-tooltip={cell.title}
                      tabIndex={0}
                    />
                  );
                })
              ) : (
                <p className="usageEmpty">No activity buckets.</p>
              )}
            </div>
            {monthLabels.length ? (
              <div
                className="usageHeatmapMonths"
                aria-hidden="true"
                style={{ gridTemplateColumns: `repeat(${heatmapColumns}, 14px)` }}
              >
                {monthLabels.map((lbl, idx) => (
                  <span
                    key={idx}
                    className="usageHeatmapMonthLabel"
                    style={{ gridColumnStart: lbl.colIndex + 1 }}
                  >
                    {lbl.text}
                  </span>
                ))}
              </div>
            ) : null}
          </div>

          <div className="usageHeatmapSidebar">
            <div>
              <span className="legendLabel">Activity Level</span>
              <div className="legendColors">
                <span className="legendColorCell usageHeatmapCell--0" title="No activity" />
                <span className="legendColorCell usageHeatmapCell--1" title="Low activity" />
                <span className="legendColorCell usageHeatmapCell--2" title="Medium-low activity" />
                <span
                  className="legendColorCell usageHeatmapCell--3"
                  title="Medium-high activity"
                />
                <span className="legendColorCell usageHeatmapCell--4" title="High activity" />
              </div>
            </div>
            <div className="insightItem">
              <label>Range Calls</label>
              <strong>{formatCompactNumber(summary?.totalCalls ?? 0)} calls</strong>
            </div>
            <div className="insightItem">
              <label>Range Tokens</label>
              <strong>{tokenLabel(summary?.totalTokens ?? 0)}</strong>
            </div>
          </div>
        </div>
      </section>

      <div className="usageTabs" role="tablist" aria-label="Usage views">
        {(['insights', 'calls', 'threads', 'diagnostics'] as const).map((tab) => (
          <button
            key={tab}
            type="button"
            role="tab"
            aria-selected={usageTab === tab}
            className={usageTab === tab ? 'active' : ''}
            onClick={() => setUsageTab(tab)}
          >
            {tab === 'insights'
              ? 'Insights'
              : tab === 'calls'
                ? 'Calls'
                : tab === 'threads'
                  ? 'Threads'
                  : 'Diagnostics'}
          </button>
        ))}
      </div>

      {usageTab === 'insights' ? (
        <div className="usageDashboardGrid">
          <section className="usagePanel">
            <h3>Activity insights</h3>
            <div className="usageTableWrap">
              <table className="usageTable">
                <tbody>
                  {insightRows.map(([label, value]) => (
                    <tr key={label}>
                      <td>{label}</td>
                      <td>{value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      ) : null}

      {usageTab === 'calls' ? (
        <div className="usageDashboardGrid usageDashboardGrid--wide">
          <div className="usageTableWrap">
            {loading.calls ? <p className="usageSectionLoading">Loading calls...</p> : null}
            <table className="usageTable usageTable--wide">
              <thead>
                <tr>
                  {[
                    'Time',
                    'Thread',
                    'Duration',
                    'Prev gap',
                    'Initiated',
                    'Model',
                    'Effort',
                    'Tokens',
                    'Cached',
                    'Uncached',
                    'Output',
                    'Reasoning Output',
                    'Cost',
                    'Cache',
                    'Actions',
                  ].map((label) => {
                    let className = '';
                    if (label === 'Time') className = 'usageColTime';
                    if (label === 'Thread') className = 'usageColThread';
                    if (label === 'Actions') className = 'usageColActions';
                    return (
                      <th key={label} className={className}>
                        {label}
                      </th>
                    );
                  })}
                </tr>
              </thead>
              <tbody>
                {visibleCalls.map((call) => (
                  <tr key={call.recordId} onClick={() => setSelectedRecordId(call.recordId)}>
                    <td className="usageColTime">{formatTimestamp(call.eventTimestamp)}</td>
                    <td className="usageColThread" title={threadName(call)}>
                      {threadName(call)}
                    </td>
                    <td>{durationLabel(call)}</td>
                    <td>{call.previousRecordId ? 'linked' : 'none'}</td>
                    <td>{renderInitiator(call.callInitiator)}</td>
                    <td>{renderModel(call.model)}</td>
                    <td>{renderEffort(call.effort)}</td>
                    <td>{formatCompactNumber(call.totalTokens)}</td>
                    <td>{formatCompactNumber(call.cachedInputTokens)}</td>
                    <td>{formatCompactNumber(call.uncachedInputTokens)}</td>
                    <td>{formatCompactNumber(call.outputTokens)}</td>
                    <td>{formatCompactNumber(call.reasoningOutputTokens)}</td>
                    <td>{formatCost(call.estimatedCostUsd)}</td>
                    <td>{renderCacheRatio(call.cacheRatio)}</td>
                    <td className="usageColActions">
                      <button
                        className="usageBtnDots"
                        title="View details"
                        onClick={async (e) => {
                          e.stopPropagation();
                          setActiveDetailCall(call);
                          setRawContents(null);
                          setIsLoadingRaw(true);
                          try {
                            const data = await getCallRawContents(call.sourceFile, call.lineNumber);
                            setRawContents(data);
                          } catch (err) {
                            console.error('Failed to fetch raw call contents:', err);
                          } finally {
                            setIsLoadingRaw(false);
                          }
                        }}
                      >
                        •••
                      </button>
                    </td>
                  </tr>
                ))}
                {callsPage && (callsPage.total > visibleCalls.length || callsPage.offset > 0) ? (
                  <tr className="usageTableNotice">
                    <td colSpan={15}>
                      Showing {callsPage.offset + 1}-{callsPage.offset + visibleCalls.length} of{' '}
                      {callsPage.total} matching calls.
                      <button
                        type="button"
                        disabled={callsPage.offset === 0 || loading.calls}
                        onClick={() =>
                          setCallOffset(Math.max(0, callsPage.offset - callsPage.limit))
                        }
                      >
                        Previous
                      </button>
                      <button
                        type="button"
                        disabled={!callsPage.nextOffset || loading.calls}
                        onClick={() => setCallOffset(callsPage.nextOffset ?? callsPage.offset)}
                      >
                        Next
                      </button>
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
          <aside className="usagePanel usageDetailPanel">
            <h3>Details</h3>
            {selectedCall ? (
              <dl>
                <dt>Record</dt>
                <dd>{selectedCall.recordId}</dd>
                <dt>Source</dt>
                <dd>
                  {selectedCall.sourceFile}:{selectedCall.lineNumber}
                </dd>
                <dt>Thread call</dt>
                <dd>{selectedCall.threadCallIndex ?? 0}</dd>
                <dt>Context</dt>
                <dd>{formatPercent(selectedCall.contextWindowPercent ?? 0)}</dd>
                <dt>Pricing</dt>
                <dd>
                  {selectedCall.pricingModel ?? 'unknown'} ·{' '}
                  {selectedCall.pricingConfidence ?? 'unknown'}
                </dd>
              </dl>
            ) : (
              <p className="usageEmpty">No calls loaded.</p>
            )}
          </aside>
        </div>
      ) : null}

      {usageTab === 'threads' ? (
        <div className="usageTableWrap">
          {loading.threads ? <p className="usageSectionLoading">Loading threads...</p> : null}
          <table className="usageTable">
            <thead>
              <tr>
                <th className="usageColThread">Thread</th>
                <th>Calls</th>
                <th>Sessions</th>
                <th>Total</th>
                <th>Cache</th>
                <th className="usageColTime">Latest</th>
                <th>Recommendation</th>
                <th>Cost</th>
              </tr>
            </thead>
            <tbody>
              {visibleThreads.map((thread) => (
                <tr key={thread.threadKey}>
                  <td className="usageColThread" title={thread.threadLabel}>
                    {thread.threadLabel}
                  </td>
                  <td>{thread.callCount}</td>
                  <td>{thread.sessionCount ?? 1}</td>
                  <td>{formatCompactNumber(thread.totalTokens)}</td>
                  <td>{renderCacheRatio(thread.cacheRatio)}</td>
                  <td className="usageColTime">{formatTimestamp(thread.latestEventTimestamp)}</td>
                  <td>{thread.primaryRecommendation ?? 'None'}</td>
                  <td>{formatCost(thread.estimatedCostUsd)}</td>
                </tr>
              ))}
              {threadsPage && (threadsPage.total > visibleThreads.length || threadsPage.offset > 0) ? (
                <tr className="usageTableNotice">
                  <td colSpan={8}>
                    Showing {threadsPage.offset + 1}-{threadsPage.offset + visibleThreads.length}{' '}
                    of {threadsPage.total} threads.
                    <button
                      type="button"
                      disabled={threadsPage.offset === 0 || loading.threads}
                      onClick={() =>
                        setThreadOffset(Math.max(0, threadsPage.offset - threadsPage.limit))
                      }
                    >
                      Previous
                    </button>
                    <button
                      type="button"
                      disabled={!threadsPage.nextOffset || loading.threads}
                      onClick={() => setThreadOffset(threadsPage.nextOffset ?? threadsPage.offset)}
                    >
                      Next
                    </button>
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </div>
      ) : null}

      {usageTab === 'diagnostics' ? (
        <div className="usageDashboardGrid">
          {loading.diagnostics ? (
            <p className="usageSectionLoading">Loading diagnostics...</p>
          ) : null}
          <section className="usagePanel">
            <h3>Parser diagnostics</h3>
            <div className="usageTableWrap">
              <table className="usageTable">
                <tbody>
                  {diagnosticsRows.map(([label, value]) => (
                    <tr key={label}>
                      <td>{label}</td>
                      <td>{value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
          <section className="usagePanel">
            <h3>Aggregate facts</h3>
            <div className="usageTableWrap">
              <table className="usageTable">
                <tbody>
                  {Object.entries(diagnostics.parserDiagnostics).map(([key, value]) => (
                    <tr key={key}>
                      <td>{key}</td>
                      <td>{value}</td>
                    </tr>
                  ))}
                  {(summary?.pricingCoverage?.unknownModels ?? []).map((value) => (
                    <tr key={value}>
                      <td>unknown_model</td>
                      <td>{value}</td>
                    </tr>
                  ))}
                  <tr>
                    <td>credit_coverage</td>
                    <td>{formatPercent(summary?.pricingCoverage?.pricedTokenRatio ?? 0)}</td>
                  </tr>
                  <tr>
                    <td>source_file_refresh_state</td>
                    <td>{summary?.parsedEvents ?? 0} parsed events</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>
        </div>
      ) : null}

      {activeDetailCall && (
        <div className="usageModalOverlay" onClick={() => setActiveDetailCall(null)}>
          <div className="usageModal" onClick={(e) => e.stopPropagation()}>
            <div className="usageModalHeader">
              <h3>Call Details</h3>
              <button className="usageModalClose" onClick={() => setActiveDetailCall(null)}>
                &times;
              </button>
            </div>
            <div className="usageModalBody">
              <div className="usageModalGrid">
                <div className="usageModalSection">
                  <h4>Metadata</h4>
                  <dl>
                    <dt>Record ID</dt>
                    <dd className="mono">{activeDetailCall.recordId}</dd>
                    <dt>Session ID</dt>
                    <dd className="mono">{activeDetailCall.sessionId}</dd>
                    <dt>Timestamp</dt>
                    <dd>{activeDetailCall.eventTimestamp}</dd>
                    <dt>Source Location</dt>
                    <dd className="mono">
                      {activeDetailCall.sourceFile}:{activeDetailCall.lineNumber}
                    </dd>
                    <dt>CWD</dt>
                    <dd className="mono">{activeDetailCall.cwd ?? '-'}</dd>
                  </dl>
                </div>

                <div className="usageModalSection">
                  <h4>Model & Cost</h4>
                  <dl>
                    <dt>Model</dt>
                    <dd className="mono">{activeDetailCall.model ?? '-'}</dd>
                    <dt>Effort</dt>
                    <dd>{activeDetailCall.effort ?? '-'}</dd>
                    <dt>Pricing Model</dt>
                    <dd>
                      {activeDetailCall.pricingModel ?? 'unknown'} (
                      {activeDetailCall.pricingConfidence ?? 'unknown'})
                    </dd>
                    <dt>Estimated Cost</dt>
                    <dd className="cost-val">{formatCost(activeDetailCall.estimatedCostUsd)}</dd>
                    <dt>Context Window</dt>
                    <dd>
                      {activeDetailCall.modelContextWindow
                        ? `${formatNumber(activeDetailCall.modelContextWindow)} tokens`
                        : '-'}
                    </dd>
                    <dt>Context Window %</dt>
                    <dd>
                      {activeDetailCall.contextWindowPercent
                        ? formatPercent(activeDetailCall.contextWindowPercent)
                        : '-'}
                    </dd>
                  </dl>
                </div>

                <div className="usageModalSection">
                  <h4>Tokens</h4>
                  <dl>
                    <dt>Total Tokens</dt>
                    <dd>
                      <strong>{formatNumber(activeDetailCall.totalTokens)}</strong>
                    </dd>
                    <dt>Input Tokens</dt>
                    <dd>{formatNumber(activeDetailCall.inputTokens)}</dd>
                    <dt>Cached Input</dt>
                    <dd>{formatNumber(activeDetailCall.cachedInputTokens)}</dd>
                    <dt>Uncached Input</dt>
                    <dd>{formatNumber(activeDetailCall.uncachedInputTokens)}</dd>
                    <dt>Output Tokens</dt>
                    <dd>{formatNumber(activeDetailCall.outputTokens)}</dd>
                    <dt>Reasoning Output</dt>
                    <dd>{formatNumber(activeDetailCall.reasoningOutputTokens)}</dd>
                  </dl>
                </div>

                <div className="usageModalSection">
                  <h4>Initiator & Context</h4>
                  <dl>
                    <dt>Initiator</dt>
                    <dd>{activeDetailCall.callInitiator ?? '-'}</dd>
                    <dt>Initiator Reason</dt>
                    <dd>{activeDetailCall.callInitiatorReason ?? '-'}</dd>
                    <dt>Confidence</dt>
                    <dd>{activeDetailCall.callInitiatorConfidence ?? '-'}</dd>
                    <dt>Agent Nickname</dt>
                    <dd>{activeDetailCall.agentNickname ?? '-'}</dd>
                    <dt>Agent Role</dt>
                    <dd>{activeDetailCall.agentRole ?? '-'}</dd>
                    <dt>Parent Session ID</dt>
                    <dd className="mono">{activeDetailCall.parentSessionId ?? '-'}</dd>
                  </dl>
                </div>

                <div className="usageModalFullSection">
                  <h4>Request content</h4>
                  <div className="usageRawContentBox">
                    {isLoadingRaw ? (
                      <div className="usageContentLoading">Loading request details...</div>
                    ) : rawContents?.request ? (
                      <pre className="usageRawPre">{rawContents.request}</pre>
                    ) : (
                      <div className="usageContentEmpty">
                        No request content found in this call window.
                      </div>
                    )}
                  </div>
                </div>

                <div className="usageModalFullSection">
                  <h4>Assistant output</h4>
                  <div className="usageRawContentBox">
                    {isLoadingRaw ? (
                      <div className="usageContentLoading">Loading assistant output...</div>
                    ) : rawContents?.assistant ? (
                      <pre className="usageRawPre">{rawContents.assistant}</pre>
                    ) : (
                      <div className="usageContentEmpty">
                        No assistant output found in this call window.
                      </div>
                    )}
                  </div>
                </div>

                <div className="usageModalFullSection">
                  <h4>Tool output</h4>
                  <div className="usageRawContentBox">
                    {isLoadingRaw ? (
                      <div className="usageContentLoading">Loading tool output...</div>
                    ) : rawContents?.toolOutput ? (
                      <pre className="usageRawPre">{rawContents.toolOutput}</pre>
                    ) : (
                      <div className="usageContentEmpty">
                        No tool output found in this call window.
                      </div>
                    )}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
