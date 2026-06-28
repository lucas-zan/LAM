import { useMemo, useState } from 'react';
import { IconRefresh, IconUsage } from '../components/icons';
import { UIButton } from '../components/ui-button';
import type { UsageCallRow, UsageDashboard, UsageWindow, UsageWindowPreset } from '../lib/types';
import { lowCacheThreads, sortThreads, sortedThreadCalls } from '../lib/usage-dashboard-analysis';
import { formatCompactNumber, formatPercent, formatTimestamp, formatNumber } from '../lib/usage-dashboard-format';
import { summarizeUsageDiagnostics } from '../lib/usage-diagnostics';
import { formatCost } from '../lib/usage-pricing';
import { getCallRawContents, type CallRawContents } from '../lib/api';

type UsageTab = 'insights' | 'calls' | 'threads' | 'diagnostics';
type LoadLimit = 5000 | 10000 | 20000 | 'all';

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
  refreshing: boolean;
  usageWindow: UsageWindow;
  includeArchivedUsage: boolean;
  usageTab: UsageTab;
  setUsageTab: (tab: UsageTab) => void;
  setUsagePreset: (preset: UsageWindowPreset) => void;
  setUsageWindow: (updater: (current: UsageWindow) => UsageWindow) => void;
  setIncludeArchivedUsage: (include: boolean) => void;
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
  return (
    <span className={`usageEffortTag usageEffortTag--${val.toLowerCase()}`}>
      {val}
    </span>
  );
}

function renderCacheRatio(ratio: number) {
  const percent = formatPercent(ratio);
  let className = "usageCacheTag";
  if (ratio >= 0.8) {
    className += " usageCacheTag--high";
  } else if (ratio >= 0.3) {
    className += " usageCacheTag--medium";
  } else {
    className += " usageCacheTag--low";
  }
  return <span className={className}>{percent}</span>;
}

