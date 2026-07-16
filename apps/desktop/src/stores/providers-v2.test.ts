import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useProviderStore } from './providers';
import { useAccountStore } from './accounts';
import { useAppStore } from './app';
import * as api from '../lib/api';
import type {
  ProfileAttachPlanViewV2,
  ProfileDetachPlanViewV2,
  ProviderDefinitionV2,
  ProviderProfileViewV2,
} from '../lib/types';

vi.mock('../lib/api', () => ({
  inTauri: vi.fn(() => true),
  listProvidersV2: vi.fn(),
  listProfileProviderBindingsV2: vi.fn(),
  createProviderV2: vi.fn(),
  updateProviderV2: vi.fn(),
  rotateProviderCredentialV2: vi.fn(),
  planAttachProviderV2: vi.fn(),
  executeAttachProviderV2: vi.fn(),
  planDetachProviderV2: vi.fn(),
  executeDetachProviderV2: vi.fn(),
}));

const definition: ProviderDefinitionV2 = {
  id: 'company',
  name: 'Company',
  protocol: 'responses',
  baseUrl: 'https://company.example.test/v1',
  defaultModel: 'model-a',
  models: [{ id: 'model-a', label: 'Model A' }],
  upstreamAuth: {
    kind: 'bearer',
    credential: { kind: 'env', envKey: 'COMPANY_TOKEN' },
  },
  adapter: { kind: 'none' },
  codex: { queryParams: {}, envHttpHeaders: {} },
};

const provider: ProviderProfileViewV2 = {
  ...definition,
  storeRevision: 4,
  usedBy: [],
  readinessBlockers: [],
};

const attachPlan: ProfileAttachPlanViewV2 = {
  planId: 'attach-1',
  fingerprint: 'fp-attach',
  expiresAtMs: Date.now() + 60_000,
  profileId: 'profile-a',
  providerId: 'company',
  selectedModel: 'model-a',
  routeKind: 'direct',
  blockers: [],
  warnings: [],
  operations: ['backup /tmp/config.toml.bak', 'write managed provider projection'],
  redactedPreview: '[model_providers.company]\nenv_key = "COMPANY_TOKEN"',
  expectedProviderStoreRevision: 4,
  expectedBindingStoreRevision: 0,
  expectedBindingRevision: null,
  sourceConfigHash: 'hash',
};

const detachPlan: ProfileDetachPlanViewV2 = {
  planId: 'detach-1',
  fingerprint: 'fp-detach',
  expiresAtMs: Date.now() + 60_000,
  profileId: 'profile-a',
  expectedBindingStoreRevision: 1,
  expectedBindingRevision: 1,
  sourceConfigHash: 'hash',
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.inTauri).mockReturnValue(true);
  useProviderStore.setState({
    providers: [],
    bindings: [],
    attachPlan: null,
    detachPlan: null,
    loading: false,
    recoveryMessage: '',
  });
  useAppStore.setState({ status: 'Ready', error: '' });
  vi.mocked(api.listProvidersV2).mockResolvedValue([provider]);
  vi.mocked(api.listProfileProviderBindingsV2).mockResolvedValue([]);
  vi.mocked(api.createProviderV2).mockResolvedValue(provider);
  vi.mocked(api.updateProviderV2).mockResolvedValue(provider);
  vi.mocked(api.planAttachProviderV2).mockResolvedValue(attachPlan);
  vi.mocked(api.executeAttachProviderV2).mockResolvedValue({
    operationId: 'op-1',
    state: 'completed',
    idempotent: false,
  });
  vi.mocked(api.planDetachProviderV2).mockResolvedValue(detachPlan);
  vi.mocked(api.executeDetachProviderV2).mockResolvedValue({
    operationId: 'op-2',
    state: 'completed',
    idempotent: false,
  });
});

