import { create } from 'zustand';
import * as api from '../lib/api';
import type {
  CredentialReferenceV2,
  ApiAccountConnectionViewV2,
  UpdateApiAccountConnectionRequestV2,
  PlanAttachRequestV2,
  ProfileAttachPlanViewV2,
  ProfileDetachPlanViewV2,
  ProfileProviderBindingViewV2,
  ProviderDefinitionV2,
  ProviderProfileViewV2,
  StructuredErrorViewV2,
} from '../lib/types';
import { useAppStore } from './app';
import { useAccountStore } from './accounts';
import { formatError } from '../lib/format';

interface ProviderState {
  providers: ProviderProfileViewV2[];
  providerStoreRevision: number | null;
  bindings: ProfileProviderBindingViewV2[];
  attachPlan: ProfileAttachPlanViewV2 | null;
  detachPlan: ProfileDetachPlanViewV2 | null;
  loading: boolean;
  recoveryMessage: string;
  apiAccountConnection: ApiAccountConnectionViewV2 | null;
  refreshGeneration: number;
  refresh: () => Promise<void>;
  saveProvider: (provider: ProviderDefinitionV2, editing: boolean) => Promise<void>;
  approveAuthCommand: (executable: string, args: string[]) => Promise<string>;
  createKeychainProvider: (provider: ProviderDefinitionV2, secret: string) => Promise<void>;
  rotateKeychainCredential: (
    providerId: string,
    expectedCredential: CredentialReferenceV2,
    secret: string,
  ) => Promise<void>;
  testProvider: (providerId: string) => Promise<void>;
  previewAttach: (req: PlanAttachRequestV2) => Promise<ProfileAttachPlanViewV2>;
  executeAttach: () => Promise<void>;
  previewDetach: (profileId: string) => Promise<ProfileDetachPlanViewV2>;
  executeDetach: () => Promise<void>;
  clearPlans: () => void;
  loadApiAccountConnection: (profileId: string) => Promise<ApiAccountConnectionViewV2>;
  updateApiAccountConnection: (
    request: UpdateApiAccountConnectionRequestV2,
  ) => Promise<ApiAccountConnectionViewV2>;
  clearApiAccountConnection: () => void;
}

function requiredProviderStoreRevision(state: ProviderState): number {
  if (state.providerStoreRevision == null) {
    throw new Error('Provider state has not been loaded');
  }
  return state.providerStoreRevision;
}

const conflictCodes = new Set([
  'ATTACH_PLAN_STALE',
  'ATTACH_PLAN_EXPIRED',
  'STORE_REVISION_CONFLICT',
  'PROFILE_BINDING_CONFLICT',
  'CODEX_CONFIG_OWNERSHIP_CONFLICT',
]);

function structuredError(error: unknown): StructuredErrorViewV2 | null {
  if (!error || typeof error !== 'object' || !('code' in error)) return null;
  const candidate = error as Partial<StructuredErrorViewV2>;
  if (typeof candidate.code !== 'string') return null;
  return {
    code: candidate.code,
    message: typeof candidate.message === 'string' ? candidate.message : 'Operation failed',
    recoverable: Boolean(candidate.recoverable),
    recoveryActions: Array.isArray(candidate.recoveryActions)
      ? candidate.recoveryActions.map(String)
      : [],
  };
}

async function recoverProviderConflict(
  error: unknown,
  clearStaleState: () => void,
  refresh: () => Promise<void>,
): Promise<void> {
  const detail = structuredError(error);
  if (!detail || !conflictCodes.has(detail.code)) return;
  clearStaleState();
  await refresh().catch(() => undefined);
}

