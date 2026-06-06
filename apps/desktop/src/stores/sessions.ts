import { create } from "zustand";
import * as api from "../lib/api";
import type { CodexSession, ResumeCommand } from "../lib/types";
import { useAppStore } from "./app";
import { formatError } from "../lib/format";

interface SessionState {
  sessions: CodexSession[];
  selectedSessionId: string;
  query: string;
  resume: ResumeCommand | null;

  selectedSession: () => CodexSession | undefined;
  filteredSessions: () => CodexSession[];
  setSelectedSessionId: (id: string) => void;
  setQuery: (q: string) => void;
  setResume: (r: ResumeCommand | null) => void;
  loadSessions: (accountId: string) => void;
  clear: () => void;
  previewResume: (session?: CodexSession) => Promise<void>;
  copyResume: (session?: CodexSession) => Promise<void>;
  openResume: (session?: CodexSession) => Promise<void>;
  openSessionDetails: (session?: CodexSession) => Promise<void>;
}

export const useSessionStore = create<SessionState>()((set, get) => ({
  sessions: [],
  selectedSessionId: "",
  query: "",
  resume: null,

  selectedSession: () => {
    const { sessions, selectedSessionId } = get();
    return sessions.find((s) => s.id === selectedSessionId) ?? sessions[0];
  },

  filteredSessions: () => {
    const { sessions, query } = get();
    const needle = query.trim().toLowerCase();
    if (!needle) return sessions;
    return sessions.filter((s) =>
      [s.id, s.cwd, s.summary, s.path, s.model]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
        .includes(needle),
    );
  },

  setSelectedSessionId: (id) => set({ selectedSessionId: id }),
  setQuery: (query) => set({ query }),
  setResume: (resume) => set({ resume }),

  loadSessions: (accountId) => {
    api
      .listSessions(accountId)
      .then((items) => set({ sessions: items, selectedSessionId: items[0]?.id ?? "" }))
      .catch((err) => useAppStore.getState().setError(formatError(err)));
  },

  clear: () => set({ sessions: [], selectedSessionId: "" }),

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
    useAppStore.getState().setStatus("Resume command copied");
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
      useAppStore.getState().setStatus("Terminal resume opened");
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
    useAppStore.getState().openModal("sessionDetail");
  },
}));
