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
  createProviderWithKeychainV2: vi.fn(),
  updateProviderV2: vi.fn(),
  deleteProviderV2: vi.fn(),
  rotateProviderCredentialV2: vi.fn(),
  planAttachProviderV2: vi.fn(),
  executeAttachProviderV2: vi.fn(),
  planDetachProviderV2: vi.fn(),
  executeDetachProviderV2: vi.fn(),
  getApiAccountConnectionV2: vi.fn(),
  updateApiAccountConnectionV2: vi.fn(),
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.inTauri).mockReturnValue(true);
  useProviderStore.setState({
    providers: [],
    providerStoreRevision: null,
    bindings: [],
    attachPlan: null,
    detachPlan: null,
    loading: false,
    recoveryMessage: '',
  });
  useAppStore.setState({ status: 'Ready', error: '' });
  vi.mocked(api.listProvidersV2).mockResolvedValue({ revision: 4, providers: [provider] });
  vi.mocked(api.listProfileProviderBindingsV2).mockResolvedValue([]);
  vi.mocked(api.createProviderV2).mockResolvedValue(provider);
  vi.mocked(api.createProviderWithKeychainV2).mockResolvedValue(provider);
  vi.mocked(api.updateProviderV2).mockResolvedValue(provider);
  vi.mocked(api.deleteProviderV2).mockResolvedValue({ providerId: 'company', storeRevision: 5 });
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
  vi.mocked(api.getApiAccountConnectionV2).mockResolvedValue({
    profileId: 'work-api',
    providerId: 'account-work-api',
    protocol: 'responses',
    baseUrl: 'https://api.example.test/v1',
    selectedModel: 'model-a',
    providerStoreRevision: 4,
    apiKeyConfigured: true,
    models: [{ id: 'model-a', label: 'Model A' }],
  });
  vi.mocked(api.updateApiAccountConnectionV2).mockResolvedValue({
    profileId: 'work-api',
    providerId: 'account-work-api',
    protocol: 'responses',
    baseUrl: 'https://new.example.test/v1',
    selectedModel: 'model-a',
    providerStoreRevision: 5,
    apiKeyConfigured: true,
    models: [{ id: 'model-a', label: 'Model A' }],
  });
});

