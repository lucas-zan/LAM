import { create } from 'zustand';
import * as api from '../lib/api';
import type {
  CodexSession,
  ResumeCommand,
  SessionAgeFilter,
  SessionPageCursor,
  SessionSort,
  SessionStorageSummary,
} from '../lib/types';
import { useAppStore } from './app';
import { useUsageStore } from './usage';
import { formatError } from '../lib/format';

const SESSION_PAGE_SIZE = 20;

function mergeSessions(
  current: CodexSession[],
  next: CodexSession[],
  sort: SessionSort,
): CodexSession[] {
  const merged = new Map(current.map((session) => [session.path, session]));
  for (const session of next) merged.set(session.path, session);
  return [...merged.values()].sort((left, right) => {
    if (sort === 'largest')
      return right.sizeBytes - left.sizeBytes || left.path.localeCompare(right.path);
    if (sort === 'smallest')
      return left.sizeBytes - right.sizeBytes || left.path.localeCompare(right.path);
    return right.modifiedAt - left.modifiedAt || left.path.localeCompare(right.path);
  });
}

interface SessionState {
  sessions: CodexSession[];
  selectedSessionId: string;
  query: string;
  resume: ResumeCommand | null;
  loadedAccountId: string;
  cursor: SessionPageCursor | null;
  hasMore: boolean;
  loading: boolean;
  loadGeneration: number;
  sort: SessionSort;
  ageFilter: SessionAgeFilter;
  managementAccountId: string;
  managementLoading: boolean;
  managementGeneration: number;
  mutationRunning: boolean;
  storageSummary: SessionStorageSummary | null;

  selectedSession: () => CodexSession | undefined;
  filteredSessions: () => CodexSession[];
  setSelectedSessionId: (id: string) => void;
  setQuery: (q: string) => void;
  setResume: (r: ResumeCommand | null) => void;
  setSort: (sort: SessionSort) => Promise<void>;
  setAgeFilter: (age: SessionAgeFilter) => Promise<void>;
  loadSessions: (accountId: string) => Promise<void>;
  loadMoreSessions: () => Promise<void>;
  loadSessionManagement: (accountId: string) => Promise<void>;
  selectAllFilteredSessions: () => Promise<string[]>;
  deleteSelectedSessions: (paths: string[]) => Promise<void>;
  clear: () => void;
  previewResume: (session?: CodexSession) => Promise<void>;
  copyResume: (session?: CodexSession) => Promise<void>;
  openResume: (session?: CodexSession) => Promise<void>;
  openSessionDetails: (session?: CodexSession) => Promise<void>;
}

