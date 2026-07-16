import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import * as api from '../lib/api';
import type {
  AccountNoteUpdate,
  CodexAccount,
  CodexSession,
  DivergedSessionStrategy,
} from '../lib/types';
import { useAppStore } from './app';
import { useSessionStore } from './sessions';
import { useQuotaStore } from './quota';
import { useProviderStore } from './providers';
import { formatError } from '../lib/format';
import { quotaRefreshProfileIds } from '../lib/quota';

const DIVERGED_KEY = 'lam-diverged-session-strategy';

function isCompatibilityConfirmationRequired(error: unknown): boolean {
  return Boolean(
    error &&
    typeof error === 'object' &&
    'code' in error &&
    (error as { code?: unknown }).code === 'RELAY_COMPATIBILITY_CONFIRMATION_REQUIRED',
  );
}

function readDivergedStrategy(): DivergedSessionStrategy {
  const saved = localStorage.getItem(DIVERGED_KEY);
  if (
    saved === 'stop_and_ask' ||
    saved === 'summarize_fork_with_target_account' ||
    saved === 'timeline_merge_to_fork' ||
    saved === 'prefer_source' ||
    saved === 'prefer_target'
  )
    return saved;
  return 'summarize_fork_with_target_account';
}

interface AccountState {
  accounts: CodexAccount[];
  selectedAccountId: string;
  activeSession: CodexSession | undefined;
  divergedStrategy: DivergedSessionStrategy;
  refreshing: boolean;

  selectedAccount: () => CodexAccount | undefined;
  setSelectedAccountId: (id: string) => void;
  setDivergedStrategy: (strategy: DivergedSessionStrategy) => void;
  refresh: (options?: { refreshQuotasNow?: boolean }) => Promise<void>;
  refreshActiveSession: (accounts?: CodexAccount[]) => Promise<void>;
  relayResumeTo: (account: CodexAccount) => Promise<boolean>;
  relaySessionTo: (session: CodexSession, account: CodexAccount) => Promise<boolean>;
  login: (account?: CodexAccount) => Promise<void>;
  saveAccountNote: (req: AccountNoteUpdate) => Promise<void>;
}

