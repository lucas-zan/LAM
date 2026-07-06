import { create } from 'zustand';
import * as api from '../lib/api';
import type {
  UsageDashboard,
  UsageDashboardRequest,
  UsageScope,
  UsageSection,
  UsageSectionLoading,
} from '../lib/types';
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
  offset: null,
  scopeId: null,
  accountId: null,
});

interface UsageState {
  summary: UsageDashboard | null;
  scopes: UsageScope[];
  activeScopeId: string | null;
  refreshing: boolean;
  sectionLoading: UsageSectionLoading;
  loadUsageSummary: (req?: UsageDashboardRequest) => Promise<void>;
  loadUsageSection: (section: UsageSection, req?: UsageDashboardRequest) => Promise<void>;
  refreshUsageSections: (sections: UsageSection[], req?: UsageDashboardRequest) => Promise<void>;
  refreshUsageSection: (section: UsageSection, req?: UsageDashboardRequest) => Promise<void>;
  refreshUsage: (req?: UsageDashboardRequest) => Promise<void>;
  selectUsageScope: (scopeId: string) => void;
}

const emptySectionLoading = (): UsageSectionLoading => ({
  scopes: false,
  overview: false,
  activity: false,
  insights: false,
  calls: false,
  threads: false,
  diagnostics: false,
});

function mergeSummary(
  current: UsageDashboard | null,
  next: Partial<UsageDashboard>,
): UsageDashboard {
  return {
    ...(current ?? (next as UsageDashboard)),
    ...next,
    recentCalls: next.recentCalls ?? current?.recentCalls ?? [],
    topThreads: next.topThreads ?? current?.topThreads ?? [],
    activityBuckets: next.activityBuckets ?? current?.activityBuckets ?? [],
    insights: next.insights ?? current?.insights ?? null,
    callsPage: next.callsPage ?? current?.callsPage ?? null,
    threadsPage: next.threadsPage ?? current?.threadsPage ?? null,
  } as UsageDashboard;
}

function overviewOnly(summary: UsageDashboard): Partial<UsageDashboard> {
  const {
    activityBuckets: _activityBuckets,
    recentCalls: _recentCalls,
    topThreads: _topThreads,
    ...overview
  } = summary;
  return overview;
}

export const useUsageStore = create<UsageState>()((set) => ({
  summary: null,
  scopes: [],
  activeScopeId: null,
  refreshing: false,
  sectionLoading: emptySectionLoading(),

  loadUsageSummary: async (req = defaultUsageSummaryRequest()) => {
    set((state) => ({
      sectionLoading: {
        ...state.sectionLoading,
        scopes: true,
        overview: true,
        activity: true,
      },
    }));
    const tasks = [
      api
        .getUsageScopes(req)
        .then((response) => {
          set({ scopes: response.scopes, activeScopeId: response.activeScopeId });
        })
        .catch((err) => {
          useAppStore.getState().setError(formatError(err));
        })
        .finally(() => {
          set((state) => ({
            sectionLoading: { ...state.sectionLoading, scopes: false },
          }));
        }),
      api
        .getUsageOverview(req)
        .then((overview) => {
          set((state) => ({ summary: mergeSummary(state.summary, overviewOnly(overview)) }));
        })
        .catch((err) => {
          useAppStore.getState().setError(formatError(err));
        })
        .finally(() => {
          set((state) => ({
            sectionLoading: { ...state.sectionLoading, overview: false },
          }));
        }),
      api
        .getUsageActivity(req)
        .then((activityBuckets) => {
          set((state) => ({ summary: mergeSummary(state.summary, { activityBuckets }) }));
        })
        .catch((err) => {
          useAppStore.getState().setError(formatError(err));
        })
        .finally(() => {
          set((state) => ({
            sectionLoading: { ...state.sectionLoading, activity: false },
          }));
        }),
    ];
    await Promise.allSettled(tasks);
  },

  loadUsageSection: async (section, req = defaultUsageSummaryRequest()) => {
    set((state) => ({
      sectionLoading: {
        ...state.sectionLoading,
        [section]: true,
      },
    }));
    try {
      if (section === 'scopes') {
        const response = await api.getUsageScopes(req);
        set({ scopes: response.scopes, activeScopeId: response.activeScopeId });
        return;
      }
      if (section === 'overview') {
        const overview = await api.getUsageOverview(req);
        set((state) => ({ summary: mergeSummary(state.summary, overviewOnly(overview)) }));
        return;
      }
      if (section === 'activity') {
        const activityBuckets = await api.getUsageActivity(req);
        set((state) => ({ summary: mergeSummary(state.summary, { activityBuckets }) }));
        return;
      }
      if (section === 'insights') {
        const insights = await api.getUsageInsights(req);
        set((state) => ({ summary: mergeSummary(state.summary, { insights }) }));
        return;
      }
      if (section === 'calls') {
        const callsPage = await api.getUsageCalls(req);
        set((state) => ({
          summary: mergeSummary(state.summary, { callsPage, recentCalls: callsPage.rows }),
        }));
        return;
      }
      if (section === 'threads') {
        const threadsPage = await api.getUsageThreads(req);
        set((state) => ({
          summary: mergeSummary(state.summary, { threadsPage, topThreads: threadsPage.rows }),
        }));
        return;
      }
      const diagnostics = await api.getUsageDiagnostics(req);
      set((state) => ({ summary: mergeSummary(state.summary, { diagnostics }) }));
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
    } finally {
      set((state) => ({
        sectionLoading: {
          ...state.sectionLoading,
          [section]: false,
        },
      }));
    }
  },

  refreshUsageSections: async (sections, req = defaultUsageSummaryRequest()) => {
    set((state) => ({
      refreshing: true,
      sectionLoading: sections.reduce(
        (loading, section) => ({ ...loading, [section]: true }),
        state.sectionLoading,
      ),
    }));
    try {
      await api.refreshUsageIndex(req.includeArchived);
      await Promise.all(
        sections.map((section) => useUsageStore.getState().loadUsageSection(section, req)),
      );
      useAppStore.getState().setStatus('Refreshed Codex usage statistics');
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
    } finally {
      set((state) => ({
        refreshing: false,
        sectionLoading: sections.reduce(
          (loading, section) => ({ ...loading, [section]: false }),
          state.sectionLoading,
        ),
      }));
    }
  },

  refreshUsageSection: async (section, req = defaultUsageSummaryRequest()) => {
    await useUsageStore.getState().refreshUsageSections([section], req);
  },

  selectUsageScope: (scopeId) => {
    set({ activeScopeId: scopeId });
  },

  refreshUsage: async (req = defaultUsageSummaryRequest()) => {
    set({ refreshing: true });
    try {
      await api.refreshUsageIndex(req.includeArchived);
      await useUsageStore.getState().loadUsageSummary(req);
      useAppStore.getState().setStatus('Refreshed Codex usage statistics');
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
    } finally {
      set({ refreshing: false });
    }
  },
}));
