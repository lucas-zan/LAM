import { create } from 'zustand';
import * as api from '../lib/api';
import type { UsageDashboard, UsageDashboardRequest } from '../lib/types';
import { formatError } from '../lib/format';
import { useAppStore } from './app';

export const defaultUsageSummaryRequest = (): UsageDashboardRequest => ({
  window: { preset: 'all', from: null, to: null },
  includeArchived: false,
  search: null,
  model: null,
  effort: null,
  pricingConfidence: null,
  sortKey: 'time',
  sortDirection: 'desc',
  limit: null,
});

interface UsageState {
  summary: UsageDashboard | null;
  refreshing: boolean;
  loadUsageSummary: (req?: UsageDashboardRequest) => Promise<void>;
  refreshUsage: (req?: UsageDashboardRequest) => Promise<void>;
}

export const useUsageStore = create<UsageState>()((set) => ({
  summary: null,
  refreshing: false,

  loadUsageSummary: async (req = defaultUsageSummaryRequest()) => {
    try {
      set({ summary: await api.getUsageDashboard(req) });
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
    }
  },

  refreshUsage: async (req = defaultUsageSummaryRequest()) => {
    set({ refreshing: true });
    try {
      await api.refreshUsageIndex(req.includeArchived);
      set({ summary: await api.getUsageDashboard(req) });
      useAppStore.getState().setStatus('Refreshed Codex usage statistics');
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
    } finally {
      set({ refreshing: false });
    }
  },
}));
