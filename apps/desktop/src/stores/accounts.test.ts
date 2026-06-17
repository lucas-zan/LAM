import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAccountStore } from "./accounts";
import { useAppStore } from "./app";
import { useQuotaStore } from "./quota";
import { useProviderStore } from "./providers";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  inTauri: vi.fn(() => true),
  listCachedAccounts: vi.fn(),
  healthCheck: vi.fn(),
  listAccounts: vi.fn(),
  listProviders: vi.fn(),
  listSessions: vi.fn(),
  getProfileQuota: vi.fn(),
  listCachedQuotas: vi.fn(),
  syncTrayQuota: vi.fn(),
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
    route: "overview",
    status: "Ready",
    error: "",
    appReady: false,
    modal: null,
  });
  useAccountStore.setState({
    accounts: [],
    selectedAccountId: "",
    activeSession: undefined,
    divergedStrategy: "summarize_fork_with_target_account",
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
    version: "test",
    homeRoot: "/tmp",
  });
  vi.mocked(api.listProviders).mockResolvedValue([]);
  vi.mocked(api.listSessions).mockResolvedValue([]);
  vi.mocked(api.listCachedQuotas).mockResolvedValue([]);
  vi.mocked(api.getProfileQuota).mockResolvedValue({
    profileId: "a",
    source: "app_server_rate_limits",
    fetchedAt: 1,
    staleness: "fresh",
    planType: "plus",
    activityTokens: null,
    primaryUsedPercent: 40,
    secondaryUsedPercent: 20,
    remainingPercent: 60,
    resetAt: "2026-06-16T10:00:00Z",
    secondaryResetAt: "2026-06-17T10:00:00Z",
    alerts: [],
    suggestedActions: [],
  });
});

describe("useAccountStore", () => {
  it("tracks refresh state while the app refresh button is running", async () => {
    const accounts = deferred<Awaited<ReturnType<typeof api.listAccounts>>>();
    vi.mocked(api.listAccounts).mockReturnValue(accounts.promise);

    const refresh = useAccountStore.getState().refresh();
    expect(useAccountStore.getState().refreshing).toBe(true);

    accounts.resolve([]);
    await refresh;
    expect(useAccountStore.getState().refreshing).toBe(false);
  });

  it("manual refresh immediately refreshes quota for each account", async () => {
    vi.mocked(api.listAccounts).mockResolvedValue([
      {
        id: "a",
        displayName: "codex-a",
        codexHome: "/tmp/.codex-a",
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
        providerId: "openai",
        model: "gpt-5-codex",
        authMode: "config",
      },
    ]);

    await useAccountStore.getState().refresh({ refreshQuotasNow: true });

    expect(api.getProfileQuota).toHaveBeenCalledWith("a", true);
    expect(useQuotaStore.getState().quotas).toHaveLength(1);
  });
});
