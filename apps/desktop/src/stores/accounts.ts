import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import * as api from "../lib/api";
import type {
  CodexAccount,
  CodexSession,
  DivergedSessionStrategy,
} from "../lib/types";
import { useAppStore } from "./app";
import { useSessionStore } from "./sessions";
import { useQuotaStore } from "./quota";
import { useProviderStore } from "./providers";
import { formatError } from "../lib/format";

const DIVERGED_KEY = "lam-diverged-session-strategy";

function readDivergedStrategy(): DivergedSessionStrategy {
  const saved = localStorage.getItem(DIVERGED_KEY);
  if (
    saved === "stop_and_ask" ||
    saved === "summarize_fork_with_target_account" ||
    saved === "timeline_merge_to_fork" ||
    saved === "prefer_source" ||
    saved === "prefer_target"
  )
    return saved;
  return "summarize_fork_with_target_account";
}

interface AccountState {
  accounts: CodexAccount[];
  selectedAccountId: string;
  activeSession: CodexSession | undefined;
  divergedStrategy: DivergedSessionStrategy;

  selectedAccount: () => CodexAccount | undefined;
  setSelectedAccountId: (id: string) => void;
  setDivergedStrategy: (strategy: DivergedSessionStrategy) => void;
  refresh: () => Promise<void>;
  refreshActiveSession: (accounts?: CodexAccount[]) => Promise<void>;
  relayResumeTo: (account: CodexAccount) => Promise<void>;
  login: (account?: CodexAccount) => Promise<void>;
}

export const useAccountStore = create<AccountState>()(
  subscribeWithSelector((set, get) => ({
    accounts: [],
    selectedAccountId: "",
    activeSession: undefined,
    divergedStrategy: readDivergedStrategy(),

    selectedAccount: () => {
      const { accounts, selectedAccountId } = get();
      return accounts.find((a) => a.id === selectedAccountId) ?? accounts[0];
    },

    setSelectedAccountId: (id) => set({ selectedAccountId: id }),

    setDivergedStrategy: (strategy) => {
      localStorage.setItem(DIVERGED_KEY, strategy);
      set({ divergedStrategy: strategy });
    },

    refresh: async () => {
      const app = useAppStore.getState();
      app.clearError();

      if (api.inTauri()) {
        try {
          const cached = await api.listCachedAccounts();
          if (cached.length) {
            applyAccountsList(cached, set, get, true);
          }
        } catch {
          /* cache miss is fine */
        }
      }

      try {
        api.healthCheck().then(app.setHealth).catch((e) => app.setError(formatError(e)));
        const accountData = await api.listAccounts();
        applyAccountsList(accountData, set, get, false);
        api.listProviders().then((p) => useProviderStore.getState().setProviders(p)).catch((e) => app.setError(formatError(e)));
      } catch (err) {
        app.setAppReady();
        app.setError(formatError(err));
      }
    },

    refreshActiveSession: async (accountData) => {
      const accts = accountData ?? get().accounts;
      if (!accts.length) {
        set({ activeSession: undefined });
        return;
      }
      const results = await Promise.allSettled(accts.map((a) => api.listSessions(a.id)));
      const all = results.flatMap((r) => (r.status === "fulfilled" ? r.value : []));
      const latest = all.sort((a, b) => b.modifiedAt - a.modifiedAt)[0];
      set({ activeSession: latest });
    },

    relayResumeTo: async (account) => {
      const { activeSession, divergedStrategy, accounts } = get();
      const app = useAppStore.getState();
      if (!activeSession) {
        app.setError("No active source session found for Resume Here.");
        return;
      }
      try {
        if (account.id === activeSession.accountId) {
          await useSessionStore.getState().openResume(activeSession);
          return;
        }
        const result = await api.relayResumeSession({
          fromProfileId: activeSession.accountId,
          toProfileId: account.id,
          sessionId: activeSession.id,
          cwd: activeSession.cwd,
          divergedStrategy,
        });
        set({ selectedAccountId: account.id });
        useSessionStore.getState().setSelectedSessionId(activeSession.id);
        useSessionStore.getState().setResume(result.resume);
        await api.openTerminalWithCommand(result.resume.command);
        const actionLabel = result.action === "already_current" ? "already current" : result.action;
        app.setStatus(`Resume Here ${actionLabel}: ${activeSession.id} on ${account.id}`);
        get().refreshActiveSession(accounts);
        if (result.warnings.length) app.setError(result.warnings.join(" "));
      } catch (err) {
        app.setError(`${formatError(err)}. Existing session was not overwritten.`);
      }
    },

    login: async (account) => {
      const target = account ?? get().selectedAccount();
      if (!target) return;
      try {
        await api.openTerminalForLogin(target.id);
      } catch (err) {
        const command = await api.buildLoginCommand(target.id);
        useSessionStore.getState().setResume(command);
        useAppStore.getState().setError(`${formatError(err)}. Copy login command fallback is available.`);
      }
    },
  })),
);

function applyAccountsList(
  data: CodexAccount[],
  set: (partial: Partial<AccountState>) => void,
  get: () => AccountState,
  fromCache: boolean,
) {
  const app = useAppStore.getState();
  const keepSelection = get().selectedAccountId && data.some((a) => a.id === get().selectedAccountId);
  const nextAccount = keepSelection ? get().selectedAccountId : data[0]?.id ?? "";

  set({ accounts: data, selectedAccountId: nextAccount });
  useQuotaStore.getState().filterToProfileIds(data.map((a) => a.id));
  app.setAppReady();

  if (nextAccount) {
    useSessionStore.getState().loadSessions(nextAccount);
  } else {
    useSessionStore.getState().clear();
  }

  if (data.length) {
    get().refreshActiveSession(data);
    useQuotaStore.getState().loadCachedQuotas(data.map((a) => a.id));
    useQuotaStore.getState().scheduleQuotaRefresh(data.map((a) => a.id), 8_000);
  } else {
    useQuotaStore.getState().clearQuotas();
  }

  app.setStatus(fromCache ? `Cached ${data.length} accounts · scanning…` : `Loaded ${data.length} accounts`);
}