export const useSessionStore = create<SessionState>()((set, get) => ({
  sessions: [],
  selectedSessionId: '',
  query: '',
  resume: null,
  loadedAccountId: '',
  cursor: null,
  hasMore: false,
  loading: false,
  loadGeneration: 0,
  sort: 'newest',
  ageFilter: 'all',
  managementAccountId: '',
  managementLoading: false,
  managementGeneration: 0,
  mutationRunning: false,
  storageSummary: null,

  selectedSession: () => {
    const { sessions, selectedSessionId } = get();
    return sessions.find((s) => s.id === selectedSessionId) ?? sessions[0];
  },

  filteredSessions: () => {
    const { sessions, query } = get();
    const needle = query.trim().toLowerCase();
    if (!needle) return sessions;
    return sessions.filter((s) =>
      [s.id, s.threadName, s.cwd, s.summary, s.path, s.model]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(needle),
    );
  },

  setSelectedSessionId: (id) => set({ selectedSessionId: id }),
  setQuery: (query) => set({ query }),
  setResume: (resume) => set({ resume }),
  setSort: async (sort) => {
    set({ sort });
    const accountId = get().loadedAccountId;
    if (accountId) await get().loadSessions(accountId);
  },
  setAgeFilter: async (ageFilter) => {
    set({ ageFilter });
    const accountId = get().loadedAccountId;
    if (accountId) await get().loadSessions(accountId);
  },

  loadSessions: async (accountId) => {
    const generation = get().loadGeneration + 1;
    set({
      sessions: [],
      selectedSessionId: '',
      loadedAccountId: accountId,
      cursor: null,
      hasMore: false,
      loading: true,
      loadGeneration: generation,
    });
    try {
      const { sort, ageFilter } = get();
      const page = await api.querySessionsPage(accountId, {
        limit: SESSION_PAGE_SIZE,
        sort,
        age: ageFilter,
      });
      if (get().loadGeneration !== generation || get().loadedAccountId !== accountId) return;
      set({
        sessions: page.items,
        selectedSessionId: page.items[0]?.id ?? '',
        cursor: page.nextCursor ?? null,
        hasMore: Boolean(page.nextCursor),
      });
    } catch (err) {
      if (get().loadGeneration === generation && get().loadedAccountId === accountId) {
        useAppStore.getState().setError(formatError(err));
      }
    } finally {
      if (get().loadGeneration === generation && get().loadedAccountId === accountId) {
        set({ loading: false });
      }
    }
  },

  loadMoreSessions: async () => {
    const { loadedAccountId, cursor, loading, loadGeneration, sort, ageFilter } = get();
    if (!loadedAccountId || !cursor || loading) return;
    set({ loading: true });
    try {
      const page = await api.querySessionsPage(loadedAccountId, {
        limit: SESSION_PAGE_SIZE,
        cursor,
        sort,
        age: ageFilter,
      });
      if (get().loadGeneration !== loadGeneration || get().loadedAccountId !== loadedAccountId)
        return;
      set((state) => ({
        sessions: mergeSessions(state.sessions, page.items, sort),
        cursor: page.nextCursor ?? null,
        hasMore: Boolean(page.nextCursor),
      }));
    } catch (err) {
      if (get().loadGeneration === loadGeneration && get().loadedAccountId === loadedAccountId) {
        useAppStore.getState().setError(formatError(err));
      }
    } finally {
      if (get().loadGeneration === loadGeneration && get().loadedAccountId === loadedAccountId) {
        set({ loading: false });
      }
    }
  },

  loadSessionManagement: async (accountId) => {
    const generation = get().managementGeneration + 1;
    set({
      managementAccountId: accountId,
      managementLoading: true,
      managementGeneration: generation,
      storageSummary: null,
    });
    try {
      const summary = await api.getSessionStorageSummary(accountId);
      if (get().managementGeneration !== generation || get().managementAccountId !== accountId)
        return;
      set({
        storageSummary: summary,
      });
    } catch (err) {
      if (get().managementGeneration === generation && get().managementAccountId === accountId) {
        useAppStore.getState().setError(formatError(err));
      }
    } finally {
      if (get().managementGeneration === generation && get().managementAccountId === accountId) {
        set({ managementLoading: false });
      }
    }
  },

  selectAllFilteredSessions: async () => {
    const { loadedAccountId, ageFilter, query } = get();
    if (!loadedAccountId || get().mutationRunning) return [];
    try {
      return await api.queryDeletableSessionPaths(loadedAccountId, {
        age: ageFilter,
        query: query.trim() || null,
      });
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
      return [];
    }
  },

  deleteSelectedSessions: async (paths) => {
    const accountId = get().loadedAccountId;
    if (!accountId || !paths.length || get().mutationRunning) return;
    set({ mutationRunning: true });
    try {
      const result = await api.deleteSessions({ profileId: accountId, paths });
      useAppStore
        .getState()
        .setStatus(`Deleted ${result.deletedCount} session${result.deletedCount === 1 ? '' : 's'}`);
      useUsageStore.getState().invalidate();
      await Promise.all([get().loadSessions(accountId), get().loadSessionManagement(accountId)]);
    } catch (err) {
      useAppStore.getState().setError(formatError(err));
    } finally {
      set({ mutationRunning: false });
    }
  },

  clear: () =>
    set((state) => ({
      sessions: [],
      selectedSessionId: '',
      loadedAccountId: '',
      cursor: null,
      hasMore: false,
      loading: false,
      loadGeneration: state.loadGeneration + 1,
      sort: 'newest',
      ageFilter: 'all',
      managementAccountId: '',
      managementLoading: false,
      managementGeneration: state.managementGeneration + 1,
      mutationRunning: false,
      storageSummary: null,
    })),

  previewResume: async (session) => {
    const target = session ?? get().selectedSession();
    if (!target) return;
    const command = await api.buildResumeCommand({
      profileId: target.accountId,
      sessionId: target.id,
      cwd: target.cwd,
    });
    set({ resume: command });
  },

  copyResume: async (session) => {
    const target = session ?? get().selectedSession();
    if (!target) return;
    const command = await api.buildResumeCommand({
      profileId: target.accountId,
      sessionId: target.id,
      cwd: target.cwd,
    });
    await navigator.clipboard.writeText(command.command);
    set({ resume: command });
    useAppStore.getState().setStatus('Resume command copied');
  },

  openResume: async (session) => {
    const target = session ?? get().selectedSession();
    if (!target) return;
    try {
      await api.openTerminalWithResume({
        profileId: target.accountId,
        sessionId: target.id,
        cwd: target.cwd,
      });
      useAppStore.getState().setStatus('Terminal resume opened');
    } catch (err) {
      useAppStore.getState().setError(`${formatError(err)}. Copy command fallback is available.`);
      await get().previewResume(target);
    }
  },

  openSessionDetails: async (session) => {
    const target = session ?? get().selectedSession();
    if (!target) return;
    set({ selectedSessionId: target.id });
    await get().previewResume(target);
    useAppStore.getState().openModal('sessionDetail');
  },
}));
