import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TrayQuotaPanel } from './tray-quota-panel';
import * as api from '../lib/api';
import { listen } from '@tauri-apps/api/event';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: vi.fn(() => ({
    hide: vi.fn(),
    listen: vi.fn(() => Promise.resolve(vi.fn())),
  })),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../lib/tray-popover-size', () => ({
  scheduleTrayPopoverWindowSize: vi.fn(),
}));

vi.mock('../lib/api', () => ({
  hideQuotaPopover: vi.fn(),
  listAccounts: vi.fn(),
  listCachedAccounts: vi.fn(),
  listCachedQuotas: vi.fn(),
  listSessions: vi.fn(),
  openTerminalWithCommand: vi.fn(),
  openTerminalWithResume: vi.fn(),
  relayResumeSession: vi.fn(),
  getProfileQuota: vi.fn(),
  inTauri: vi.fn(() => true),
  setQuotaPopoverOpacity: vi.fn(),
  getAntigravityQuota: vi.fn(),
}));

const account = {
  id: 'main',
  displayName: 'main',
  codexHome: '/tmp/.codex',
  wrapperPath: null,
  hasAuth: true,
  hasConfig: true,
  hasHistory: false,
  sessionCount: 1,
  latestSessionModifiedAt: 1,
  managed: false,
  isRelay: false,
  relaySource: null,
  relayIdentity: null,
  providerId: 'openai',
  model: 'gpt-5-codex',
  authMode: 'config',
};

const cachedQuota = {
  profileId: 'main',
  source: 'app_server_rate_limits',
  fetchedAt: 1,
  staleness: 'cached',
  planType: 'team',
  activityTokens: null,
  primaryUsedPercent: 40,
  secondaryUsedPercent: 10,
  remainingPercent: 60,
  resetAt: '2026-06-16T10:00:00Z',
  secondaryResetAt: '2026-06-17T10:00:00Z',
  alerts: [],
  suggestedActions: [],
};

const freshQuota = {
  ...cachedQuota,
  fetchedAt: 2,
  staleness: 'fresh',
  primaryUsedPercent: 20,
  remainingPercent: 80,
};

function setTauriInternals(enabled: boolean) {
  if (enabled) {
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    return;
  }
  Reflect.deleteProperty(window, '__TAURI_INTERNALS__');
}

beforeEach(() => {
  vi.clearAllMocks();
  setTauriInternals(true);
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: vi.fn(() => ({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  });
  Object.defineProperty(window, 'ResizeObserver', {
    configurable: true,
    value: class {
      observe() {}
      disconnect() {}
    },
  });
  localStorage.clear();
  vi.mocked(api.listCachedAccounts).mockResolvedValue([account]);
  vi.mocked(api.listAccounts).mockResolvedValue([account]);
  vi.mocked(api.listCachedQuotas).mockResolvedValue([cachedQuota]);
  vi.mocked(api.listSessions).mockResolvedValue([
    {
      id: 's1',
      accountId: 'main',
      path: '/tmp/session.jsonl',
      cwd: '/tmp',
      modifiedAt: 1,
      sizeBytes: 1,
      model: 'gpt-5-codex',
      summary: null,
      originalProviderId: 'openai',
      originalModel: 'gpt-5-codex',
      currentProviderId: 'openai',
      currentModel: 'gpt-5-codex',
      providerMismatch: false,
    },
  ]);
  vi.mocked(api.getProfileQuota).mockResolvedValue(freshQuota);
  vi.mocked(api.getAntigravityQuota).mockResolvedValue({ ok: true, models: [] });
  vi.mocked(listen).mockResolvedValue(vi.fn());
});

describe('TrayQuotaPanel', () => {
  it('uses cached quota when the main app sync event fires', async () => {
    const listeners = new Map<string, Array<() => void>>();
    vi.mocked(listen).mockImplementation((event, handler) => {
      listeners.set(event, [...(listeners.get(event) ?? []), handler as () => void]);
      return Promise.resolve(vi.fn());
    });

    const { container } = render(<TrayQuotaPanel />);
    await waitFor(() => expect(screen.getAllByText('60%').length).toBeGreaterThan(0));
    expect(screen.getByText('TEAM')).toBeTruthy();
    expect(container.querySelector('.trayAccountPlanLine')?.textContent).toContain('TEAM');

    const cachedReads = vi.mocked(api.listCachedQuotas).mock.calls.length;
    listeners.get('quota-popover-refresh')?.forEach((handler) => handler());
    await waitFor(() =>
      expect(vi.mocked(api.listCachedQuotas).mock.calls.length).toBeGreaterThan(cachedReads),
    );

    expect(api.getProfileQuota).not.toHaveBeenCalled();
  });

  it('refreshes only the selected account from its row action', async () => {
    const { container } = render(<TrayQuotaPanel />);
    await waitFor(() => expect(screen.getAllByText('60%').length).toBeGreaterThan(0));
    expect(
      container.querySelector('.trayAccountRow')?.classList.contains('trayAccountRow--monthlyOnly'),
    ).toBe(false);

    fireEvent.click(screen.getByRole('button', { name: 'Refresh main quota' }));

    await waitFor(() => expect(api.getProfileQuota).toHaveBeenCalledWith('main', true));
    expect(api.getProfileQuota).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.getAllByText('80%').length).toBeGreaterThan(0));
  });

  it('shows only the monthly window for monthly-only quota accounts', async () => {
    vi.mocked(api.listCachedQuotas).mockResolvedValue([
      {
        ...cachedQuota,
        primaryUsedPercent: 11,
        primaryWindowDurationMins: 43800,
        secondaryUsedPercent: null,
        secondaryWindowDurationMins: null,
        remainingPercent: 89,
        resetAt: '1784724636',
        secondaryResetAt: null,
      },
    ]);

    const { container } = render(<TrayQuotaPanel />);

    await waitFor(() => expect(screen.getByText('monthly')).toBeTruthy());
    expect(
      container.querySelector('.trayAccountRow')?.classList.contains('trayAccountRow--monthlyOnly'),
    ).toBe(true);
    expect(screen.getAllByText('89%').length).toBeGreaterThan(0);
    expect(screen.queryByText('weekly')).toBeNull();
    expect(screen.queryByText('5h')).toBeNull();
  });
});