export const useProviderStore = create<ProviderState>()((set, get) => ({
  providers: [],
  providerStoreRevision: null,
  bindings: [],
  attachPlan: null,
  detachPlan: null,
  loading: false,
  recoveryMessage: '',
  apiAccountConnection: null,
  refreshGeneration: 0,

  refresh: async () => {
    const generation = get().refreshGeneration + 1;
    if (!api.inTauri()) {
      set({
        providers: [],
        providerStoreRevision: null,
        bindings: [],
        loading: false,
        refreshGeneration: generation,
      });
      return;
    }
    set({ loading: true, refreshGeneration: generation });
    try {
      const [providerList, bindings] = await Promise.all([
        api.listProvidersV2(),
        api.listProfileProviderBindingsV2(),
      ]);
      if (get().refreshGeneration === generation) {
        set({
          providers: providerList.providers,
          providerStoreRevision: providerList.revision,
          bindings,
        });
      }
    } catch (error) {
      if (get().refreshGeneration === generation) {
        useAppStore.getState().setError(formatError(error));
      }
      throw error;
    } finally {
      if (get().refreshGeneration === generation) {
        set({ loading: false });
      }
    }
  },

  saveProvider: async (provider, editing) => {
    const expectedRevision = requiredProviderStoreRevision(get());
    try {
      const request = { expectedRevision, provider };
      if (editing) await api.updateProviderV2(request);
      else await api.createProviderV2(request);
      await get().refresh();
      useAppStore
        .getState()
        .setStatus(`Provider ${provider.id} ${editing ? 'updated' : 'created'}`);
    } catch (error) {
      await recoverProviderConflict(
        error,
        () =>
          set({
            attachPlan: null,
            detachPlan: null,
            recoveryMessage: 'Provider state changed. Refresh completed; preview again.',
          }),
        get().refresh,
      );
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  approveAuthCommand: async (executable, args) => {
    try {
      const current = await api.listProviderAuthCommandApprovalsV2();
      const approval = await api.approveProviderAuthCommandV2({
        expectedRevision: current.revision,
        executable,
        args,
        timeoutMs: 5_000,
        maxStdoutBytes: 8_192,
        refreshIntervalMs: 0,
      });
      return approval.approvalId;
    } catch (error) {
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  createKeychainProvider: async (provider, secret) => {
    const expectedRevision = requiredProviderStoreRevision(get());
    try {
      await api.createProviderWithKeychainV2({ expectedRevision, provider, secret });
      await get().refresh();
      useAppStore.getState().setStatus(`Provider ${provider.id} created`);
    } catch (error) {
      await recoverProviderConflict(
        error,
        () =>
          set({
            attachPlan: null,
            detachPlan: null,
            recoveryMessage: 'Provider state changed. Refresh completed; preview again.',
          }),
        get().refresh,
      );
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  rotateKeychainCredential: async (providerId, expectedCredential, secret) => {
    const expectedRevision = requiredProviderStoreRevision(get());
    try {
      await api.rotateProviderCredentialV2({
        expectedRevision,
        providerId,
        expectedCredential,
        secret,
      });
      await get().refresh();
      useAppStore.getState().setStatus(`Provider ${providerId} credential rotated`);
    } catch (error) {
      await recoverProviderConflict(
        error,
        () =>
          set({
            attachPlan: null,
            detachPlan: null,
            recoveryMessage: 'Provider state changed. Refresh completed; preview again.',
          }),
        get().refresh,
      );
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  testProvider: async (providerId) => {
    try {
      const result = await api.testProviderUpstreamV2(providerId);
      useAppStore.getState().setStatus(result.redactedSummary);
    } catch (error) {
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  previewAttach: async (req) => {
    set({ recoveryMessage: '', detachPlan: null });
    const attachPlan = await api.planAttachProviderV2(req);
    set({ attachPlan });
    return attachPlan;
  },

  executeAttach: async () => {
    const plan = get().attachPlan;
    if (!plan) throw new Error('Preview is required before attach');
    try {
      await api.executeAttachProviderV2({
        planId: plan.planId,
        fingerprint: plan.fingerprint,
      });
      set({ attachPlan: null, recoveryMessage: '' });
      await get().refresh();
      await useAccountStore.getState().refresh();
      useAppStore
        .getState()
        .setStatus(
          `${plan.expectedBindingRevision == null ? 'Attached' : 'Rebound'} ${plan.providerId} to ${plan.profileId}`,
        );
    } catch (error) {
      const detail = structuredError(error);
      if (detail && conflictCodes.has(detail.code)) {
        set({
          attachPlan: null,
          detachPlan: null,
          recoveryMessage: 'Provider or profile changed. Refresh completed; preview again.',
        });
        await get()
          .refresh()
          .catch(() => undefined);
      }
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  previewDetach: async (profileId) => {
    set({ recoveryMessage: '', attachPlan: null });
    const detachPlan = await api.planDetachProviderV2(profileId);
    set({ detachPlan });
    return detachPlan;
  },

  executeDetach: async () => {
    const plan = get().detachPlan;
    if (!plan) throw new Error('Preview is required before detach');
    try {
      await api.executeDetachProviderV2({
        planId: plan.planId,
        fingerprint: plan.fingerprint,
      });
      set({ detachPlan: null, recoveryMessage: '' });
      await get().refresh();
      await useAccountStore.getState().refresh();
      useAppStore.getState().setStatus(`Detached Provider from ${plan.profileId}`);
    } catch (error) {
      const detail = structuredError(error);
      if (detail && conflictCodes.has(detail.code)) {
        set({
          attachPlan: null,
          detachPlan: null,
          recoveryMessage: 'Provider or profile changed. Refresh completed; preview again.',
        });
        await get()
          .refresh()
          .catch(() => undefined);
      }
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  loadApiAccountConnection: async (profileId) => {
    try {
      const connection = await api.getApiAccountConnectionV2(profileId);
      set({ apiAccountConnection: connection });
      return connection;
    } catch (error) {
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  updateApiAccountConnection: async (request) => {
    try {
      const connection = await api.updateApiAccountConnectionV2(request);
      set({ apiAccountConnection: connection });
      await get().refresh();
      useAppStore.getState().setStatus(`Updated API account ${connection.profileId}`);
      return connection;
    } catch (error) {
      await recoverProviderConflict(
        error,
        () => set({ apiAccountConnection: null }),
        get().refresh,
      );
      useAppStore.getState().setError(formatError(error));
      throw error;
    }
  },

  clearApiAccountConnection: () => set({ apiAccountConnection: null }),

  clearPlans: () => set({ attachPlan: null, detachPlan: null, recoveryMessage: '' }),
}));