export function UsagePage({
  authMode,
  summary,
  refreshing,
  usageWindow,
  includeArchivedUsage,
  usageTab,
  setUsageTab,
  setUsagePreset,
  setUsageWindow,
  setIncludeArchivedUsage,
  refreshUsage,
}: Props) {
  const [search, setSearch] = useState('');
  const [model, setModel] = useState('');
  const [effort, setEffort] = useState('');
  const [pricingConfidence, setPricingConfidence] = useState('');
  const [sortKey, setSortKey] = useState('time');
  const [loadLimit, setLoadLimit] = useState<LoadLimit>(5000);
  const [selectedRecordId, setSelectedRecordId] = useState<string | null>(null);
  const [activeDetailCall, setActiveDetailCall] = useState<UsageCallRow | null>(null);
  const [rawContents, setRawContents] = useState<CallRawContents | null>(null);
  const [isLoadingRaw, setIsLoadingRaw] = useState(false);

  const calls = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const rows = sortedThreadCalls(summary?.recentCalls ?? [], sortKey, 'desc')
      .filter((call) => !needle || `${threadName(call)} ${call.cwd ?? ''} ${call.model ?? ''}`.toLowerCase().includes(needle))
      .filter((call) => !model || call.model === model)
      .filter((call) => !effort || call.effort === effort)
      .filter((call) => !pricingConfidence || call.pricingConfidence === pricingConfidence);
    return loadLimit === 'all' ? rows : rows.slice(0, loadLimit);
  }, [summary?.recentCalls, search, model, effort, pricingConfidence, sortKey, loadLimit]);

  const selectedCall = calls.find((call) => call.recordId === selectedRecordId) ?? calls[0] ?? null;
  const threads = useMemo(() => {
    const rows = sortThreads(summary?.topThreads ?? [], 'total', 'desc');
    return loadLimit === 'all' ? rows : rows.slice(0, loadLimit);
  }, [summary?.topThreads, loadLimit]);
  const diagnostics = summary?.diagnostics ?? {
    parserDiagnostics: {},
    skippedEvents: 0,
    unknownModels: [],
    lowCacheThreads: [],
    highContextCalls: [],
    lastRefreshError: null,
  };

  if (authMode !== 'pat') {
    return (
      <section className="usageDashboardPage usageEmptyState">
        <h2>Usage</h2>
        <p>Usage statistics are available in PAT mode.</p>
      </section>
    );
  }

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
            Updated {formatTimestamp(summary?.refreshedAt, 'never')} · {summary?.scannedFiles ?? 0} files ·{' '}
            {summary?.totalCalls ?? 0} calls
          </p>
          </div>
        </div>
        <div className="usageDashboardControls">
          <label>
            Load limit
            <select
              value={String(loadLimit)}
              onChange={(event) => setLoadLimit(event.target.value === 'all' ? 'all' : (Number(event.target.value) as LoadLimit))}
            >
              {loadLimitOptions.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <UIButton type="button" size="sm" onClick={refreshUsage} disabled={refreshing}>
            <IconRefresh size={14} /> {refreshing ? 'Refreshing' : 'Refresh'}
          </UIButton>
        </div>
      </header>

      <div className="usageStatusChips">
        {(summary?.statusChips ?? [
          { label: 'Pricing source', value: 'local rate card' },
          { label: 'Privacy mode', value: 'aggregate only' },
          { label: 'Parser diagnostics', value: String(summary?.skippedEvents ?? 0) },
        ]).map((chip) => (
          <span key={chip.label}>
            {chip.label}: <strong>{chip.value}</strong>
          </span>
        ))}
        <span>
          Scope: <strong>{includeArchivedUsage ? 'All history' : 'Active'}</strong>
        </span>
      </div>

      <div className="usageFilterPanel">
        <label>
          Search
          <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Thread, cwd, model" />
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
          <select value={pricingConfidence} onChange={(event) => setPricingConfidence(event.target.value)}>
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
          <select value={usageWindow.preset} onChange={(event) => setUsagePreset(event.target.value as UsageWindowPreset)}>
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
            onChange={(event) => setUsageWindow((current) => ({ ...current, preset: 'custom', from: event.target.value || null }))}
          />
        </label>
        <label>
          Custom end
          <input
            type="date"
            value={usageWindow.to ?? ''}
            onChange={(event) => setUsageWindow((current) => ({ ...current, preset: 'custom', to: event.target.value || null }))}
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
      </div>

      <div className="usageMetricGrid">
        {[
          ['Visible Calls', formatCompactNumber(calls.length)],
          ['Total Tokens', `${formatCompactNumber(summary?.totalTokens)} tok`],
          ['Cached Input', `${formatCompactNumber(summary?.cachedInputTokens)} tok`],
          ['Uncached Input', `${formatCompactNumber(summary?.uncachedInputTokens)} tok`],
          ['Reasoning Output', `${formatCompactNumber(summary?.reasoningOutputTokens)} tok`],
          ['Estimated Cost', formatCost(summary?.estimatedCostUsd)],
          ['Codex Credits', formatCost(summary?.estimatedCostUsd)],
        ].map(([label, value]) => (
          <div className="usageMetric" key={label}>
            <span>{label}</span>
            <strong>{value}</strong>
          </div>
        ))}
      </div>

      <div className="usageTabs" role="tablist" aria-label="Usage views">
        {(['insights', 'calls', 'threads', 'diagnostics'] as const).map((tab) => (
          <button key={tab} type="button" className={usageTab === tab ? 'active' : ''} onClick={() => setUsageTab(tab)}>
            {tab === 'insights' ? 'Insights' : tab === 'calls' ? 'Calls' : tab === 'threads' ? 'Threads' : 'Diagnostics'}
          </button>
        ))}
      </div>

      {usageTab === 'insights' ? (
        <div className="usageDashboardGrid">
          <section className="usagePanel">
            <h3>Needs attention</h3>
            <div className="usageTableWrap">
              <table className="usageTable">
                <thead>
                  <tr>
                    <th className="usageColThread">Thread</th>
                    <th>Calls</th>
                    <th>Total</th>
                    <th>Cache</th>
                    <th>Cost</th>
                  </tr>
                </thead>
                <tbody>
                  {sortThreads(summary?.topThreads ?? [], 'attention', 'desc')
                    .slice(0, 3)
                    .map((thread) => (
                      <tr key={thread.threadKey}>
                        <td className="usageColThread" title={thread.threadLabel}>{thread.threadLabel}</td>
                        <td>{thread.callCount}</td>
                        <td>{formatCompactNumber(thread.totalTokens)}</td>
                        <td>{renderCacheRatio(thread.cacheRatio)}</td>
                        <td>{formatCost(thread.estimatedCostUsd)}</td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
            <div className="usagePresetList">
              {(summary?.investigationPresets ?? []).map((preset) => (
                <button key={preset.id} type="button">
                  <strong>{preset.label}</strong>
                  <span>{preset.description}</span>
                </button>
              ))}
            </div>
            {lowCacheThreads(summary?.topThreads ?? []).length ? (
              <p className="usageNote">Low cache: {lowCacheThreads(summary?.topThreads ?? [])[0].threadLabel}</p>
            ) : (
              <p className="usageEmpty">No low-cache high-token threads.</p>
            )}
          </section>
          <section className="usagePanel">
            <h3>Diagnostics brief</h3>
            <p className="usageNote">{summarizeUsageDiagnostics(diagnostics)}</p>
          </section>
        </div>
      ) : null}

      {usageTab === 'calls' ? (
        <div className="usageDashboardGrid usageDashboardGrid--wide">
          <div className="usageTableWrap">
            <table className="usageTable">
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
                    return <th key={label} className={className}>{label}</th>;
                  })}
                </tr>
              </thead>
              <tbody>
                {calls.map((call) => (
                  <tr key={call.recordId} onClick={() => setSelectedRecordId(call.recordId)}>
                    <td className="usageColTime">{formatTimestamp(call.eventTimestamp)}</td>
                    <td className="usageColThread" title={threadName(call)}>{threadName(call)}</td>
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
                <dd>{selectedCall.sourceFile}:{selectedCall.lineNumber}</dd>
                <dt>Thread call</dt>
                <dd>{selectedCall.threadCallIndex ?? 0}</dd>
                <dt>Context</dt>
                <dd>{formatPercent(selectedCall.contextWindowPercent ?? 0)}</dd>
                <dt>Pricing</dt>
                <dd>{selectedCall.pricingModel ?? 'unknown'} · {selectedCall.pricingConfidence ?? 'unknown'}</dd>
              </dl>
            ) : (
              <p className="usageEmpty">No calls loaded.</p>
            )}
          </aside>
        </div>
      ) : null}

      {usageTab === 'threads' ? (
        <div className="usageTableWrap">
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
              {threads.map((thread) => (
                <tr key={thread.threadKey}>
                  <td className="usageColThread" title={thread.threadLabel}>{thread.threadLabel}</td>
                  <td>{thread.callCount}</td>
                  <td>{thread.sessionCount ?? 1}</td>
                  <td>{formatCompactNumber(thread.totalTokens)}</td>
                  <td>{renderCacheRatio(thread.cacheRatio)}</td>
                  <td className="usageColTime">{formatTimestamp(thread.latestEventTimestamp)}</td>
                  <td>{thread.primaryRecommendation ?? 'None'}</td>
                  <td>{formatCost(thread.estimatedCostUsd)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}

      {usageTab === 'diagnostics' ? (
        <div className="usageDashboardGrid">
          <section className="usagePanel">
            <h3>Parser diagnostics</h3>
            <p className="usageNote">{summarizeUsageDiagnostics(diagnostics)}</p>
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
                  {(summary?.pricingCoverage.unknownModels ?? []).map((value) => (
                    <tr key={value}>
                      <td>unknown_model</td>
                      <td>{value}</td>
                    </tr>
                  ))}
                  <tr>
                    <td>credit_coverage</td>
                    <td>{formatPercent(summary?.pricingCoverage.pricedTokenRatio ?? 0)}</td>
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
              <button className="usageModalClose" onClick={() => setActiveDetailCall(null)}>&times;</button>
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
                    <dd className="mono">{activeDetailCall.sourceFile}:{activeDetailCall.lineNumber}</dd>
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
                    <dd>{activeDetailCall.pricingModel ?? 'unknown'} ({activeDetailCall.pricingConfidence ?? 'unknown'})</dd>
                    <dt>Estimated Cost</dt>
                    <dd className="cost-val">{formatCost(activeDetailCall.estimatedCostUsd)}</dd>
                    <dt>Context Window</dt>
                    <dd>{activeDetailCall.modelContextWindow ? `${formatNumber(activeDetailCall.modelContextWindow)} tokens` : '-'}</dd>
                    <dt>Context Window %</dt>
                    <dd>{activeDetailCall.contextWindowPercent ? formatPercent(activeDetailCall.contextWindowPercent) : '-'}</dd>
                  </dl>
                </div>

                <div className="usageModalSection">
                  <h4>Tokens</h4>
                  <dl>
                    <dt>Total Tokens</dt>
                    <dd><strong>{formatNumber(activeDetailCall.totalTokens)}</strong></dd>
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
                      <div className="usageContentEmpty">No request content found in this call window.</div>
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
                      <div className="usageContentEmpty">No assistant output found in this call window.</div>
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
                      <div className="usageContentEmpty">No tool output found in this call window.</div>
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
