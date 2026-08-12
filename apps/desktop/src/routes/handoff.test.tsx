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
    rename: vi.fn(),
    deleteAccount: vi.fn(),
    editApiAccount: vi.fn(),
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

  it('does not expose whole-account session Sync in navigation or account actions', () => {
    expect(routes.map((route) => route.id)).not.toContain('sync');
    expect(routes.map((route) => route.label)).not.toContain('Sync');

    render(<Overview {...overviewProps()} />);
    for (const button of screen.getAllByRole('button', { name: /more options/i })) {
      fireEvent.click(button);
      expect(screen.queryByRole('button', { name: /sync sessions/i })).toBeNull();
    }
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

  it('moves External API configuration into its menu and never offers Login', () => {
    const select = vi.fn();
    const editApiAccount = vi.fn();
    render(
      <Overview
        {...overviewProps()}
        apiAccountIds={['a']}
        select={select}
        editApiAccount={editApiAccount}
      />,
    );

    const externalCard = screen.getByRole('heading', { name: 'codex-a' }).closest('article');
    const normalCard = screen.getByRole('heading', { name: 'codex-b' }).closest('article');
    expect(
      within(externalCard!).queryByRole('button', { name: 'Edit codex-a API configuration' }),
    ).toBeNull();
    expect(
      externalCard!.querySelector('.cardHeadActions')?.querySelectorAll('button'),
    ).toHaveLength(1);

    fireEvent.click(within(externalCard!).getByRole('button', { name: 'More options' }));
    const edit = within(externalCard!).getByRole('button', {
      name: 'View&Edit',
    });
    expect(within(externalCard!).queryByRole('button', { name: 'Login' })).toBeNull();

    fireEvent.click(edit);
    expect(editApiAccount).toHaveBeenCalledWith(accounts[0]);
    expect(select).not.toHaveBeenCalled();

    fireEvent.click(within(normalCard!).getByRole('button', { name: 'More options' }));
    expect(within(normalCard!).getByRole('button', { name: 'Login' })).toBeTruthy();
  });

  it('switches account cards from compact menus to expanded actions from the heading', () => {
    const setCompactButtons = vi.fn();
    const editApiAccount = vi.fn();
    const select = vi.fn();
    const { rerender } = render(
      <Overview
        {...overviewProps()}
        apiAccountIds={['a']}
        compactButtons
        setCompactButtons={setCompactButtons}
      />,
    );

    const expand = screen.getByRole('button', { name: 'Show all account actions' });
    expect(expand.getAttribute('aria-pressed')).toBe('false');
    expect(screen.getAllByRole('button', { name: 'More options' })).toHaveLength(accounts.length);
    fireEvent.click(expand);
    expect(setCompactButtons).toHaveBeenCalledWith(false);

    rerender(
      <Overview
        {...overviewProps()}
        apiAccountIds={['a']}
        compactButtons={false}
        setCompactButtons={setCompactButtons}
        editApiAccount={editApiAccount}
        select={select}
      />,
    );

    const collapse = screen.getByRole('button', { name: 'Collapse account actions' });
    expect(collapse.getAttribute('aria-pressed')).toBe('true');
    expect(screen.queryByRole('button', { name: 'More options' })).toBeNull();

    const externalCard = screen.getByRole('heading', { name: 'codex-a' }).closest('article');
    for (const name of ['Switch Model', 'Handoff', 'View&Edit', 'Rename', 'Delete codex-a']) {
      expect(within(externalCard!).getByRole('button', { name })).toBeTruthy();
    }
    expect(within(externalCard!).queryByRole('button', { name: 'Login' })).toBeNull();

    fireEvent.click(within(externalCard!).getByRole('button', { name: 'View&Edit' }));
    expect(editApiAccount).toHaveBeenCalledWith(accounts[0]);
    expect(select).not.toHaveBeenCalled();

    const normalCard = screen.getByRole('heading', { name: 'codex-b' }).closest('article');
    for (const name of ['Relay Latest', 'Handoff', 'Reset Quota', 'Login', 'Rename']) {
      expect(within(normalCard!).getByRole('button', { name })).toBeTruthy();
    }
    expect(within(normalCard!).getByRole('button', { name: 'Delete codex-b' })).toBeTruthy();

    fireEvent.click(collapse);
    expect(setCompactButtons).toHaveBeenLastCalledWith(true);
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
    render(
      <Overview
        {...overviewProps()}
        accounts={patAccounts}
        authMode="pat"
        compactButtons={false}
      />,
    );

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
    ).toHaveProperty('disabled', true);
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

  it('previews and edits renewal notes from the account title without a body block', async () => {
    const select = vi.fn();
    const props = { ...overviewProps(), select };
    render(<Overview {...props} />);

    const card = screen.getByRole('heading', { name: 'codex-a' }).closest('article');
    const title = within(card!).getByRole('button', {
      name: 'Edit renewal information for codex-a',
    });
    const tooltip = within(card!).getByRole('tooltip');
    expect(card?.querySelector('.accountNoteSummary')).toBeNull();
    expect(within(tooltip).getByText('Renews 2026-07-15')).toBeTruthy();
    expect(within(tooltip).getByText('Team Plus renewal')).toBeTruthy();
    expect(within(tooltip).getByText('Click to edit')).toBeTruthy();

    fireEvent.click(title);
    expect(select).not.toHaveBeenCalled();
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
    await waitFor(() => expect(screen.queryByLabelText('Renewal date')).toBeNull());
  });

  it('keeps empty renewal notes out of the card body and cancels title editing', () => {
    const props = overviewProps();
    render(<Overview {...props} />);

    const card = screen.getByRole('heading', { name: 'codex-b' }).closest('article');
    const title = within(card!).getByRole('button', {
      name: 'Edit renewal information for codex-b',
    });
    const tooltip = within(card!).getByRole('tooltip');
    expect(within(card!).queryByText('+ Add renewal date or note')).toBeNull();
    expect(within(tooltip).getByText('No renewal information or note')).toBeTruthy();
    expect(within(tooltip).getByText('Click to add')).toBeTruthy();

    fireEvent.click(title);
    expect(screen.getByLabelText('Renewal date')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByLabelText('Renewal date')).toBeNull();
    expect(props.onSaveAccountNote).not.toHaveBeenCalled();
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

  it('loads an older Sessions page on demand', () => {
    const loadMore = vi.fn();
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
        hasMore
        loading={false}
        loadMore={loadMore}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /load more sessions/i }));
    expect(loadMore).toHaveBeenCalledTimes(1);
    expect(screen.getByPlaceholderText('Search loaded sessions')).toBeTruthy();
  });

  it('filters, size-sorts, and permanently deletes eligible loaded sessions', async () => {
    tauriDialogConfirm.mockResolvedValue(true);
    const oldSessions = Array.from({ length: 21 }, (_, index) => ({
      ...sessions[0],
      id: `sid-${index + 1}`,
      path: `/tmp/.codex-a/sessions/sid-${index + 1}.jsonl`,
      modifiedAt: Math.floor(Date.now() / 1000) - (index === 20 ? 8 * 24 * 60 * 60 : index),
      deletable: index === 20,
      deletionProtectionReason: index === 20 ? null : 'Protected',
    }));
    const deleteSelected = vi.fn();
    const selectAllFiltered = vi.fn().mockResolvedValue(['/tmp/.codex-a/sessions/sid-21.jsonl']);
    const setSort = vi.fn();
    const setAgeFilter = vi.fn();

    render(
      <Sessions
        sessions={oldSessions}
        storageSummary={{
          evaluatedAt: Math.floor(Date.now() / 1000),
          activeCount: 40,
          activeBytes: 4_000,
          eligibleCount: 12,
          eligibleBytes: 1_200,
          retainedRecentCount: 20,
          minimumAgeDays: 7,
        }}
        accounts={accounts}
        selectedAccountId="a"
        setSelectedAccountId={vi.fn()}
        query=""
        setQuery={vi.fn()}
        copy={vi.fn()}
        open={vi.fn()}
        details={vi.fn()}
        openHandoff={vi.fn()}
        sort="newest"
        ageFilter="all"
        setSort={setSort}
        setAgeFilter={setAgeFilter}
        deleteSelected={deleteSelected}
        selectAllFiltered={selectAllFiltered}
      />,
    );

    expect(screen.getByText(/40 active/)).toBeTruthy();
    expect(screen.getByText(/12 deletable/)).toBeTruthy();
    expect(screen.getAllByText('100 B').length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: 'All time' })).toBeTruthy();
    expect(screen.getByText('Showing 21 of 40 sessions')).toBeTruthy();
    expect(screen.getByRole('columnheader', { name: 'Last active' })).toBeTruthy();
    expect(document.querySelectorAll('.sessionsTable colgroup col')).toHaveLength(6);
    expect(document.querySelector('.sessionColActions')).toBeTruthy();
    expect(document.querySelector('time.sessionLastActive')?.getAttribute('datetime')).toBe(
      new Date(oldSessions[0].modifiedAt * 1000).toISOString(),
    );
    fireEvent.click(
      screen.getByRole('checkbox', { name: 'Select all filtered deletable sessions' }),
    );
    await waitFor(() => expect(selectAllFiltered).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: 'Delete selected (1)' })).toBeTruthy();
    fireEvent.click(
      screen.getByRole('checkbox', { name: 'Select all filtered deletable sessions' }),
    );
    expect(screen.getByRole('button', { name: 'Delete selected (0)' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Older than 30 days' }));
    expect(setAgeFilter).toHaveBeenCalledWith('olderThan30Days');
    fireEvent.change(screen.getByRole('combobox', { name: 'Sort sessions' }), {
      target: { value: 'largest' },
    });
    expect(setSort).toHaveBeenCalledWith('largest');
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select sid-21' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete selected (1)' }));
    await waitFor(() =>
      expect(deleteSelected).toHaveBeenCalledWith(['/tmp/.codex-a/sessions/sid-21.jsonl']),
    );
    expect(tauriDialogConfirm).toHaveBeenCalledWith(
      expect.stringContaining('token and usage statistics will also be removed'),
      expect.anything(),
    );
  });
});
