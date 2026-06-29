import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppStore } from "./app";
import { useQuotaStore } from "./quota";
import * as api from "../lib/api";

vi.mock("../lib/api", () => ({
  refreshAllQuotas: vi.fn(),
  getProfileQuota: vi.fn(),
  resetProfileQuota: vi.fn(),
  listCachedQuotas: vi.fn(),
  syncTrayQuota: vi.fn(),
}));

const snapshot = {
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
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
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
  useQuotaStore.setState({
    quotas: [],
    refreshingQuotaIds: [],
    resettingQuotaIds: [],
    _timerId: null,
    _intervalId: null,
  });
});

describe("useQuotaStore", () => {
  it("refreshes accounts concurrently and updates each account as it returns", async () => {
    const first = deferred<typeof snapshot>();
    const second = deferred<typeof snapshot>();
    const snapshotB = { ...snapshot, profileId: "b", primaryUsedPercent: 10 };
    vi.mocked(api.getProfileQuota).mockImplementation((profileId) => {
      if (profileId === "a") return first.promise;
      if (profileId === "b") return second.promise;
      throw new Error(`unexpected profile ${profileId}`);
    });

    const refresh = useQuotaStore.getState().refreshQuotas(["a", "b"]);
    expect(api.getProfileQuota).toHaveBeenCalledWith("a", true);
    expect(api.getProfileQuota).toHaveBeenCalledWith("b", true);
    expect(api.refreshAllQuotas).not.toHaveBeenCalled();
    expect(useQuotaStore.getState().refreshingQuotaIds).toEqual(["a", "b"]);

    first.resolve(snapshot);
    await flush();
    expect(useQuotaStore.getState().quotas).toEqual([snapshot]);
    expect(useQuotaStore.getState().refreshingQuotaIds).toEqual(["b"]);

    second.resolve(snapshotB);
    await refresh;
    expect(useQuotaStore.getState().quotas).toEqual([snapshot, snapshotB]);
    expect(useQuotaStore.getState().refreshingQuotaIds).toEqual([]);
    expect(useAppStore.getState().status).toBe("Refreshed 2 quota snapshots");
    expect(api.syncTrayQuota).toHaveBeenCalled();
  });

  it("surfaces backend quota warnings without dropping successful snapshots", async () => {
    const cachedSnapshot = { ...snapshot, staleness: "cached" };
    vi.mocked(api.getProfileQuota).mockResolvedValue(cachedSnapshot);

    await useQuotaStore.getState().refreshQuotas(["a"]);

    expect(useQuotaStore.getState().quotas).toEqual([cachedSnapshot]);
    expect(useAppStore.getState().status).toBe(
      "Refreshed 1 quota snapshots; 1 unavailable",
    );
    expect(useAppStore.getState().error).toBe(
      "a: realtime quota unavailable; using cached quota",
    );
  });

  it("clears stale quota warnings after a fresh account refresh", async () => {
    useAppStore
      .getState()
      .setError("c: realtime quota unavailable; using cached quota");
    vi.mocked(api.getProfileQuota).mockResolvedValue({
      ...snapshot,
      profileId: "c",
      staleness: "fresh",
    });

    await useQuotaStore.getState().refreshQuotas(["c"]);

    expect(useQuotaStore.getState().quotas[0]).toMatchObject({
      profileId: "c",
      staleness: "fresh",
    });
    expect(useAppStore.getState().error).toBe("");
  });

  it("resets an account quota and merges the fresh snapshot", async () => {
    const fresh = { ...snapshot, resetCreditCount: 0, primaryUsedPercent: 3 };
    vi.mocked(api.resetProfileQuota).mockResolvedValue({
      snapshot: fresh,
      outcome: "reset",
      operationId: "op-1",
    });

    await useQuotaStore.getState().resetAccountQuota("a");

    expect(api.resetProfileQuota).toHaveBeenCalledWith("a");
    expect(useQuotaStore.getState().quotas).toEqual([fresh]);
    expect(useQuotaStore.getState().resettingQuotaIds).toEqual([]);
    expect(useAppStore.getState().status).toBe("Reset quota: reset");
    expect(api.syncTrayQuota).toHaveBeenCalled();
  });
});
