import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAccountStore } from './accounts';
import { useAppStore } from './app';
import { useQuotaStore } from './quota';
import { useProviderStore } from './providers';
import * as api from '../lib/api';

vi.mock('../lib/api', () => ({
  inTauri: vi.fn(() => true),
  listCachedAccounts: vi.fn(),
  healthCheck: vi.fn(),
  listAccounts: vi.fn(),
  listProviders: vi.fn(),
  listSessions: vi.fn(),
  getProfileQuota: vi.fn(),
  listCachedQuotas: vi.fn(),
  syncTrayQuota: vi.fn(),
  relayResumeSession: vi.fn(),
  openTerminalWithCommand: vi.fn(),
  updateAccountNote: vi.fn(),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

beforeEach(() => {
  vi.clearAllMocks();
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
  useQuotaStore.setState({
    quotas: [],
    refreshingQuotaIds: [],
    _timerId: null,
    _intervalId: null,
  });
  useProviderStore.setState({ providers: [] });
  vi.mocked(api.listCachedAccounts).mockResolvedValue([]);
  vi.mocked(api.healthCheck).mockResolvedValue({
    ok: true,
    version: 'test',
    homeRoot: '/tmp',
  });
  vi.mocked(api.listProviders).mockResolvedValue([]);
  vi.mocked(api.listSessions).mockResolvedValue([]);
  vi.mocked(api.listCachedQuotas).mockResolvedValue([]);
  vi.mocked(api.relayResumeSession).mockResolvedValue({
    action: 'copied',
    fromProfileId: 'codex-luna002',
    toProfileId: 'codex-c',
    sessionId: 'latest-session',
    sourcePath: '/tmp/latest.jsonl',
    targetPath: '/tmp/codex-c/latest.jsonl',
    resume: { command: 'codex resume latest-session', sideEffects: [] },
    warnings: [],
  });
  vi.mocked(api.openTerminalWithCommand).mockResolvedValue();
  vi.mocked(api.updateAccountNote).mockResolvedValue({
    id: 'a',
    displayName: 'codex-a',
    codexHome: '/tmp/.codex-a',
    wrapperPath: null,
    hasAuth: true,
    hasConfig: true,
    hasHistory: false,
    sessionCount: 0,
    latestSessionModifiedAt: null,
    managed: false,
    isRelay: false,
    relaySource: null,
    relayIdentity: null,
    providerId: 'openai',
    model: 'gpt-5-codex',
    authMode: 'config',
    renewalDate: '2026-07-15',
    note: 'Team Plus renewal',
  });
  vi.mocked(api.getProfileQuota).mockResolvedValue({
    profileId: 'a',
    source: 'app_server_rate_limits',
    fetchedAt: 1,
    staleness: 'fresh',
    planType: 'plus',
    activityTokens: null,
    primaryUsedPercent: 40,
    secondaryUsedPercent: 20,
    remainingPercent: 60,
    resetAt: '2026-06-16T10:00:00Z',
    secondaryResetAt: '2026-06-17T10:00:00Z',
    alerts: [],
    suggestedActions: [],
  });
});

describe('useAccountStore', () => {
  it('tracks refresh state while the app refresh button is running', async () => {
    const accounts = deferred<Awaited<ReturnType<typeof api.listAccounts>>>();
    vi.mocked(api.listAccounts).mockReturnValue(accounts.promise);

    const refresh = useAccountStore.getState().refresh();
    expect(useAccountStore.getState().refreshing).toBe(true);

    accounts.resolve([]);
    await refresh;
    expect(useAccountStore.getState().refreshing).toBe(false);
  });

  it('manual refresh immediately refreshes quota for each account', async () => {
    vi.mocked(api.listAccounts).mockResolvedValue([
      {
        id: 'a',
        displayName: 'codex-a',
        codexHome: '/tmp/.codex-a',
        wrapperPath: null,
        hasAuth: true,
        hasConfig: true,
        hasHistory: false,
        sessionCount: 0,
        latestSessionModifiedAt: null,
        managed: false,
        isRelay: false,
        relaySource: null,
        relayIdentity: null,
        providerId: 'openai',
        model: 'gpt-5-codex',
        authMode: 'config',
        renewalDate: null,
        note: null,
      },
    ]);

    await useAccountStore.getState().refresh({ refreshQuotasNow: true });

    expect(api.getProfileQuota).toHaveBeenCalledWith('a', true);
    expect(useQuotaStore.getState().quotas).toHaveLength(1);
  });

  it('manual refresh does not let stale cached quotas overwrite fresh snapshots', async () => {
    const staleCache = deferred<Awaited<ReturnType<typeof api.listCachedQuotas>>>();
    vi.mocked(api.listAccounts).mockResolvedValue([
      {
        id: 'a',
        displayName: 'codex-a',
        codexHome: '/tmp/.codex-a',
        wrapperPath: null,
        hasAuth: true,
        hasConfig: true,
        hasHistory: false,
        sessionCount: 0,
        latestSessionModifiedAt: null,
        managed: false,
        isRelay: false,
        relaySource: null,
        relayIdentity: null,
        providerId: 'openai',
        model: 'gpt-5-codex',
        authMode: 'config',
        renewalDate: null,
        note: null,
      },
    ]);
    vi.mocked(api.listCachedQuotas).mockReturnValue(staleCache.promise);
    vi.mocked(api.getProfileQuota).mockResolvedValue({
      profileId: 'a',
      source: 'app_server_rate_limits',
      fetchedAt: 2,
      staleness: 'fresh',
      planType: 'plus',
      activityTokens: null,
      primaryUsedPercent: 20,
      secondaryUsedPercent: 10,
      remainingPercent: 80,
      resetAt: '2026-06-16T10:00:00Z',
      secondaryResetAt: '2026-06-17T10:00:00Z',
      alerts: [],
      suggestedActions: [],
    });

    await useAccountStore.getState().refresh({ refreshQuotasNow: true });
    staleCache.resolve([
      {
        profileId: 'a',
        source: 'app_server_rate_limits',
        fetchedAt: 1,
        staleness: 'cached',
        planType: 'plus',
        activityTokens: null,
        primaryUsedPercent: 60,
        secondaryUsedPercent: 40,
        remainingPercent: 40,
        resetAt: '2026-06-16T10:00:00Z',
        secondaryResetAt: '2026-06-17T10:00:00Z',
        alerts: [],
        suggestedActions: [],
      },
    ]);
    await Promise.resolve();

    expect(useQuotaStore.getState().quotas).toMatchObject([
      {
        profileId: 'a',
        fetchedAt: 2,
        staleness: 'fresh',
        primaryUsedPercent: 20,
      },
    ]);
  });

  it('relay latest uses the current active session', async () => {
    const target = {
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
      authMode: 'config' as const,
      renewalDate: null,
      note: null,
    };
    useAccountStore.setState({
      accounts: [target],
      activeSession: {
        id: 'latest-session',
        accountId: 'codex-luna002',
        path: '/tmp/latest.jsonl',
        modifiedAt: 30,
        sizeBytes: 1,
        cwd: '/repo/latest',
        summary: null,
        model: 'gpt-5',
        currentProviderId: 'openai',
        currentModel: 'gpt-5',
        providerMismatch: false,
      },
    });

    await useAccountStore.getState().relayResumeTo(target);

    expect(api.relayResumeSession).toHaveBeenCalledWith({
      fromProfileId: 'codex-luna002',
      toProfileId: 'codex-c',
      sessionId: 'latest-session',
      cwd: '/repo/latest',
      divergedStrategy: 'summarize_fork_with_target_account',
    });
  });

  it('saves account note metadata and updates the matching account', async () => {
    const account = {
      id: 'a',
      displayName: 'codex-a',
      codexHome: '/tmp/.codex-a',
      wrapperPath: null,
      hasAuth: true,
      hasConfig: true,
      hasHistory: false,
      sessionCount: 0,
      latestSessionModifiedAt: null,
      managed: false,
      isRelay: false,
      relaySource: null,
      relayIdentity: null,
      providerId: 'openai',
      model: 'gpt-5-codex',
      authMode: 'config' as const,
      renewalDate: null,
      note: null,
    };
    useAccountStore.setState({ accounts: [account], selectedAccountId: 'a' });

    await useAccountStore.getState().saveAccountNote({
      profileId: 'a',
      renewalDate: '2026-07-15',
      note: 'Team Plus renewal',
    });

    expect(api.updateAccountNote).toHaveBeenCalledWith({
      profileId: 'a',
      renewalDate: '2026-07-15',
      note: 'Team Plus renewal',
    });
    expect(useAccountStore.getState().accounts[0]).toMatchObject({
      id: 'a',
      renewalDate: '2026-07-15',
      note: 'Team Plus renewal',
    });
  });
});
