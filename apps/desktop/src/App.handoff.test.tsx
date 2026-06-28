import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
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
  listProviders: vi.fn(),
  listSessions: vi.fn(),
  getProfileQuota: vi.fn(),
  listCachedQuotas: vi.fn(),
  getUsageSummary: vi.fn(),
  getUsageDashboard: vi.fn(),
  refreshUsageIndex: vi.fn(),
  resetUsageIndex: vi.fn(),
  compactUsageDb: vi.fn(),
  takePendingRoute: vi.fn(),
  syncTrayQuota: vi.fn(),
  relayResumeSession: vi.fn(),
  openTerminalWithCommand: vi.fn(),
  openTerminalForLogin: vi.fn(),
  buildLoginCommand: vi.fn(),
  switchToPatAccount: vi.fn(),
  exportCpaCredentials: vi.fn(),
  updatePatSessionAuth: vi.fn(),
  addPatAccount: vi.fn(),
  setAuthMode: vi.fn(),
  getAuthMode: vi.fn(() => Promise.resolve('oauth')),
  getHideDockIcon: vi.fn(() => Promise.resolve(false)),
  setHideDockIcon: vi.fn(),
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
    _timerId: null,
    _intervalId: null,
  });
  useUsageStore.setState({
    summary: null,
    refreshing: false,
  });
  useProviderStore.setState({ providers: [] });
  vi.mocked(api.listCachedAccounts).mockResolvedValue([]);
  vi.mocked(api.healthCheck).mockResolvedValue({
    ok: true,
    version: 'test',
    homeRoot: '/tmp',
  });
  vi.mocked(api.listAccounts).mockResolvedValue(accounts);
  vi.mocked(api.listProviders).mockResolvedValue([]);
  vi.mocked(api.listCachedQuotas).mockResolvedValue([]);
  vi.mocked(api.getUsageSummary).mockResolvedValue(usageSummary);
  vi.mocked(api.getUsageDashboard).mockResolvedValue(usageSummary);
  vi.mocked(api.refreshUsageIndex).mockResolvedValue({
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

describe('App handoff modal', () => {
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
        within(accountCard!).getByRole('button', { name: /switch to this account/i }),
      ).toHaveProperty('disabled', true),
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
    const authFile = new File([JSON.stringify({ tokens: { account_id: 'account-test-6789' } })], 'auth.json', {
      type: 'application/json',
    });
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

    fireEvent.submit(screen.getByRole('button', { name: /upload/i }).closest('form')!);

    await waitFor(() => expect(api.addPatAccount).toHaveBeenCalled());
    await waitFor(() => expect(refreshAccountQuota).toHaveBeenCalledWith('codex-nova'));
  });

  it('creates a PAT token account from pasted ChatGPT session JSON', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /new account/i }));
    fireEvent.click(screen.getByRole('button', { name: /pat/i }));
    const patInput = screen.getByPlaceholderText(/enter token/i);
    fireEvent.change(patInput, { target: { value: 'pat-new' } });
    await screen.findByText('Paste the JSON returned by https://chatgpt.com/api/auth/session');
    const sessionButton = await screen.findByRole('button', { name: /paste session/i });
    fireEvent.click(sessionButton);
    fireEvent.change(screen.getByLabelText(/paste session/i), {
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

    fireEvent.click(within(accountCard!).getByRole('button', { name: /export cpa/i }));

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

  it('uses login switching for every account in Auth mode', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const accountCard = (await screen.findByText('codex-c')).closest('article');
    expect(accountCard).not.toBeNull();

    fireEvent.click(within(accountCard!).getByRole('button', { name: /switch to this account/i }));

    await waitFor(() => expect(api.openTerminalForLogin).toHaveBeenCalledWith('codex-c'));
    expect(api.switchToPatAccount).not.toHaveBeenCalled();
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
      target: { value: '{"accessToken":"at-new","idToken":"id-new","user":{"email":"u@example.com"}}' },
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

  it('shows Usage beside Overview and renders full-page dashboard in PAT mode', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await waitFor(() => expect(screen.getByLabelText(/pat mode/i)).toHaveProperty('checked', true));
    const nav = screen.getByRole('navigation', { name: /primary/i });
    const labels = within(nav).getAllByRole('button').map((button) => button.textContent);
    expect(labels.slice(0, 2)).toEqual(['Overview', 'Usage']);
    expect(screen.queryByRole('button', { name: /^stats$/i })).toBeNull();

    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));

    expect(await screen.findByText('Visible Calls')).not.toBeNull();
    expect(await screen.findByText('Estimated Cost')).not.toBeNull();
    expect(await screen.findByText('Codex Credits')).not.toBeNull();
    expect(screen.queryByText('Usage observed')).toBeNull();
    expect(document.querySelector('.modal')).toBeNull();
    expect(api.getUsageDashboard).toHaveBeenCalled();
  });

  it('opens the Usage page from a pending tray Stats route', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.takePendingRoute).mockResolvedValue('usage');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);

    expect(await screen.findByText('Visible Calls')).not.toBeNull();
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

    expect(await screen.findByText('Visible Calls')).not.toBeNull();
  });

  it('renders a Usage empty state in OAuth mode', async () => {
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    await screen.findByText('codex-c');
    fireEvent.click(screen.getByRole('button', { name: /usage/i }));
    expect(await screen.findByText(/available in PAT mode/i)).not.toBeNull();
  });

  it('refreshes the full-page usage dashboard manually', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const nav = await screen.findByRole('navigation', { name: /primary/i });
    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));

    expect(await screen.findByText('workspace/LAM')).not.toBeNull();
    expect((await screen.findAllByText('$1.23')).length).toBeGreaterThan(0);
    expect(await screen.findByText(/unknown_event_msg/i)).not.toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /^refresh$/i }));
    await waitFor(() => expect(api.refreshUsageIndex).toHaveBeenCalledWith(false));
    expect(api.getUsageDashboard).toHaveBeenCalled();
  });

  it('reloads all-history usage and refreshes with the same archived flag', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const nav = await screen.findByRole('navigation', { name: /primary/i });
    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));
    fireEvent.click(await screen.findByLabelText(/all history/i));

    await waitFor(() =>
      expect(api.getUsageDashboard).toHaveBeenCalledWith(
        expect.objectContaining({ includeArchived: true }),
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: /^refresh$/i }));
    await waitFor(() => expect(api.refreshUsageIndex).toHaveBeenCalledWith(true));
  });

  it('wires usage time windows into summary requests', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);
    const nav = await screen.findByRole('navigation', { name: /primary/i });
    fireEvent.click(within(nav).getByRole('button', { name: /usage/i }));
    const loadLimit = await screen.findByLabelText(/load limit/i);
    expect(Array.from((loadLimit as HTMLSelectElement).options).map((option) => option.textContent)).toEqual([
      '5,000 calls',
      '10,000 calls',
      '20,000 calls',
      'All calls',
    ]);

    const preset = await screen.findByLabelText(/time preset/i);
    expect(Array.from((preset as HTMLSelectElement).options).map((option) => option.textContent)).toEqual([
      'All time',
      'Today',
      'This week',
      'Last 7 days',
      'This month',
      'Custom range',
    ]);

    const sort = await screen.findByLabelText(/^sort$/i);
    expect(Array.from((sort as HTMLSelectElement).options).map((option) => option.textContent)).toEqual([
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
      expect(api.getUsageDashboard).toHaveBeenCalledWith(
        expect.objectContaining({
          window: expect.objectContaining({
            preset: 'custom',
            from: '2026-06-01',
            to: '2026-06-28',
          }),
        }),
      ),
    );
    expect(api.getUsageDashboard).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'today' }) }),
    );
    expect(api.getUsageDashboard).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'this-week' }) }),
    );
    expect(api.getUsageDashboard).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'last-7-days' }) }),
    );
    expect(api.getUsageDashboard).toHaveBeenCalledWith(
      expect.objectContaining({ window: expect.objectContaining({ preset: 'this-month' }) }),
    );
  });

  it('resets usage statistics from settings and reloads summary', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);
    useAppStore.setState({ route: 'settings' });

    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: /reset usage statistics/i }));

    await waitFor(() => expect(api.resetUsageIndex).toHaveBeenCalled());
    await waitFor(() => expect(api.getUsageDashboard).toHaveBeenCalled());
  });

  it('loads PAT usage without creating a React-owned usage interval', async () => {
    vi.mocked(api.getAuthMode).mockResolvedValue('pat');
    vi.mocked(api.listSessions).mockResolvedValue([]);

    render(<App />);

    await waitFor(() => expect(api.getUsageDashboard).toHaveBeenCalled());
    expect((useUsageStore.getState() as unknown as { _intervalId?: number | null })._intervalId).toBeUndefined();
  });
});