describe('useProviderStore V2', () => {
  it('does not enter the Tauri SDK while the native bridge is unavailable', async () => {
    vi.mocked(api.inTauri).mockReturnValue(false);
    useProviderStore.setState({ providers: [provider], loading: true });

    await useProviderStore.getState().refresh();

    expect(api.listProvidersV2).not.toHaveBeenCalled();
    expect(api.listProfileProviderBindingsV2).not.toHaveBeenCalled();
    expect(useProviderStore.getState()).toMatchObject({
      providers: [],
      bindings: [],
      loading: false,
    });
  });

  it('refreshes providers and bindings as one visible snapshot', async () => {
    await useProviderStore.getState().refresh();

    expect(api.listProvidersV2).toHaveBeenCalledOnce();
    expect(api.listProfileProviderBindingsV2).toHaveBeenCalledOnce();
    expect(useProviderStore.getState().providers).toEqual([provider]);
    expect(useProviderStore.getState().bindings).toEqual([]);
  });

  it('creates and updates against the visible store revision then refreshes', async () => {
    useProviderStore.setState({ providers: [provider] });

    await useProviderStore.getState().saveProvider(definition, false);
    expect(api.createProviderV2).toHaveBeenCalledWith({
      expectedRevision: 4,
      provider: definition,
    });

    await useProviderStore.getState().saveProvider(definition, true);
    expect(api.updateProviderV2).toHaveBeenCalledWith({
      expectedRevision: 4,
      provider: definition,
    });
    expect(api.listProvidersV2).toHaveBeenCalledTimes(2);
  });

  it('executes only the issued attach ticket and refreshes providers, bindings, and accounts', async () => {
    const accountRefresh = vi.spyOn(useAccountStore.getState(), 'refresh').mockResolvedValue();

    await useProviderStore.getState().previewAttach({
      profileId: 'profile-a',
      providerId: 'company',
      selectedModel: 'model-a',
    });
    await useProviderStore.getState().executeAttach();

    expect(api.executeAttachProviderV2).toHaveBeenCalledWith({
      planId: 'attach-1',
      fingerprint: 'fp-attach',
    });
    expect(accountRefresh).toHaveBeenCalledOnce();
    expect(useProviderStore.getState().attachPlan).toBeNull();
  });

  it('previews and executes detach with its matching ticket', async () => {
    const accountRefresh = vi.spyOn(useAccountStore.getState(), 'refresh').mockResolvedValue();

    await useProviderStore.getState().previewDetach('profile-a');
    await useProviderStore.getState().executeDetach();

    expect(api.executeDetachProviderV2).toHaveBeenCalledWith({
      planId: 'detach-1',
      fingerprint: 'fp-detach',
    });
    expect(accountRefresh).toHaveBeenCalledOnce();
    expect(useProviderStore.getState().detachPlan).toBeNull();
  });

  it.each(['ATTACH_PLAN_STALE', 'STORE_REVISION_CONFLICT', 'PROFILE_BINDING_CONFLICT'])(
    'clears previews and refreshes after recoverable %s',
    async (code) => {
      useProviderStore.setState({ attachPlan, detachPlan });
      vi.mocked(api.executeAttachProviderV2).mockRejectedValue({
        code,
        message: 'redacted',
        recoverable: true,
        recoveryActions: ['refresh', 'plan_again'],
      });

      await expect(useProviderStore.getState().executeAttach()).rejects.toMatchObject({ code });

      expect(useProviderStore.getState().attachPlan).toBeNull();
      expect(useProviderStore.getState().detachPlan).toBeNull();
      expect(useProviderStore.getState().recoveryMessage).toContain('preview again');
      expect(api.listProvidersV2).toHaveBeenCalledOnce();
    },
  );

  it('refuses attach execution when no preview exists', async () => {
    await expect(useProviderStore.getState().executeAttach()).rejects.toThrow(
      'Preview is required',
    );
    expect(api.executeAttachProviderV2).not.toHaveBeenCalled();
  });
});