export const useAccountStore = create<AccountState>()(
  subscribeWithSelector((set, get) => ({
    accounts: [],
    selectedAccountId: '',
    activeSession: undefined,
    divergedStrategy: readDivergedStrategy(),
    refreshing: false,

    selectedAccount: () => {
      const { accounts, selectedAccountId } = get();
      return accounts.find((a) => a.id === selectedAccountId) ?? accounts[0];
    },

    setSelectedAccountId: (id) => set({ selectedAccountId: id }),

    setDivergedStrategy: (strategy) => {
      localStorage.setItem(DIVERGED_KEY, strategy);
      set({ divergedStrategy: strategy });
    },

    refresh: async (options) => {
      const app = useAppStore.getState();
      app.clearError();
      set({ refreshing: true });

      try {
        const providerRefresh = useProviderStore
          .getState()
          .refresh()
          .catch((e) => app.setError(formatError(e)));
        if (api.inTauri()) {
          try {
            const cached = await api.listCachedAccounts();
            if (cached.length) {
              applyAccountsList(cached, set, get, true, false);
            }
          } catch {
            /* cache miss is fine */
          }
        }

        api
          .healthCheck()
          .then(app.setHealth)
          .catch((e) => app.setError(formatError(e)));
        const accountData = await api.listAccounts();
        await providerRefresh;
        applyAccountsList(accountData, set, get, false, !options?.refreshQuotasNow);
        if (options?.refreshQuotasNow && accountData.length) {
          await useQuotaStore.getState().refreshQuotas(refreshableProfileIds(accountData));
        }
      } catch (err) {
        app.setAppReady();
        app.setError(formatError(err));
      } finally {
        set({ refreshing: false });
      }
    },

    refreshActiveSession: async (accountData) => {
      const accts = accountData ?? get().accounts;
      if (!accts.length) {
        set({ activeSession: undefined });
        return;
      }
      const results = await Promise.allSettled(accts.map((a) => api.listSessions(a.id)));
      const all = results.flatMap((r) => (r.status === 'fulfilled' ? r.value : []));
      const latest = all.sort((a, b) => b.modifiedAt - a.modifiedAt)[0];
      set({ activeSession: latest });
    },

    relayResumeTo: async (account) => {
      const { activeSession, accounts } = get();
      const app = useAppStore.getState();
      if (!activeSession) {
        app.setError('No active source session found for Resume Here.');
        return false;
      }
      const success = await get().relaySessionTo(activeSession, account);
      if (success) get().refreshActiveSession(accounts);
      return success;
    },

    relaySessionTo: async (session, account) => {
      const { divergedStrategy } = get();
      const app = useAppStore.getState();
      try {
        if (account.id === session.accountId) {
          await useSessionStore.getState().openResume(session);
          return true;
        }
        const request = {
          fromProfileId: session.accountId,
          toProfileId: account.id,
          sessionId: session.id,
          cwd: session.cwd,
          divergedStrategy,
        };
        let result;
        try {
          result = await api.relayResumeSession(request);
        } catch (error) {
          if (
            !isCompatibilityConfirmationRequired(error) ||
            !window.confirm(
              'This handoff only drops display metadata. Continue with the target Provider?',
            )
          )
            throw error;
          result = await api.relayResumeSession({
            ...request,
            confirmCompatibilityLoss: true,
          });
        }
        set({ selectedAccountId: account.id });
        useSessionStore.getState().setSelectedSessionId(session.id);
        useSessionStore.getState().setResume(result.resume);
        await api.openTerminalWithCommand(result.resume.command);
        const actionLabel = result.action === 'already_current' ? 'already current' : result.action;
        app.setStatus(`Handoff ${actionLabel}: ${session.id} on ${account.id}`);
        if (result.warnings.length) app.setError(result.warnings.join(' '));
        return true;
      } catch (err) {
        app.setError(`${formatError(err)}. Existing session was not overwritten.`);
        return false;
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
        useAppStore
          .getState()
          .setError(`${formatError(err)}. Copy login command fallback is available.`);
      }
    },

    saveAccountNote: async (req) => {
      const app = useAppStore.getState();
      try {
        const updated = await api.updateAccountNote(req);
        set({
          accounts: get().accounts.map((account) =>
            account.id === updated.id ? updated : account,
          ),
        });
        app.setStatus(`Updated note for ${updated.displayName}`);
      } catch (err) {
        app.setError(formatError(err));
      }
    },
  })),
);

function applyAccountsList(
  data: CodexAccount[],
  set: (partial: Partial<AccountState>) => void,
  get: () => AccountState,
  fromCache: boolean,
  scheduleQuotaRefresh = true,
) {
  const app = useAppStore.getState();
  const keepSelection =
    get().selectedAccountId && data.some((a) => a.id === get().selectedAccountId);
  const nextAccount = keepSelection ? get().selectedAccountId : (data[0]?.id ?? '');

  const quotaProfileIds = refreshableProfileIds(data);
  set({ accounts: data, selectedAccountId: nextAccount });
  useQuotaStore.getState().filterToProfileIds(quotaProfileIds);
  app.setAppReady();

  if (nextAccount) {
    useSessionStore.getState().loadSessions(nextAccount);
  } else {
    useSessionStore.getState().clear();
  }

  if (data.length) {
    get().refreshActiveSession(data);
    useQuotaStore.getState().loadCachedQuotas(quotaProfileIds);
    if (scheduleQuotaRefresh && quotaProfileIds.length) {
      useQuotaStore.getState().scheduleQuotaRefresh(quotaProfileIds, 8_000);
    }
  } else {
    useQuotaStore.getState().clearQuotas();
  }

  app.setStatus(
    fromCache ? `Cached ${data.length} accounts · scanning…` : `Loaded ${data.length} accounts`,
  );
}

function refreshableProfileIds(accounts: CodexAccount[]): string[] {
  const bindings = useProviderStore.getState().bindings;
  return quotaRefreshProfileIds(
    accounts,
    bindings.map((binding) => binding.profileId),
  );
}
