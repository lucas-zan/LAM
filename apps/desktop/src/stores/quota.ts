import { create } from 'zustand';
import * as api from '../lib/api';
import { filterQuotaSnapshotsForProfileIds, mergeQuotaSnapshots } from '../lib/quota';
import type { UsageQuotaSnapshot } from '../lib/types';
import { useAppStore } from './app';
import { formatError } from '../lib/format';

const QUOTA_REFRESH_INTERVAL_MS = 2 * 60_000;

interface QuotaState {
  quotas: UsageQuotaSnapshot[];
  refreshingQuotaIds: string[];
  resettingQuotaIds: string[];
  _timerId: number | null;
  _intervalId: number | null;

  refreshQuotas: (profileIds?: string[]) => Promise<void>;
  refreshAccountQuota: (profileId: string) => void;
  resetAccountQuota: (profileId: string) => Promise<void>;
  loadCachedQuotas: (profileIds?: string[]) => void;
  scheduleQuotaRefresh: (profileIds: string[], delayMs: number) => void;
  startAutoRefresh: (profileIds: string[]) => void;
  stopAutoRefresh: () => void;
  filterToProfileIds: (profileIds: string[]) => void;
  clearQuotas: () => void;
}

export const useQuotaStore = create<QuotaState>()((set, get) => ({
  quotas: [],
  refreshingQuotaIds: [],
  resettingQuotaIds: [],
  _timerId: null,
  _intervalId: null,

  refreshQuotas: async (profileIds) => {
    const targets = profileIds?.length ? profileIds : [];
    if (!targets.length) return;
    useAppStore.getState().clearError();
    set((s) => ({
      refreshingQuotaIds: Array.from(new Set([...s.refreshingQuotaIds, ...targets])),
    }));

    let completed = 0;
    let failed = 0;

    await Promise.all(
      targets.map(async (profileId) => {
        try {
          const snapshot = await api.getProfileQuota(profileId, true);
          set((s) => ({ quotas: mergeQuotaSnapshots(s.quotas, snapshot) }));
          completed += 1;
          if (snapshot.staleness !== 'fresh') {
            failed += 1;
            useAppStore
              .getState()
              .setError(
                `${profileId}: realtime quota unavailable; using ${snapshot.staleness} quota`,
              );
          } else {
            useAppStore.getState().clearError();
          }
        } catch (err) {
          failed += 1;
          useAppStore.getState().setError(`${profileId}: ${formatError(err)}`);
        } finally {
          set((s) => ({
            refreshingQuotaIds: s.refreshingQuotaIds.filter((id) => id !== profileId),
          }));
        }
      }),
    );

    useAppStore
      .getState()
      .setStatus(
        failed
          ? `Refreshed ${completed} quota snapshots; ${failed} unavailable`
          : `Refreshed ${completed} quota snapshots`,
      );
    if (!failed) useAppStore.getState().clearError();
    if (completed) api.syncTrayQuota();
  },

  refreshAccountQuota: (profileId) => {
    get().refreshQuotas([profileId]);
  },

  resetAccountQuota: async (profileId) => {
    if (get().resettingQuotaIds.includes(profileId)) return;
    useAppStore.getState().clearError();
    set((s) => ({ resettingQuotaIds: [...s.resettingQuotaIds, profileId] }));
    try {
      const result = await api.resetProfileQuota(profileId);
      set((s) => ({ quotas: mergeQuotaSnapshots(s.quotas, result.snapshot) }));
      useAppStore.getState().setStatus(`Reset quota: ${result.outcome}`);
      api.syncTrayQuota();
    } catch (err) {
      useAppStore.getState().setError(`${profileId}: ${formatError(err)}`);
    } finally {
      set((s) => ({
        resettingQuotaIds: s.resettingQuotaIds.filter((id) => id !== profileId),
      }));
    }
  },

  loadCachedQuotas: (profileIds) => {
    api
      .listCachedQuotas(profileIds)
      .then((cached) => {
        if (!cached.length) return;
        set((s) => {
          const ids = profileIds ?? cached.map((snap) => snap.profileId);
          const scoped = filterQuotaSnapshotsForProfileIds(ids, s.quotas);
          const next = new Map(scoped.map((q) => [q.profileId, q]));
          for (const snap of cached) {
            const current = next.get(snap.profileId);
            if (!current || snap.fetchedAt >= current.fetchedAt) {
              next.set(snap.profileId, snap);
            }
          }
          return { quotas: Array.from(next.values()) };
        });
        useAppStore.getState().setStatus(`Loaded ${cached.length} cached quota snapshots`);
      })
      .catch((err) => useAppStore.getState().setError(formatError(err)));
  },

  scheduleQuotaRefresh: (profileIds, delayMs) => {
    if (!profileIds.length) return;
    const { _timerId } = get();
    if (_timerId !== null) window.clearTimeout(_timerId);
    const id = window.setTimeout(() => {
      set({ _timerId: null });
      get().refreshQuotas(profileIds);
    }, delayMs);
    set({ _timerId: id });
  },

  startAutoRefresh: (profileIds) => {
    get().stopAutoRefresh();
    const id = window.setInterval(() => {
      get().scheduleQuotaRefresh(profileIds, 0);
    }, QUOTA_REFRESH_INTERVAL_MS);
    set({ _intervalId: id });
  },

  stopAutoRefresh: () => {
    const { _intervalId, _timerId } = get();
    if (_intervalId !== null) window.clearInterval(_intervalId);
    if (_timerId !== null) window.clearTimeout(_timerId);
    set({ _intervalId: null, _timerId: null });
  },

  filterToProfileIds: (profileIds) => {
    set((s) => ({ quotas: filterQuotaSnapshotsForProfileIds(profileIds, s.quotas) }));
  },

  clearQuotas: () => set({ quotas: [] }),
}));
