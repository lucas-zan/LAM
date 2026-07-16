import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { routes } from './types';
import { Overview, Sessions } from './views';
import type { CodexAccount, CodexSession, UsageQuotaSnapshot } from '../lib/types';

const { tauriDialogConfirm } = vi.hoisted(() => ({
  tauriDialogConfirm: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  confirm: tauriDialogConfirm,
}));

vi.mock('../lib/api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/api')>();
  return {
    ...actual,
    inTauri: () => true,
  };
});

const accounts: CodexAccount[] = [
  {
    id: 'a',
    displayName: 'codex-a',
    codexHome: '/tmp/.codex-a',
    wrapperPath: null,
    hasAuth: true,
    hasConfig: true,
    hasHistory: false,
    sessionCount: 2,
    latestSessionModifiedAt: 20,
    managed: false,
    isRelay: false,
    relaySource: null,
    relayIdentity: null,
    providerId: 'openai',
    model: 'gpt-5',
    authMode: 'config',
    renewalDate: '2026-07-15',
    note: 'Team Plus renewal',
  },
  {
    id: 'b',
    displayName: 'codex-b',
    codexHome: '/tmp/.codex-b',
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
    renewalDate: null,
    note: null,
  },
];

const sessions: CodexSession[] = [
  {
    id: 'sid-a',
    accountId: 'a',
    path: '/tmp/.codex-a/sessions/sid-a.jsonl',
    modifiedAt: 20,
    sizeBytes: 100,
    cwd: '/repo/a',
    threadName: 'Implement relay session picker',
    summary: 'task a',
    model: 'gpt-5',
    currentProviderId: 'openai',
    currentModel: 'gpt-5',
    providerMismatch: false,
  },
];

const quotas: UsageQuotaSnapshot[] = [
  {
    profileId: 'a',
    source: 'app_server_rate_limits',
    fetchedAt: 1,
    staleness: 'fresh',
    planType: 'team',
    activityTokens: null,
    primaryUsedPercent: 20,
    primaryWindowDurationMins: 300,
    secondaryUsedPercent: 10,
    secondaryWindowDurationMins: 10080,
    remainingPercent: 80,
    resetAt: '1782109286',
    secondaryResetAt: '1782352982',
    alerts: [],
    suggestedActions: [],
  },
];

function overviewProps() {
  return {
    accounts,
    quotas,
    providers: [],
    select: vi.fn(),
    openSync: vi.fn(),
    rename: vi.fn(),
    deleteAccount: vi.fn(),
    login: vi.fn(),
    switchAccount: vi.fn(),
    exportCpa: vi.fn(),
    openHandoff: vi.fn(),
    relayLatest: vi.fn(),
    currentSession: sessions[0],
    refreshAccountQuota: vi.fn(),
    refreshingQuotaIds: [],
    antigravityQuota: null,
    refreshingAntigravity: false,
    onRefreshAntigravity: vi.fn(),
    onSaveAccountNote: vi.fn(),
    openUploadPat: vi.fn(),
  };
}

