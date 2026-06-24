import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { routes } from './types';
import { Overview, Sessions } from './views';
import type { CodexAccount, CodexSession, UsageQuotaSnapshot } from '../lib/types';

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
    login: vi.fn(),
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
  it('does not expose Relay as a first-level route', () => {
    expect(routes.map((route) => route.id)).not.toContain('relay');
    expect(routes.map((route) => route.label)).not.toContain('Relay');
  });

  it('keeps both explicit Handoff and latest-session Relay shortcuts on account cards', () => {
    render(<Overview {...overviewProps()} />);

    expect(screen.getAllByRole('button', { name: /handoff/i })).toHaveLength(accounts.length);
    expect(screen.getAllByRole('button', { name: /relay latest/i })).toHaveLength(accounts.length);
  });

  it('shows plan type beside account names when quota data includes it', () => {
    render(<Overview {...overviewProps()} />);

    expect(screen.getByText('TEAM')).toBeTruthy();
  });

  it('uses one overview account action button size class', () => {
    render(<Overview {...overviewProps()} />);

    for (const name of [/relay latest/i, /handoff/i, /sync sessions/i, /rename/i, /login/i]) {
      for (const button of screen.getAllByRole('button', { name })) {
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
