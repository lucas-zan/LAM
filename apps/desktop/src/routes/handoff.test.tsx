import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { routes } from './types';
import { Overview, Sessions } from './views';
import type { CodexAccount, CodexSession } from '../lib/types';

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
    summary: 'task a',
    model: 'gpt-5',
    currentProviderId: 'openai',
    currentModel: 'gpt-5',
    providerMismatch: false,
  },
];

function overviewProps() {
  return {
    accounts,
    quotas: [],
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
  };
}

describe('handoff navigation and entry points', () => {
  it('does not expose Relay as a first-level route', () => {
    expect(routes.map((route) => route.id)).not.toContain('relay');
    expect(routes.map((route) => route.label)).not.toContain('Relay');
  });

  it('keeps both explicit Handoff and latest-session Relay shortcuts on account cards', () => {
    render(
      <Overview
        {...overviewProps()}
      />,
    );

    expect(screen.getAllByRole('button', { name: /handoff/i })).toHaveLength(accounts.length);
    expect(screen.getAllByRole('button', { name: /relay latest/i })).toHaveLength(accounts.length);
  });

  it('uses one overview account action button size class', () => {
    render(
      <Overview
        {...overviewProps()}
      />,
    );

    for (const name of [/relay latest/i, /handoff/i, /sync sessions/i, /rename/i, /login/i]) {
      for (const button of screen.getAllByRole('button', { name })) {
        expect(button.className).toContain('accountActionBtn');
      }
    }
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
});