describe('handoff navigation and entry points', () => {
  beforeEach(() => {
    tauriDialogConfirm.mockReset();
  });

  it('does not expose Relay as a first-level route', () => {
    expect(routes.map((route) => route.id)).not.toContain('relay');
    expect(routes.map((route) => route.label)).not.toContain('Relay');
  });

  it('keeps both explicit Handoff and latest-session Relay shortcuts on account cards', () => {
    render(<Overview {...overviewProps()} />);

    expect(screen.getAllByRole('button', { name: /handoff/i })).toHaveLength(accounts.length);
    expect(screen.getAllByRole('button', { name: /relay latest/i })).toHaveLength(accounts.length);
  });

  it('labels an active External API account and removes only its quota refresh control', () => {
    render(<Overview {...overviewProps()} apiAccountIds={['a']} />);

    const externalCard = screen.getByRole('heading', { name: 'codex-a' }).closest('article');
    const normalCard = screen.getByRole('heading', { name: 'codex-b' }).closest('article');
    expect(externalCard).toBeTruthy();
    expect(normalCard).toBeTruthy();
    expect(within(externalCard!).getByText('External API')).toBeTruthy();
    expect(within(externalCard!).getByText('Active')).toBeTruthy();
    expect(
      within(externalCard!).queryByRole('button', { name: 'Refresh codex-a quota' }),
    ).toBeNull();
    expect(within(normalCard!).getByRole('button', { name: 'Refresh codex-b quota' })).toBeTruthy();
  });

  it('does not show Switch actions in Profile mode', () => {
    render(<Overview {...overviewProps()} authMode="oauth" />);

    expect(screen.queryByRole('button', { name: /switch to this account/i })).toBeNull();
    expect(screen.queryByText('Switch')).toBeNull();
  });

  it('disables Relay and Handoff, and shows Export CPA in PAT mode', () => {
    render(<Overview {...overviewProps()} authMode="pat" />);

    for (const name of [/relay latest/i, /handoff/i]) {
      for (const button of screen.queryAllByRole('button', { name })) {
        expect(button).toHaveProperty('disabled', true);
      }
    }

    const moreBtns = screen.getAllByRole('button', { name: /more options/i });
    expect(moreBtns).toHaveLength(accounts.length);
    for (const btn of moreBtns) {
      fireEvent.click(btn);
      expect(screen.getByRole('button', { name: /export cpa/i })).toBeTruthy();
    }
    expect(screen.queryByRole('button', { name: /sync sessions/i })).toBeNull();
  });

  it('uses Tauri confirm before resetting PAT quota', async () => {
    const resetAccountQuota = vi.fn();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    tauriDialogConfirm.mockResolvedValue(true);
    const patAccounts = [
      { ...accounts[0], isActiveAuth: true },
      { ...accounts[1], isActiveAuth: false },
    ];
    render(
      <Overview
        {...overviewProps()}
        accounts={patAccounts}
        authMode="pat"
        quotas={[{ ...quotas[0], resetCreditCount: 1 }]}
        resetAccountQuota={resetAccountQuota}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /reset codex-a quota/i }));

    await waitFor(() => expect(tauriDialogConfirm).toHaveBeenCalled());
    expect(confirmSpy).not.toHaveBeenCalled();
    await waitFor(() => expect(resetAccountQuota).toHaveBeenCalledWith('a'));
  });

  it('disables PAT reset when reset credit count is zero', async () => {
    const resetAccountQuota = vi.fn();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    tauriDialogConfirm.mockResolvedValue(true);
    const patAccounts = [
      { ...accounts[0], isActiveAuth: true },
      { ...accounts[1], isActiveAuth: false },
    ];
    render(
      <Overview
        {...overviewProps()}
        accounts={patAccounts}
        authMode="pat"
        quotas={[{ ...quotas[0], resetCreditCount: 0 }]}
        resetAccountQuota={resetAccountQuota}
      />,
    );

    const button = screen.getByRole('button', { name: /reset codex-a quota/i });
    expect(button).toHaveProperty('disabled', true);

    fireEvent.click(button);

    expect(confirmSpy).not.toHaveBeenCalled();
    expect(tauriDialogConfirm).not.toHaveBeenCalled();
    expect(resetAccountQuota).not.toHaveBeenCalled();
  });

  it('uses backend auth identity for PAT switch availability', () => {
    const patAccounts: CodexAccount[] = [
      {
        ...accounts[0],
        id: 'main',
        displayName: 'main',
        codexHome: '/tmp/.codex',
        isActiveAuth: false,
      },
      { ...accounts[0], isActiveAuth: true },
      { ...accounts[1], isActiveAuth: false },
    ];
    render(<Overview {...overviewProps()} accounts={patAccounts} authMode="pat" />);

    expect(screen.getByText('Active auth').nextElementSibling?.textContent).toBe('codex-a');
    expect(
      screen
        .getByRole('heading', { name: 'main' })
        .closest('article')
        ?.querySelector('[aria-label="Switch to this account"]'),
    ).toHaveProperty('disabled', true);
    expect(
      screen
        .getByRole('heading', { name: 'codex-a' })
        .closest('article')
        ?.querySelector('[aria-label="Switch to this account"]'),
    ).toBeNull();
    expect(
      screen
        .getByRole('heading', { name: 'codex-b' })
        .closest('article')
        ?.querySelector('[aria-label="Switch to this account"]'),
    ).toHaveProperty('disabled', false);
  });

  it('shows unrecognized when PAT auth has no unique profile match', () => {
    render(<Overview {...overviewProps()} authMode="pat" />);

    expect(screen.getByText('Unrecognized')).toBeTruthy();
    expect(screen.getByText('No unique tokens.account_id match')).toBeTruthy();
  });

  it('shows plan type beside account names when quota data includes it', () => {
    render(<Overview {...overviewProps()} />);

    expect(screen.getByText('TEAM')).toBeTruthy();
  });

  it('renders Antigravity quota summary groups with weekly and five hour buckets', () => {
    render(
      <Overview
        {...overviewProps()}
        antigravityQuota={{
          ok: true,
          models: [
            { label: 'Gemini Flash', remainingFraction: 0.91 },
            { label: 'Claude Sonnet', remainingFraction: 0.66 },
            { label: 'GPT-OSS', remainingFraction: 1 },
          ],
          description: 'Within each group, models share a weekly limit and a 5-hour limit.',
          groups: [
            {
              displayName: 'Gemini Models',
              description: 'Models within this group: Gemini Flash, Gemini Pro',
              buckets: [
                {
                  bucketId: 'gemini-weekly',
                  displayName: 'Weekly Limit',
                  description: 'Refreshes in 3 days',
                  window: 'weekly',
                  remainingFraction: 0.91,
                  resetTime: '2026-07-07T01:21:15Z',
                },
                {
                  bucketId: 'gemini-5h',
                  displayName: '5h',
                  window: '5h',
                  remainingFraction: 0.82,
                  resetTime: '2026-07-03T07:06:36Z',
                },
              ],
            },
            {
              displayName: 'Claude and GPT models',
              description: 'Models within this group: Claude Opus, Claude Sonnet, GPT-OSS',
              buckets: [
                {
                  bucketId: '3p-weekly',
                  displayName: 'Weekly Limit',
                  window: 'weekly',
                  remainingFraction: 0.66,
                  resetTime: '2026-07-06T05:08:03Z',
                },
                {
                  bucketId: '3p-5h',
                  displayName: '5h',
                  window: '5h',
                  remainingFraction: 1,
                  resetTime: '2026-07-03T11:11:26Z',
                },
              ],
            },
          ],
        }}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /antigravity/i }));

    expect(screen.getByRole('heading', { name: 'Gemini Models' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Claude and GPT models' })).toBeTruthy();
    expect(screen.getAllByText('Weekly Limit').length).toBeGreaterThan(0);
    expect(screen.getAllByText('5h').length).toBeGreaterThan(0);
    expect(screen.getByText('Gemini Flash')).toBeTruthy();
    expect(screen.getByText('Claude Sonnet')).toBeTruthy();
    expect(screen.getByText('GPT-OSS')).toBeTruthy();
    expect(screen.queryByText('No Antigravity models found.')).toBeNull();
  });

  it('counts Antigravity models and groups separately in overview metrics', () => {
    render(
      <Overview
        {...overviewProps()}
        antigravityQuota={{
          ok: true,
          models: [
            { label: 'Gemini Flash', remainingFraction: 0.9 },
            { label: 'Gemini Pro', remainingFraction: 0.8 },
            { label: 'Claude Sonnet', remainingFraction: 0.7 },
          ],
          groups: [
            {
              displayName: 'Gemini Models',
              description: 'Models within this group: Gemini Flash, Gemini Pro',
              buckets: [{ displayName: 'Weekly Limit', window: 'weekly', remainingFraction: 0.9 }],
            },
            {
              displayName: 'Claude and GPT models',
              description: 'Models within this group: Claude Sonnet, GPT-OSS',
              buckets: [{ displayName: 'Weekly Limit', window: 'weekly', remainingFraction: 0 }],
            },
          ],
        }}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /antigravity/i }));

    expect(screen.getByText('Models').nextElementSibling?.textContent).toBe('2/3');
    expect(screen.getByText('Groups').nextElementSibling?.textContent).toBe('2');
    expect(screen.getByText('Models usable').nextElementSibling?.textContent).toBe('2');
  });

  it('renders API-key-like Codex auth as generic Auth on account cards', () => {
    render(<Overview {...overviewProps()} accounts={[{ ...accounts[0], authMode: 'api_key' }]} />);

    expect(screen.getByText('Auth')).toBeTruthy();
    expect(screen.queryByText('API Key')).toBeNull();
  });

  it('shows readable reset-credit count and nearest expiry on account cards', () => {
    render(
      <Overview
        {...overviewProps()}
        quotas={[
          {
            ...quotas[0],
            resetCreditCount: 2,
            resetCreditExpirySource: 'api',
            resetCreditDetails: [
              { id: 'later', expiresAt: '2026-07-26T23:16:35+08:00', source: 'api' },
              { id: 'soon', expiresAt: '2026-07-11T23:18:30+08:00', source: 'api' },
            ],
          },
        ]}
      />,
    );

    expect(screen.getByText('Jul 11 23:18')).toBeTruthy();
  });

  it('uses one overview account action button size class', () => {
    render(<Overview {...overviewProps()} />);

    for (const name of [/relay latest/i, /handoff/i, /login/i]) {
      const buttons = screen.queryAllByRole('button', { name });
      for (const button of buttons) {
        expect(button.className).toContain('accountActionBtn');
      }
    }
  });

  it('shows and edits account renewal notes from account cards', async () => {
    const props = overviewProps();
    render(<Overview {...props} />);

    expect(screen.getByText('Renews 2026-07-15')).toBeTruthy();
    expect(screen.getByText('Team Plus renewal')).toBeTruthy();

    fireEvent.click(screen.getAllByRole('button', { name: /edit note/i })[0]);
    fireEvent.change(screen.getByLabelText('Renewal date'), {
      target: { value: '2026-08-01' },
    });
    fireEvent.change(screen.getByLabelText('Account note'), {
      target: { value: 'Annual invoice paid by ops' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() =>
      expect(props.onSaveAccountNote).toHaveBeenCalledWith({
        profileId: 'a',
        renewalDate: '2026-08-01',
        note: 'Annual invoice paid by ops',
      }),
    );
  });

  it('offers Relay To from a concrete session row', () => {
    render(
      <Sessions
        sessions={sessions}
        accounts={accounts}
        selectedAccountId="a"
        setSelectedAccountId={vi.fn()}
        query=""
        setQuery={vi.fn()}
        copy={vi.fn()}
        open={vi.fn()}
        details={vi.fn()}
        openHandoff={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: /relay to/i })).toBeTruthy();
  });

  it('shows Codex thread names as primary session labels while keeping the id visible', () => {
    render(
      <Sessions
        sessions={sessions}
        accounts={accounts}
        selectedAccountId="a"
        setSelectedAccountId={vi.fn()}
        query=""
        setQuery={vi.fn()}
        copy={vi.fn()}
        open={vi.fn()}
        details={vi.fn()}
        openHandoff={vi.fn()}
      />,
    );

    expect(screen.getByText('Implement relay session picker')).toBeTruthy();
    expect(screen.getByText('sid-a')).toBeTruthy();
  });
});