describe('useProviderStore V2', () => {
  it('loads and updates a redacted API account connection then refreshes', async () => {
    const detail = await useProviderStore.getState().loadApiAccountConnection('work-api');
    expect(detail.apiKeyConfigured).toBe(true);
    await useProviderStore.getState().updateApiAccountConnection({
      profileId: 'work-api',
      expectedProviderStoreRevision: 4,
      baseUrl: 'https://new.example.test/v1',
      apiKey: 'sk-write-only',
    });
    expect(api.updateApiAccountConnectionV2).toHaveBeenCalledWith({
      profileId: 'work-api',
      expectedProviderStoreRevision: 4,
      baseUrl: 'https://new.example.test/v1',
      apiKey: 'sk-write-only',
    });
    expect(api.listProvidersV2).toHaveBeenCalledOnce();
    expect(useProviderStore.getState().apiAccountConnection?.providerStoreRevision).toBe(5);
  });

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
    expect(useProviderStore.getState().providerStoreRevision).toBe(4);
    expect(useProviderStore.getState().bindings).toEqual([]);
  });

  it('creates and updates against the visible store revision then refreshes', async () => {
    useProviderStore.setState({ providers: [provider], providerStoreRevision: 4 });

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

  it('uses the explicit revision when the provider list is empty', async () => {
    useProviderStore.setState({ providers: [], providerStoreRevision: 7 });

    await useProviderStore.getState().saveProvider(definition, false);

    expect(api.createProviderV2).toHaveBeenCalledWith({
      expectedRevision: 7,
      provider: definition,
    });
  });

  it('deletes a Provider against the visible revision then refreshes', async () => {
    useProviderStore.setState({ providers: [provider], providerStoreRevision: 4 });

    await useProviderStore.getState().deleteProvider('company');

    expect(api.deleteProviderV2).toHaveBeenCalledWith({
      expectedRevision: 4,
      providerId: 'company',
    });
    expect(api.listProvidersV2).toHaveBeenCalledOnce();
    expect(useAppStore.getState().status).toBe('Provider company deleted');
  });

  it('uses the explicit collection revision for Keychain create and credential rotation', async () => {
    useProviderStore.setState({ providers: [], providerStoreRevision: 7 });

    await useProviderStore.getState().createKeychainProvider(definition, 'write-only-secret');
    expect(api.createProviderWithKeychainV2).toHaveBeenCalledWith({
      expectedRevision: 7,
      provider: definition,
      secret: 'write-only-secret',
    });

    useProviderStore.setState({ providerStoreRevision: 8 });
    await useProviderStore.getState().rotateKeychainCredential(
      'company',
      {
        kind: 'keychain',
        service: 'lam.remote-provider',
        account: 'company',
        version: 1,
      },
      'rotated-secret',
    );
    expect(api.rotateProviderCredentialV2).toHaveBeenCalledWith({
      expectedRevision: 8,
      providerId: 'company',
      expectedCredential: {
        kind: 'keychain',
        service: 'lam.remote-provider',
        account: 'company',
        version: 1,
      },
      secret: 'rotated-secret',
    });
  });

  it('refreshes after a create conflict without replaying the stale write', async () => {
    useProviderStore.setState({ providers: [], providerStoreRevision: 7 });
    vi.mocked(api.createProviderV2).mockRejectedValueOnce({
      code: 'STORE_REVISION_CONFLICT',
      message: 'redacted',
      recoverable: true,
      recoveryActions: ['refresh'],
    });
    vi.mocked(api.listProvidersV2).mockResolvedValueOnce({ revision: 8, providers: [] });

    await expect(useProviderStore.getState().saveProvider(definition, false)).rejects.toMatchObject(
      {
        code: 'STORE_REVISION_CONFLICT',
      },
    );

    expect(api.createProviderV2).toHaveBeenCalledTimes(1);
    expect(useProviderStore.getState().providerStoreRevision).toBe(8);
    expect(useProviderStore.getState().recoveryMessage).toContain('preview again');
  });

  it('refreshes and clears stale plans after a Keychain create conflict without replay', async () => {
    useProviderStore.setState({
      providers: [],
      providerStoreRevision: 7,
      attachPlan,
      detachPlan,
    });
    vi.mocked(api.createProviderWithKeychainV2).mockRejectedValueOnce({
      code: 'STORE_REVISION_CONFLICT',
      message: 'redacted',
      recoverable: true,
      recoveryActions: ['refresh'],
    });
    vi.mocked(api.listProvidersV2).mockResolvedValueOnce({ revision: 8, providers: [] });

    await expect(
      useProviderStore.getState().createKeychainProvider(definition, 'write-only-secret'),
    ).rejects.toMatchObject({ code: 'STORE_REVISION_CONFLICT' });

    expect(api.createProviderWithKeychainV2).toHaveBeenCalledTimes(1);
    expect(api.listProvidersV2).toHaveBeenCalledOnce();
    expect(useProviderStore.getState().providerStoreRevision).toBe(8);
    expect(useProviderStore.getState().attachPlan).toBeNull();
    expect(useProviderStore.getState().detachPlan).toBeNull();
    expect(useProviderStore.getState().recoveryMessage).toContain('preview again');
  });

  it('refreshes and clears stale plans after a credential rotation conflict without replay', async () => {
    const credential = {
      kind: 'keychain' as const,
      service: 'lam.remote-provider' as const,
      account: 'company',
      version: 1,
    };
    useProviderStore.setState({
      providers: [provider],
      providerStoreRevision: 8,
      attachPlan,
      detachPlan,
    });
    vi.mocked(api.rotateProviderCredentialV2).mockRejectedValueOnce({
      code: 'STORE_REVISION_CONFLICT',
      message: 'redacted',
      recoverable: true,
      recoveryActions: ['refresh'],
    });
    vi.mocked(api.listProvidersV2).mockResolvedValueOnce({ revision: 9, providers: [provider] });

    await expect(
      useProviderStore.getState().rotateKeychainCredential('company', credential, 'rotated-secret'),
    ).rejects.toMatchObject({ code: 'STORE_REVISION_CONFLICT' });

    expect(api.rotateProviderCredentialV2).toHaveBeenCalledTimes(1);
    expect(api.listProvidersV2).toHaveBeenCalledOnce();
    expect(useProviderStore.getState().providerStoreRevision).toBe(9);
    expect(useProviderStore.getState().attachPlan).toBeNull();
    expect(useProviderStore.getState().detachPlan).toBeNull();
    expect(useProviderStore.getState().recoveryMessage).toContain('preview again');
  });

  it('does not refresh after a non-conflict Keychain create failure', async () => {
    useProviderStore.setState({ providers: [], providerStoreRevision: 7 });
    vi.mocked(api.createProviderWithKeychainV2).mockRejectedValueOnce(new Error('keychain failed'));

    await expect(
      useProviderStore.getState().createKeychainProvider(definition, 'write-only-secret'),
    ).rejects.toThrow('keychain failed');

    expect(api.createProviderWithKeychainV2).toHaveBeenCalledTimes(1);
    expect(api.listProvidersV2).not.toHaveBeenCalled();
  });

  it('refuses provider writes before the collection revision is loaded', async () => {
    useProviderStore.setState({ providers: [], providerStoreRevision: null });

    await expect(useProviderStore.getState().saveProvider(definition, false)).rejects.toThrow(
      'Provider state has not been loaded',
    );

    expect(api.createProviderV2).not.toHaveBeenCalled();
  });

  it('does not let an older refresh overwrite a newer provider snapshot', async () => {
    const firstProviders = deferred<Awaited<ReturnType<typeof api.listProvidersV2>>>();
    const firstBindings = deferred<Awaited<ReturnType<typeof api.listProfileProviderBindingsV2>>>();
    vi.mocked(api.listProvidersV2)
      .mockImplementationOnce(() => firstProviders.promise)
      .mockResolvedValueOnce({ revision: 8, providers: [] });
    vi.mocked(api.listProfileProviderBindingsV2)
      .mockImplementationOnce(() => firstBindings.promise)
      .mockResolvedValueOnce([]);

    const older = useProviderStore.getState().refresh();
    const newer = useProviderStore.getState().refresh();
    await newer;
    firstProviders.resolve({ revision: 7, providers: [provider] });
    firstBindings.resolve([]);
    await older;

    expect(useProviderStore.getState().providerStoreRevision).toBe(8);
    expect(useProviderStore.getState().providers).toEqual([]);
    expect(useProviderStore.getState().loading).toBe(false);
  });

  it('does not commit a partial snapshot when bindings fail to load', async () => {
    useProviderStore.setState({
      providers: [provider],
      providerStoreRevision: 4,
      bindings: [],
    });
    vi.mocked(api.listProvidersV2).mockResolvedValueOnce({ revision: 5, providers: [] });
    vi.mocked(api.listProfileProviderBindingsV2).mockRejectedValueOnce(
      new Error('bindings failed'),
    );

    await expect(useProviderStore.getState().refresh()).rejects.toThrow('bindings failed');

    expect(useProviderStore.getState().providerStoreRevision).toBe(4);
    expect(useProviderStore.getState().providers).toEqual([provider]);
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

it('refreshes the account card after saving an attached Provider', async () => {
  useProviderStore.setState({
    providers: [{ ...provider, usedBy: ['profile-a'] }],
    providerStoreRevision: 4,
  });
  const refreshAccounts = vi
    .spyOn(useAccountStore.getState(), 'refresh')
    .mockResolvedValue(undefined);
  await useProviderStore.getState().saveProvider(definition, true);
  expect(refreshAccounts).toHaveBeenCalledOnce();
});

it('refreshes a rolled-back save revision before the user retries', async () => {
  useProviderStore.setState({ providers: [provider], providerStoreRevision: 4 });
  const error = { code: 'ATTACH_BINDING_COMMIT_FAILED', message: 'Save rolled back' };
  vi.mocked(api.updateProviderV2).mockRejectedValueOnce(error);
  vi.mocked(api.listProvidersV2).mockResolvedValueOnce({ revision: 6, providers: [provider] });
  await expect(useProviderStore.getState().saveProvider(definition, true)).rejects.toEqual(error);
  expect(useProviderStore.getState().providerStoreRevision).toBe(6);
  await useProviderStore.getState().saveProvider(definition, true);
  expect(api.updateProviderV2).toHaveBeenLastCalledWith({
    expectedRevision: 6,
    provider: definition,
  });
});
