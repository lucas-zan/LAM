import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import * as api from './lib/api';
import { useAccountStore } from './stores/accounts';
import { useAppStore } from './stores/app';
import { useProviderStore } from './stores/providers';
import { useQuotaStore } from './stores/quota';
import { useSessionStore } from './stores/sessions';
import type { CodexAccount, CodexSession } from './lib/types';

vi.mock('./lib/api', () => ({
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
});
