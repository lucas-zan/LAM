import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import * as api from './lib/api';
import { useAccountStore } from './stores/accounts';
import { useAppStore } from './stores/app';
import { useProviderStore } from './stores/providers';
import { useQuotaStore } from './stores/quota';
import { useUsageStore } from './stores/usage';
import { useSessionStore } from './stores/sessions';
import type { CodexAccount, CodexSession, UsageDashboard } from './lib/types';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(vi.fn())),
}));

vi.mock('./lib/api', () => ({
  inTauri: vi.fn(() => true),
  listCachedAccounts: vi.fn(),
  healthCheck: vi.fn(),
  listAccounts: vi.fn(),
  listProvidersV2: vi.fn(),
  listProfileProviderBindingsV2: vi.fn(),
  listSessions: vi.fn(),
  getProfileQuota: vi.fn(),
  resetProfileQuota: vi.fn(),
  listCachedQuotas: vi.fn(),
  getUsageSummary: vi.fn(),
  getUsageDashboard: vi.fn(),
  getUsageDashboardResponse: vi.fn(),
  getUsageScopes: vi.fn(),
  getUsageOverview: vi.fn(),
  getUsageActivity: vi.fn(),
  getUsageInsights: vi.fn(),
  getUsageCalls: vi.fn(),
  getUsageThreads: vi.fn(),
  getUsageDiagnostics: vi.fn(),
  getUsageRateCard: vi.fn(),
  tryRefreshUsageIndex: vi.fn(),
  refreshUsageIndex: vi.fn(),
  resetUsageIndex: vi.fn(),
  compactUsageDb: vi.fn(),
  getCallRawContents: vi.fn(),
  takePendingRoute: vi.fn(),
  syncTrayQuota: vi.fn(),
  relayResumeSession: vi.fn(),
  openTerminalWithCommand: vi.fn(),
  openTerminalForLogin: vi.fn(),
  buildLoginCommand: vi.fn(),
  switchToPatAccount: vi.fn(),
  restartChatgpt: vi.fn(),
  exportCpaCredentials: vi.fn(),
  updatePatSessionAuth: vi.fn(),
  addPatAccount: vi.fn(),
  addSessionProfileAccount: vi.fn(),
  deleteAccount: vi.fn(),
  setAuthMode: vi.fn(),
  getAuthMode: vi.fn(() => Promise.resolve('oauth')),
  getHideDockIcon: vi.fn(() => Promise.resolve(false)),
  setHideDockIcon: vi.fn(),
  listTerminalTargets: vi.fn(() =>
    Promise.resolve([
      { id: 'terminal', displayName: 'Terminal.app', kind: 'terminal', installed: true },
      { id: 'ghostty', displayName: 'Ghostty', kind: 'terminal', installed: true },
    ]),
  ),
  getSelectedTerminalTarget: vi.fn(() => Promise.resolve('terminal')),
  setSelectedTerminalTarget: vi.fn(),
  getAntigravityQuota: vi.fn(() => Promise.resolve({ ok: true, models: [] })),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const accounts: CodexAccount[] = [
  {
    id: 'main',
    displayName: 'main',
    codexHome: '/tmp/.codex',
    wrapperPath: null,
    hasAuth: true,
    hasConfig: true,
    hasHistory: false,
    sessionCount: 1,
    latestSessionModifiedAt: 10,
    managed: false,
    isRelay: false,
    relaySource: null,
    relayIdentity: null,
    providerId: 'openai',
    model: 'gpt-5',
    authMode: 'config',
  },
  {
    id: 'codex-luna002',
    displayName: 'codex-luna002',
    codexHome: '/tmp/.codex-luna002',
    wrapperPath: null,
    hasAuth: true,
    hasConfig: true,
    hasHistory: false,
    sessionCount: 1,
    latestSessionModifiedAt: 30,
    managed: false,
    isRelay: false,
    relaySource: null,
    relayIdentity: null,
    providerId: 'openai',
    model: 'gpt-5',
    authMode: 'config',
  },
  {
    id: 'codex-c',
    displayName: 'codex-c',
    codexHome: '/tmp/.codex-c',
    wrapperPath: null,
    hasAuth: true,
    hasConfig: true,
    hasHistory: false,
    sessionCount: 1,
    latestSessionModifiedAt: 20,
    managed: false,
    isRelay: false,
    relaySource: null,
    relayIdentity: null,
    providerId: 'openai',
    model: 'gpt-5',
    authMode: 'config',
  },
];

const usageSummary: UsageDashboard = {
  refreshedAt: '2026-06-28T06:22:00Z',
  scannedFiles: 18,
  parsedEvents: 96,
  skippedEvents: 2,
  totalCalls: 96,
  totalTokens: 12_400_000,
  inputTokens: 10_000_000,
  cachedInputTokens: 9_100_000,
  uncachedInputTokens: 900_000,
  outputTokens: 2_400_000,
  reasoningOutputTokens: 700_000,
  estimatedCostUsd: 1.23,
  pricingCoverage: {
    pricedTokens: 12_400_000,
    unpricedTokens: 0,
    pricedTokenRatio: 1,
    unknownModels: [],
  },
  diagnostics: {
    parserDiagnostics: { unknown_event_msg: 2 },
    skippedEvents: 2,
    unknownModels: [],
    lowCacheThreads: [],
    highContextCalls: [],
    lastRefreshError: null,
  },
  topThreads: [
    {
      threadKey: 'thread:LAM',
      threadLabel: 'workspace/LAM',
      callCount: 24,
      totalTokens: 4_800_000,
      inputTokens: 4_000_000,
      cachedInputTokens: 3_100_000,
      uncachedInputTokens: 900_000,
      outputTokens: 800_000,
      latestEventTimestamp: '2026-06-28T06:20:00Z',
      cacheRatio: 0.78,
      estimatedCostUsd: 0.44,
    },
  ],
  recentCalls: [
    {
      recordId: 'record-1',
      sessionId: 'session-1',
      threadName: 'workspace/LAM',
      eventTimestamp: '2026-06-28T06:20:00Z',
      sourceFile: '/tmp/session.jsonl',
      lineNumber: 10,
      cwd: '/repo/LAM',
      model: 'gpt-5',
      effort: 'medium',
      inputTokens: 100,
      cachedInputTokens: 70,
      uncachedInputTokens: 30,
      outputTokens: 20,
      reasoningOutputTokens: 5,
      totalTokens: 120,
      cumulativeTotalTokens: 120,
      cacheRatio: 0.7,
      isArchived: false,
      contextWindowPercent: null,
      estimatedCostUsd: 0.01,
    },
  ],
  modelOptions: ['gpt-5'],
  effortOptions: ['medium'],
  pricingConfidenceOptions: ['priced'],
  statusChips: [
    { label: 'Pricing source', value: 'local rate card' },
    { label: 'Privacy mode', value: 'aggregate only' },
    { label: 'Parser diagnostics', value: '2' },
  ],
  investigationPresets: [
    { id: 'low-cache', label: 'Low cache reuse', description: 'Threads with large uncached input' },
  ],
};

function session(accountId: string, id: string, modifiedAt: number): CodexSession {
  return {
    id,
    accountId,
    path: `/tmp/${accountId}/${id}.jsonl`,
    modifiedAt,
    sizeBytes: 1,
    cwd: `/repo/${accountId}`,
    threadName: `${id} thread name`,
    summary: null,
    model: 'gpt-5',
    currentProviderId: 'openai',
    currentModel: 'gpt-5',
    providerMismatch: false,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  // Re-install localStorage mock destroyed by unstubAllGlobals (originally set in vitest.setup.ts)
  const store: Record<string, string> = {};
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store[k] ?? null,
    setItem: (k: string, v: string) => { store[k] = v; },
    removeItem: (k: string) => { delete store[k]; },
    clear: () => { for (const k in store) delete store[k]; },
    get length() { return Object.keys(store).length; },
    key: (i: number) => Object.keys(store)[i] ?? null,
  });
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: vi.fn(() => ({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  });
  useAppStore.setState({
    route: 'overview',
    status: 'Ready',
    error: '',
    appReady: false,
    modal: null,
  });
  useAccountStore.setState({
    accounts: [],
    selectedAccountId: '',
    activeSession: undefined,
    divergedStrategy: 'summarize_fork_with_target_account',
    refreshing: false,
  });
  useSessionStore.setState({
    sessions: [],
    selectedSessionId: '',
    query: '',
    resume: null,
  });
  useQuotaStore.setState({
    quotas: [],
    refreshingQuotaIds: [],
    resettingQuotaIds: [],
    _timerId: null,
    _intervalId: null,
  });
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
  useProviderStore.setState({ providers: [] });
  vi.mocked(api.listCachedAccounts).mockResolvedValue([]);
  vi.mocked(api.healthCheck).mockResolvedValue({
    ok: true,
    version: 'test',
    homeRoot: '/tmp',
  });
  vi.mocked(api.listAccounts).mockResolvedValue(accounts);
  vi.mocked(api.listProvidersV2).mockResolvedValue([]);
  vi.mocked(api.listProfileProviderBindingsV2).mockResolvedValue([]);
  vi.mocked(api.listCachedQuotas).mockResolvedValue([]);
  vi.mocked(api.getUsageSummary).mockResolvedValue(usageSummary);
  vi.mocked(api.getUsageDashboard).mockResolvedValue(usageSummary);
  vi.mocked(api.getUsageScopes).mockResolvedValue({
    scopes: [
      { id: 'total', label: 'Total', kind: 'total', isDefault: true },
      { id: 'workspace:main', label: 'main', kind: 'workspace', isDefault: false },
      {
        id: 'workspace:codex-c',
        label: 'codex-c',
        kind: 'workspace',
        accountId: 'codex-c',
        isDefault: false,
      },
    ],
    activeScopeId: 'total',
  });
  vi.mocked(api.getUsageOverview).mockResolvedValue(usageSummary);
  vi.mocked(api.getUsageActivity).mockResolvedValue(usageSummary.activityBuckets ?? []);
  vi.mocked(api.getUsageInsights).mockResolvedValue({
    fastModePercent: 0,
    mostUsedReasoning: 'Medium',
    mostUsedReasoningPercent: 1,
    skillsExplored: 0,
    totalSkillsUsed: 0,
    totalThreads: usageSummary.topThreads.length,
  });
  vi.mocked(api.getUsageCalls).mockResolvedValue({
    rows: usageSummary.recentCalls,
    total: usageSummary.recentCalls.length,
    limit: usageSummary.recentCalls.length,
    offset: 0,
    nextOffset: null,
  });
  vi.mocked(api.getUsageThreads).mockResolvedValue({
    rows: usageSummary.topThreads,
    total: usageSummary.topThreads.length,
    limit: usageSummary.topThreads.length,
    offset: 0,
    nextOffset: null,
  });
  vi.mocked(api.getUsageDiagnostics).mockResolvedValue(usageSummary.diagnostics);
  vi.mocked(api.getUsageRateCard).mockResolvedValue([]);
  vi.mocked(api.getUsageDashboardResponse).mockResolvedValue({
    scopes: [
      { id: 'total', label: 'Total', kind: 'total', isDefault: true },
      { id: 'workspace:main', label: 'main', kind: 'workspace', isDefault: false },
      {
        id: 'workspace:codex-c',
        label: 'codex-c',
        kind: 'workspace',
        accountId: 'codex-c',
        isDefault: false,
      },
    ],
    activeScopeId: 'total',
    dashboard: usageSummary,
  });
  vi.mocked(api.refreshUsageIndex).mockResolvedValue({
    scannedFiles: 18,
    parsedFiles: 1,
    parsedEvents: 1,
    insertedOrUpdatedEvents: 1,
    skippedEvents: 0,
    dbPath: '/tmp/.codex/lam/usage/usage.sqlite3',
    parserDiagnostics: {},
  });
  vi.mocked(api.tryRefreshUsageIndex).mockResolvedValue({
    scannedFiles: 18,
    parsedFiles: 1,
    parsedEvents: 1,
    insertedOrUpdatedEvents: 1,
    skippedEvents: 0,
    dbPath: '/tmp/.codex/lam/usage/usage.sqlite3',
    parserDiagnostics: {},
  });
  vi.mocked(api.resetUsageIndex).mockResolvedValue();
  vi.mocked(api.compactUsageDb).mockResolvedValue();
  vi.mocked(api.getCallRawContents).mockResolvedValue({
    request: '',
    assistant: '',
    toolOutput: '',
  });
  vi.mocked(api.takePendingRoute).mockResolvedValue(null);
  vi.mocked(api.getProfileQuota).mockResolvedValue({
    profileId: 'main',
    source: 'usage_unavailable',
    fetchedAt: 1,
    staleness: 'unavailable',
    planType: null,
    activityTokens: null,
    primaryUsedPercent: null,
    secondaryUsedPercent: null,
    remainingPercent: null,
    resetAt: null,
    secondaryResetAt: null,
    alerts: [],
    suggestedActions: [],
  });
  vi.mocked(api.resetProfileQuota).mockResolvedValue({
    snapshot: {
      profileId: 'main',
      source: 'app_server_rate_limits',
      fetchedAt: 2,
      staleness: 'fresh',
      planType: 'team',
      activityTokens: null,
      primaryUsedPercent: 3,
      secondaryUsedPercent: null,
      remainingPercent: 97,
      resetAt: null,
      secondaryResetAt: null,
      resetCreditCount: 0,
      alerts: [],
      suggestedActions: [],
    },
    outcome: 'reset',
    operationId: 'op-1',
  });
  vi.mocked(api.relayResumeSession).mockResolvedValue({
    action: 'copied',
    fromProfileId: 'main',
    toProfileId: 'codex-c',
    sessionId: 'main-session',
    sourcePath: '/tmp/main/main-session.jsonl',
    targetPath: '/tmp/codex-c/main-session.jsonl',
    resume: { command: 'codex resume main-session', sideEffects: [] },
    warnings: [],
  });
  vi.mocked(api.openTerminalWithCommand).mockResolvedValue();
  vi.mocked(api.openTerminalForLogin).mockResolvedValue();
  vi.mocked(api.switchToPatAccount).mockResolvedValue();
  vi.mocked(api.restartChatgpt).mockResolvedValue();
  vi.mocked(api.updatePatSessionAuth).mockResolvedValue();
  vi.mocked(api.exportCpaCredentials).mockResolvedValue({
    fileName: 'codex-c-cpa.json',
    content: { access_token: 'at-test' },
  });
  vi.mocked(api.addPatAccount).mockResolvedValue({
    accountId: 'codex-nova',
    email: 'nova@example.com',
    expired: '2030-12-31T23:59:59Z',
  });
  vi.mocked(api.addSessionProfileAccount).mockResolvedValue({
    profileId: 'nova',
    homePath: '/tmp/.codex-nova',
    wrapperPath: '/tmp/bin/codex-nova',
    operations: [],
    warnings: [],
  });
  vi.mocked(api.deleteAccount).mockResolvedValue({
    profileId: 'codex-c',
    removedHomePath: '/tmp/.codex-c',
    removedWrapperPath: '/tmp/bin/codex-c',
  });
  vi.mocked(api.setAuthMode).mockResolvedValue();
  vi.mocked(api.getAuthMode).mockResolvedValue('oauth');
  Object.defineProperty(URL, 'createObjectURL', {
    configurable: true,
    value: vi.fn(() => 'blob:lam-cpa'),
  });
  Object.defineProperty(URL, 'revokeObjectURL', {
    configurable: true,
    value: vi.fn(),
  });
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
});

afterEach(() => {
  vi.useRealTimers();
});

describe('App handoff modal', () => {
  it('does not start overlapping Antigravity auto-refresh requests', async () => {
    const pending = deferred<{ ok: boolean; models: never[] }>();
    vi.mocked(api.getAntigravityQuota).mockReturnValue(pending.promise);
    vi.mocked(api.listSessions).mockResolvedValue([]);
    const intervals: Array<{ handler: TimerHandler; timeout?: number }> = [];
    const setIntervalSpy = vi
      .spyOn(window, 'setInterval')
      .mockImplementation((handler, timeout) => {
        intervals.push({ handler, timeout });
        return intervals.length as unknown as ReturnType<typeof window.setInterval>;
      });

    render(<App />);

    await waitFor(() => expect(api.getAntigravityQuota).toHaveBeenCalledTimes(1));

    await act(async () => {
      for (const interval of intervals.filter((item) => item.timeout === 2 * 60_000)) {
        if (typeof interval.handler === 'function') {
          interval.handler();
        }
      }
    });
    expect(api.getAntigravityQuota).toHaveBeenCalledTimes(1);

    pending.resolve({ ok: true, models: [] });
    await Promise.resolve();
    setIntervalSpy.mockRestore();
  });

  it('does not load Usage sections during normal app startup on Overview', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);

    await screen.findByText('main');
    await waitFor(() => expect(api.healthCheck).toHaveBeenCalled());
    expect(api.getUsageScopes).not.toHaveBeenCalled();
    expect(api.getUsageOverview).not.toHaveBeenCalled();
    expect(api.getUsageActivity).not.toHaveBeenCalled();
    expect(api.refreshUsageIndex).not.toHaveBeenCalled();
  });

  it('loads Usage first-screen sections only after navigating to Usage', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);

    await screen.findByText('main');
    expect(api.getUsageScopes).not.toHaveBeenCalled();

    fireEvent.click(
      within(screen.getByRole('navigation', { name: /primary/i })).getByRole('button', {
        name: /usage/i,
      }),
    );

    await waitFor(() => expect(api.getUsageScopes).toHaveBeenCalledTimes(1));
    expect(api.getUsageOverview).toHaveBeenCalledTimes(1);
    expect(api.getUsageActivity).toHaveBeenCalledTimes(1);
    expect(api.refreshUsageIndex).not.toHaveBeenCalled();
  });

  it('opens the complete External API account flow from Providers', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);
    render(<App />);
    await screen.findByText('main');

    fireEvent.click(
      within(screen.getByRole('navigation', { name: /primary/i })).getByRole('button', {
        name: /providers/i,
      }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Add External API' }));

    const externalApiHeading = screen.getByRole('heading', { name: 'Add External API' });
    expect(externalApiHeading).toBeTruthy();
    expect(externalApiHeading.closest('section')?.classList.contains('modalWide')).toBe(true);
    expect(screen.queryByRole('button', { name: 'Profile Account' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'PAT Account' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'API Account' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Review API Account' })).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Add Provider' })).toBeNull();
  });

  it('opens the complete External API account flow from the global header', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);
    render(<App />);
    await screen.findByText('main');

    expect(screen.queryByRole('button', { name: 'New Provider' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'External API' }));

    expect(screen.getByRole('heading', { name: 'Add External API' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Profile Account' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'PAT Account' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'API Account' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Review API Account' })).toBeTruthy();
  });

  it('keeps API Account out of the New Account chooser', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);
    render(<App />);
    await screen.findByText('main');

    fireEvent.click(screen.getByRole('button', { name: 'New Account' }));

    expect(screen.getByRole('heading', { name: 'Add Account' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Profile Account' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'PAT Account' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'API Account' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Review API Account' })).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Add External API' })).toBeNull();
  });

  it('uses auth.json copy switching for every account in PAT mode', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    vi.mocked(api.listAccounts).mockImplementation(async () =>
      accounts.map((account) => ({
        ...account,
        isActiveAuth:
          account.id === 'codex-c' && vi.mocked(api.switchToPatAccount).mock.calls.length > 0,
      })),
    );

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /switch to this account/i }));

    await waitFor(() => expect(api.switchToPatAccount).toHaveBeenCalledWith('codex-c'));
    await waitFor(() =>
      expect(
        within(accountCard!).getByRole('button', { name: /reset codex-c quota/i }),
      ).toBeTruthy(),
    );
    expect(api.openTerminalForLogin).not.toHaveBeenCalled();
  });

  it('refreshes main quota after PAT switch copies auth into main', async () => {
    const refreshAccountQuota = vi.fn();
    useQuotaStore.setState({ refreshAccountQuota });
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /switch to this account/i }));

    await waitFor(() => expect(api.switchToPatAccount).toHaveBeenCalledWith('codex-c'));
    await waitFor(() => expect(refreshAccountQuota).toHaveBeenCalledWith('main'));
    await waitFor(() => expect(api.restartChatgpt).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.switchToPatAccount).mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(api.restartChatgpt).mock.invocationCallOrder[0],
    );
    expect(refreshAccountQuota.mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(api.restartChatgpt).mock.invocationCallOrder[0],
    );
  });

  it('refreshes the new PAT card after uploading auth.json', async () => {
    const refreshAccountQuota = vi.fn();
    useQuotaStore.setState({ refreshAccountQuota });
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /new account/i }));
    fireEvent.click(screen.getByRole('button', { name: /pat/i }));
    await screen.findByText(/upload auth\.json/i);
    const authFile = new File(
      [JSON.stringify({ tokens: { account_id: 'account-test-6789' } })],
      'auth.json',
      {
        type: 'application/json',
      },
    );
    Object.defineProperty(authFile, 'text', {
      value: () => Promise.resolve(JSON.stringify({ tokens: { account_id: 'account-test-6789' } })),
    });
    vi.stubGlobal(
      'FormData',
      class {
        get(name: string) {
          if (name === 'accountName') return 'nova';
          if (name === 'authFile') return authFile;
          if (name === 'personalAccessToken') return '';
          if (name === 'tokenExpiration') return '';
          return null;
        }
      },
    );

    fireEvent.submit(screen.getByRole('button', { name: /^upload$/i }).closest('form')!);

    await waitFor(() => expect(api.addPatAccount).toHaveBeenCalled());
    await waitFor(() => expect(refreshAccountQuota).toHaveBeenCalledWith('codex-nova'));
  });

  it('creates a full profile from pasted ChatGPT session JSON in Auth mode', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('oauth');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /new account/i }));
    fireEvent.click(screen.getByRole('button', { name: /import session/i }));

    fireEvent.change(screen.getByLabelText(/account name/i), {
      target: { value: 'nova' },
    });
    fireEvent.change(screen.getByLabelText(/session json/i), {
      target: {
        value:
          '{"accessToken":"at-new","idToken":"id-new","refreshToken":"rt-new","accountId":"account-new"}',
      },
    });
    fireEvent.click(screen.getByRole('button', { name: /import profile/i }));

    await waitFor(() =>
      expect(api.addSessionProfileAccount).toHaveBeenCalledWith({
        accountId: 'nova',
        sessionJson: {
          accessToken: 'at-new',
          idToken: 'id-new',
          refreshToken: 'rt-new',
          accountId: 'account-new',
        },
        overwriteWrapper: false,
      }),
    );
    expect(api.addPatAccount).not.toHaveBeenCalled();
  });

  it('creates a PAT token account from pasted ChatGPT session JSON', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /new account/i }));
    fireEvent.click(screen.getByRole('button', { name: /pat/i }));
    fireEvent.click(screen.getByRole('button', { name: /token & session/i }));
    const patInput = screen.getByPlaceholderText(/enter token/i);
    fireEvent.change(patInput, { target: { value: 'pat-new' } });
    fireEvent.change(screen.getByLabelText(/paste session json/i), {
      target: { value: '{"accessToken":"at-new","idToken":"id-new"}' },
    });
    vi.stubGlobal(
      'FormData',
      class {
        get(name: string) {
          if (name === 'accountName') return 'nova';
          if (name === 'personalAccessToken') return 'pat-new';
          if (name === 'tokenExpiration') return '';
          return null;
        }
      },
    );

    fireEvent.submit(screen.getByRole('button', { name: /save/i }).closest('form')!);

    await waitFor(() =>
      expect(api.addPatAccount).toHaveBeenCalledWith({
        accountId: 'nova',
        authJson: { accessToken: 'at-new', idToken: 'id-new' },
        personalAccessToken: 'pat-new',
        tokenExpiration: null,
      }),
    );
  });

  it('uses codex login for the Login button in PAT mode', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /^login$/i }));

    await waitFor(() => expect(api.openTerminalForLogin).toHaveBeenCalledWith('codex-c'));
    expect(api.switchToPatAccount).not.toHaveBeenCalled();
  });

  it('exports CPA auth from the PAT mode account action', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    useQuotaStore.setState({
      quotas: [
        {
          profileId: 'codex-c',
          source: 'app_server_rate_limits',
          fetchedAt: 1,
          staleness: 'fresh',
          planType: 'team',
          activityTokens: null,
          primaryUsedPercent: null,
          secondaryUsedPercent: null,
          remainingPercent: null,
          resetAt: null,
          secondaryResetAt: null,
          alerts: [],
          suggestedActions: [],
        },
      ],
    });
    vi.mocked(api.exportCpaCredentials).mockResolvedValue({
      fileName: 'codex-c-cpa.json',
      content: {
        access_token: 'at-test',
        id_token:
          'header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9wbGFuX3R5cGUiOiJlbnRlcnByaXNlIn19.sig',
      },
    });

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /more options/i }));
    fireEvent.click(screen.getByRole('button', { name: /export cpa/i }));

    await waitFor(() => expect(api.exportCpaCredentials).toHaveBeenCalledWith('codex-c'));
    const blob = vi.mocked(URL.createObjectURL).mock.calls[0][0] as Blob;
    const exported = JSON.parse(await blob.text());
    expect(exported).toMatchObject({
      access_token: 'at-test',
      type: 'codex',
      websockets: true,
      plan_type: 'enterprise',
      chatgpt_plan_type: 'enterprise',
    });
    expect(exported.id_token_synthetic).toBeUndefined();
  });

  it('shows reset expiry rows in Shanghai order and confirms before reset', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    vi.mocked(api.listAccounts).mockResolvedValue(
      accounts.map((account) =>
        account.id === 'codex-c' ? { ...account, isActiveAuth: true } : account,
      ),
    );
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    useQuotaStore.setState({
      quotas: [
        {
          profileId: 'codex-c',
          source: 'app_server_rate_limits',
          fetchedAt: 1,
          staleness: 'fresh',
          planType: 'team',
          activityTokens: null,
          primaryUsedPercent: 41,
          secondaryUsedPercent: null,
          remainingPercent: 59,
          resetAt: null,
          secondaryResetAt: null,
          resetCreditCount: 2,
          resetCreditDetails: [
            { id: 'later', expiresAt: '2026-07-12T00:00:00Z', source: 'api' },
            { id: 'soon', expiresAt: '2026-07-01T00:00:00Z', source: 'api' },
          ],
          alerts: [],
          suggestedActions: [],
        },
      ],
      resettingQuotaIds: [],
    });

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const accountCard = (await screen.findByRole('heading', { name: 'codex-c' })).closest(
      'article',
    );
    expect(accountCard).not.toBeNull();

    expect(within(accountCard!).queryByLabelText('Manual reset expiry')).toBeNull();
    expect(within(accountCard!).getByLabelText('Expires: 2026-07-01 08:00')).toBeTruthy();
    expect(within(accountCard!).getByLabelText('Expires: 2026-07-12 08:00')).toBeTruthy();
    expect(within(accountCard!).queryByRole('button', { name: 'Handoff' })).toBeNull();
    expect(
      within(accountCard!).getAllByRole('button', { name: /reset codex-c quota/i }),
    ).toHaveLength(1);

    fireEvent.click(within(accountCard!).getByRole('button', { name: /reset codex-c quota/i }));

    await waitFor(() => expect(confirmSpy).toHaveBeenCalled());
    await waitFor(() => expect(api.resetProfileQuota).toHaveBeenCalledWith('codex-c'));
  });

  it('uses the Login action for every account in Auth mode', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();
    expect(
      within(accountCard!).queryByRole('button', { name: /switch to this account/i }),
    ).toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /more options/i }));
    fireEvent.click(screen.getByRole('button', { name: /^login$/i }));

    await waitFor(() => expect(api.openTerminalForLogin).toHaveBeenCalledWith('codex-c'));
    expect(api.switchToPatAccount).not.toHaveBeenCalled();
  });

  it('deletes a profile account from the Auth mode account card after confirmation', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('oauth');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /more options/i }));
    fireEvent.click(screen.getByRole('button', { name: /delete codex-c/i }));

    expect(await screen.findByRole('heading', { name: /delete account/i })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /delete account/i }));
    await waitFor(() => expect(api.deleteAccount).toHaveBeenCalledWith({ profileId: 'codex-c' }));
  });

  it('does not delete a profile account when confirmation is cancelled', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('oauth');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /more options/i }));
    fireEvent.click(screen.getByRole('button', { name: /delete codex-c/i }));
    fireEvent.click(await screen.findByRole('button', { name: /cancel/i }));

    expect(api.deleteAccount).not.toHaveBeenCalled();
  });

  it('does not show account delete actions in PAT mode', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /more options/i }));
    expect(screen.queryByRole('button', { name: /delete codex-c/i })).toBeNull();
  });

  it('uses the Login action as session auth update for PAT token accounts', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    vi.mocked(api.listAccounts).mockResolvedValue(
      accounts.map((account) =>
        account.id === 'codex-c' ? { ...account, hasPersonalAccessToken: true } : account,
      ),
    );

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /^update$/i }));
    const textarea = await screen.findByLabelText(/session json/i);
    fireEvent.change(textarea, {
      target: {
        value: '{"accessToken":"at-new","idToken":"id-new","user":{"email":"u@example.com"}}',
      },
    });
    const updateButtons = screen.getAllByRole('button', { name: /^update$/i });
    fireEvent.click(updateButtons[updateButtons.length - 1]);

    await waitFor(() =>
      expect(api.updatePatSessionAuth).toHaveBeenCalledWith('codex-c', {
        accessToken: 'at-new',
        idToken: 'id-new',
        user: { email: 'u@example.com' },
      }),
    );
    expect(api.openTerminalForLogin).not.toHaveBeenCalled();
  });

  it('relays the selected source session and does not fall back to latest active session', async () => {
    const mainSessions = deferred<CodexSession[]>();
    let holdMainSessions = false;
    vi.mocked(api.listSessions).mockImplementation((accountId) => {
      if (accountId === 'main') {
        if (!holdMainSessions) return Promise.resolve([session('main', 'main-initial', 10)]);
        return mainSessions.promise;
      }
      if (accountId === 'codex-luna002') {
        return Promise.resolve([session('codex-luna002', 'luna-latest', 30)]);
      }
      if (accountId === 'codex-c') return Promise.resolve([session('codex-c', 'c-session', 20)]);
      return Promise.resolve([]);
    });

    render(<App />);
    await screen.findByText('codex-c');
    await waitFor(() => expect(useAccountStore.getState().activeSession?.id).toBe('luna-latest'));
    fireEvent.click(screen.getByTitle('Choose a session to continue with codex-c'));
    await waitFor(() =>
      expect(screen.getByLabelText('Source account')).toHaveProperty('value', 'main'),
    );

    fireEvent.change(screen.getByLabelText('Source account'), {
      target: { value: 'codex-luna002' },
    });
    await screen.findAllByText(/luna-latest thread name/);

    holdMainSessions = true;
    fireEvent.change(screen.getByLabelText('Source account'), { target: { value: 'main' } });
    expect(screen.getByRole('button', { name: 'Start Handoff' })).toHaveProperty('disabled', true);

    mainSessions.resolve([session('main', 'main-session', 10)]);
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Start Handoff' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Start Handoff' }));

    await waitFor(() =>
      expect(api.relayResumeSession).toHaveBeenCalledWith({
        fromProfileId: 'main',
        toProfileId: 'codex-c',
        sessionId: 'main-session',
        cwd: '/repo/main',
        divergedStrategy: 'summarize_fork_with_target_account',
      }),
    );
    expect(api.relayResumeSession).not.toHaveBeenCalledWith(
      expect.objectContaining({ fromProfileId: 'codex-luna002', sessionId: 'luna-latest' }),
    );
  });

  it('keeps the handoff modal open when the terminal launch fails', async () => {
    vi.mocked(api.listSessions).mockImplementation((accountId) => {
      if (accountId === 'main') return Promise.resolve([session('main', 'main-session', 10)]);
      if (accountId === 'codex-c') return Promise.resolve([session('codex-c', 'c-session', 20)]);
      return Promise.resolve([]);
    });
    vi.mocked(api.openTerminalWithCommand).mockRejectedValue(
      new Error('cmux did not accept the resume command: Broken pipe'),
    );

    render(<App />);
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: 'Handoff' }));
    await waitFor(() =>
      expect(screen.getByLabelText('Source account')).toHaveProperty('value', 'main'),
    );
    await screen.findAllByText(/main-session thread name/);

    fireEvent.click(screen.getByRole('button', { name: 'Start Handoff' }));

    await waitFor(() =>
      expect(screen.getByText(/cmux did not accept the resume command/i)).toBeTruthy(),
    );
    expect(screen.getByRole('heading', { name: 'Handoff Session' })).toBeTruthy();
    expect(api.listAccounts).toHaveBeenCalledTimes(1);
  });

  it('closes the handoff modal after a successful terminal launch', async () => {
    vi.mocked(api.listSessions).mockImplementation((accountId) => {
      if (accountId === 'main') return Promise.resolve([session('main', 'main-session', 10)]);
      if (accountId === 'codex-c') return Promise.resolve([session('codex-c', 'c-session', 20)]);
      return Promise.resolve([]);
    });

    render(<App />);
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: 'Handoff' }));
    await screen.findAllByText(/main-session thread name/);

    fireEvent.click(screen.getByRole('button', { name: 'Start Handoff' }));

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Handoff Session' })).toBeNull(),
    );
  });

  it('shows Usage beside Overview and renders full-page dashboard in PAT mode', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const nav = screen.getByRole('navigation', { name: /primary/i });
    const labels = within(nav)
      .getAllByRole('button')
      .map((button) => button.textContent);
    expect(labels.slice(0, 2)).toEqual(['Overview', 'Usage']);
    expect(screen.queryByRole('button', { name: /^stats$/i })).toBeNull();

    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));

    expect(await screen.findByText('Total Calls')).not.toBeNull();
    expect(await screen.findByText('Estimated Cost')).not.toBeNull();
    expect(await screen.findByText('Codex Credits')).not.toBeNull();
    expect(screen.queryByText('Usage observed')).toBeNull();
    expect(document.querySelector('.modal')).toBeNull();
    expect(api.getUsageOverview).toHaveBeenCalled();
  });

  it('opens the Usage page from a pending tray Stats route', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.takePendingRoute).mockResolvedValue('usage');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);

    expect(await screen.findByText('Total Calls')).not.toBeNull();
    expect(await screen.findByText('Estimated Cost')).not.toBeNull();
    expect(api.takePendingRoute).toHaveBeenCalled();
  });

  it('opens the Usage page from tray Stats when the existing window is focused', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.takePendingRoute).mockResolvedValueOnce(null).mockResolvedValueOnce('usage');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await screen.findByText('codex-c');
    window.dispatchEvent(new Event('focus'));

    expect(await screen.findByText('Total Calls')).not.toBeNull();
  });

  it('renders scoped Usage dashboard in OAuth mode', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await screen.findByText('codex-c');
    fireEvent.click(screen.getByRole('button', { name: /usage/i }));
    expect(await screen.findByText('Total Calls')).not.toBeNull();
    expect(await screen.findByRole('tab', { name: 'Total' })).not.toBeNull();
    fireEvent.click(screen.getByRole('tab', { name: 'codex-c' }));
    await waitFor(() =>
      expect(api.getUsageOverview).toHaveBeenCalledWith(
        expect.objectContaining({ scopeId: 'workspace:codex-c' }),
      ),
    );
  });

  it('selects a Usage scope tab immediately while scoped data is still loading', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);
    const pendingOverview = deferred<UsageDashboard>();
    vi.mocked(api.getUsageOverview)
      .mockResolvedValueOnce(usageSummary)
      .mockReturnValueOnce(pendingOverview.promise);

    render(<App />);
    await screen.findByText('codex-c');
    fireEvent.click(screen.getByRole('button', { name: /usage/i }));
    await screen.findByRole('tab', { name: 'codex-c' });

    fireEvent.click(screen.getByRole('tab', { name: 'codex-c' }));

    expect(screen.getByRole('tab', { name: 'codex-c' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('tab', { name: 'Total' }).getAttribute('aria-selected')).toBe('false');
    expect(api.getUsageOverview).toHaveBeenCalledWith(
      expect.objectContaining({ scopeId: 'workspace:codex-c' }),
    );

    pendingOverview.resolve(usageSummary);
  });

  it('indexes usage before refreshing the active usage dashboard section manually', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const nav = await screen.findByRole('navigation', { name: /primary/i });
    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));

    expect((await screen.findAllByText('$1.23')).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: /^refresh$/i }));
    await waitFor(() => expect(api.getUsageOverview).toHaveBeenCalled());
    expect(api.tryRefreshUsageIndex).toHaveBeenCalledWith(false);
  });

  it('reloads all-history usage and refreshes with the same archived flag', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const nav = await screen.findByRole('navigation', { name: /primary/i });
    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));
    const historySelect = await screen.findByLabelText(/^history$/i);
    fireEvent.change(historySelect, { target: { value: 'all' } });

    await waitFor(() =>
      expect(api.getUsageOverview).toHaveBeenCalledWith(
        expect.objectContaining({ includeArchived: true }),
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: /^refresh$/i }));
    await waitFor(() =>
      expect(api.getUsageOverview).toHaveBeenCalledWith(
        expect.objectContaining({ includeArchived: true }),
      ),
    );
  });

  it('wires usage time windows into summary requests', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const nav = await screen.findByRole('navigation', { name: /primary/i });
    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));
    const loadLimit = await screen.findByLabelText(/load limit/i);
    expect(
      Array.from((loadLimit as HTMLSelectElement).options).map((option) => option.textContent),
    ).toEqual(['5,000 calls', '10,000 calls', '20,000 calls', 'All calls']);

    const preset = await screen.findByLabelText(/time preset/i);
    expect(
      Array.from((preset as HTMLSelectElement).options).map((option) => option.textContent),
    ).toEqual(['All time', 'Today', 'This week', 'Last 7 days', 'This month', 'Custom range']);

    const sort = await screen.findByLabelText(/^sort$/i);
    expect(
      Array.from((sort as HTMLSelectElement).options).map((option) => option.textContent),
    ).toEqual([
      'Time',
      'Duration',
      'Gap',
      'Attention',
      'Thread',
      'Initiator',
      'Model',
      'Effort',
      'Total',
      'Cached',
      'Uncached',
      'Output',
      'Reasoning',
      'Cost',
      'Usage',
      'Cache',
      'Context',
    ]);

    fireEvent.change(preset, { target: { value: 'today' } });
    fireEvent.change(preset, { target: { value: 'this-week' } });
    fireEvent.change(preset, { target: { value: 'last-7-days' } });
    fireEvent.change(preset, { target: { value: 'this-month' } });
    fireEvent.change(preset, { target: { value: 'custom' } });
    fireEvent.change(screen.getByLabelText(/custom start/i), { target: { value: '2026-06-01' } });
    fireEvent.change(screen.getByLabelText(/custom end/i), { target: { value: '2026-06-28' } });

    await waitFor(() =>
      expect(api.getUsageOverview).toHaveBeenCalledWith(
        expect.objectContaining({
          window: expect.objectContaining({
            preset: 'custom',
            from: '2026-06-01',
            to: '2026-06-28',
          }),
        }),
      ),
    );
    expect(api.getUsageOverview).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'today' }) }),
    );
    expect(api.getUsageOverview).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'this-week' }) }),
    );
    expect(api.getUsageOverview).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'last-7-days' }) }),
    );
    expect(api.getUsageOverview).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'this-month' }) }),
    );
  });

  it('resets usage statistics from settings and reloads summary', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    useAppStore.setState({ route: 'settings' });

    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /advanced/i }));
    fireEvent.click(await screen.findByRole('button', { name: /reset usage statistics/i }));

    await waitFor(() => expect(api.resetUsageIndex).toHaveBeenCalled());
    await waitFor(() => expect(api.getUsageOverview).toHaveBeenCalled());
  });

  it('shows the backend usage rate card in settings', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    vi.mocked(api.getUsageRateCard).mockResolvedValue([
      {
        model: 'gpt-5',
        pricingModel: 'gpt-5',
        contextWindow: 'all',
        estimated: false,
        inputPerMillion: 4.375,
        cachedInputPerMillion: 0.4375,
        outputPerMillion: 35,
        notes: null,
      },
    ]);
    useAppStore.setState({ route: 'settings' });

    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /rate card/i }));

    expect(await screen.findByText('Usage Rate Card')).toBeTruthy();
    expect((await screen.findAllByText('gpt-5')).length).toBeGreaterThan(0);
    expect(await screen.findByText('$4.38')).toBeTruthy();
    expect(await screen.findByText('$35.00')).toBeTruthy();
    const rateCardTable = document.querySelector('.settingsRateCardTable');
    expect(rateCardTable?.closest('.rows')).toBeNull();
  });

  it('selects a terminal target from settings', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    useAppStore.setState({ route: 'settings' });

    render(<App />);

    const terminalSelect = (await screen.findByLabelText(/handoff terminal/i)) as HTMLSelectElement;
    expect(terminalSelect.value).toBe('terminal');

    fireEvent.change(terminalSelect, { target: { value: 'ghostty' } });

    await waitFor(() => expect(api.setSelectedTerminalTarget).toHaveBeenCalledWith('ghostty'));
  });

  it('uses Profile Only mode availability from settings', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    useAppStore.setState({ route: 'settings' });

    render(<App />);

    const availabilitySelect = (await screen.findByLabelText(
      /mode availability/i,
    )) as HTMLSelectElement;
    fireEvent.change(availabilitySelect, { target: { value: 'profile' } });

    expect(await screen.findByText('Profile Mode')).toBeTruthy();
    expect(screen.queryByRole('tab', { name: /^pat$/i })).toBeNull();
    await waitFor(() => expect(api.setAuthMode).toHaveBeenCalledWith('oauth'));

    fireEvent.click(screen.getByRole('button', { name: /new account/i }));
    expect(screen.queryByRole('button', { name: /pat account/i })).toBeNull();
    expect(await screen.findByRole('button', { name: /cli auth/i })).toBeTruthy();
  });

  it('uses PAT Only mode availability from settings', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('oauth');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    useAppStore.setState({ route: 'settings' });

    render(<App />);

    const availabilitySelect = (await screen.findByLabelText(
      /mode availability/i,
    )) as HTMLSelectElement;
    fireEvent.change(availabilitySelect, { target: { value: 'pat' } });

    expect(await screen.findByText('PAT Mode')).toBeTruthy();
    expect(screen.queryByRole('tab', { name: /^profile$/i })).toBeNull();
    await waitFor(() => expect(api.setAuthMode).toHaveBeenCalledWith('pat'));

    fireEvent.click(screen.getByRole('button', { name: /new account/i }));
    expect(screen.queryByRole('button', { name: /profile account/i })).toBeNull();
    expect(await screen.findByText(/upload auth\.json/i)).toBeTruthy();
  });

  it('keeps both titlebar tabs in Profile & PAT mode availability', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('oauth');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    useAppStore.setState({ route: 'settings' });

    render(<App />);

    const availabilitySelect = (await screen.findByLabelText(
      /mode availability/i,
    )) as HTMLSelectElement;
    expect(availabilitySelect.value).toBe('both');
    expect(screen.getByRole('tab', { name: /^profile$/i })).toBeTruthy();
    expect(screen.getByRole('tab', { name: /^pat$/i })).toBeTruthy();
  });

  it('loads PAT usage without creating a React-owned usage interval', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await screen.findByText('main');

    fireEvent.click(
      within(screen.getByRole('navigation', { name: /primary/i })).getByRole('button', {
        name: /usage/i,
      }),
    );

    await waitFor(() => expect(api.getUsageOverview).toHaveBeenCalled());
    expect(
      (useUsageStore.getState() as unknown as { _intervalId?: number | null })._intervalId,
    ).toBeUndefined();
  });
});
