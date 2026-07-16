import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createProviderV2,
  executeAttachProviderV2,
  executeDetachProviderV2,
  listProfileProviderBindingsV2,
  listProvidersV2,
  planAttachProviderV2,
  planDetachProviderV2,
  updateProviderV2,
  rotateProviderCredentialV2,
  createProviderLegacyCompatV2,
  createProviderWithKeychainV2,
  testProviderUpstreamV2,
  planGatewayPortMigrationV2,
  executeGatewayPortMigrationV2,
  discoverProviderModelsV2,
} from './api';
import type { CreateProviderRequestV2 } from './types';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke);

const request: CreateProviderRequestV2 = {
  expectedRevision: 0,
  provider: {
    id: 'company-proxy',
    name: 'Company Proxy',
    protocol: 'responses',
    baseUrl: 'https://proxy.example.test/v1',
    defaultModel: 'model-a',
    models: [{ id: 'model-a', label: 'Model A' }],
    upstreamAuth: { kind: 'bearer', credential: { kind: 'env', envKey: 'COMPANY_TOKEN' } },
    adapter: { kind: 'none' },
    codex: { queryParams: {}, envHttpHeaders: {} },
  },
};

beforeEach(() => mockInvoke.mockReset());

describe('Provider V2 API wrappers', () => {
  it('passes create/update/list requests without shape conversion', async () => {
    mockInvoke.mockResolvedValueOnce([]).mockResolvedValueOnce({}).mockResolvedValueOnce({});
    await listProvidersV2();
    await createProviderV2(request);
    await updateProviderV2({ ...request, expectedRevision: 1 });
    expect(mockInvoke).toHaveBeenNthCalledWith(1, 'list_providers_v2');
    expect(mockInvoke).toHaveBeenNthCalledWith(2, 'create_provider_v2', { req: request });
    expect(mockInvoke).toHaveBeenNthCalledWith(3, 'update_provider_v2', {
      req: { ...request, expectedRevision: 1 },
    });
  });

  it('passes plan/execute/detach tickets and fingerprints exactly', async () => {
    mockInvoke.mockResolvedValue({});
    const plan = { profileId: 'profile-a', providerId: 'company-proxy', selectedModel: 'model-a' };
    const execute = { planId: 'plan-1', fingerprint: 'fingerprint-1' };
    await planAttachProviderV2(plan);
    await executeAttachProviderV2(execute);
    await listProfileProviderBindingsV2();
    await planDetachProviderV2('profile-a');
    await executeDetachProviderV2(execute);
    expect(mockInvoke.mock.calls).toEqual([
      ['plan_attach_provider_v2', { req: plan }],
      ['execute_attach_provider_v2', { req: execute }],
      ['list_profile_provider_bindings_v2'],
      ['plan_detach_provider_v2', { profileId: 'profile-a' }],
      ['execute_detach_provider_v2', { req: execute }],
    ]);
  });

  it('uses the independent redacted upstream-test command', async () => {
    mockInvoke.mockResolvedValue({});
    await testProviderUpstreamV2('company-proxy');
    expect(mockInvoke).toHaveBeenCalledWith('test_provider_upstream_v2', {
      providerId: 'company-proxy',
    });
  });

  it('passes model discovery credentials only to the write-only command request', async () => {
    mockInvoke.mockResolvedValue({ models: [{ id: 'model-a', label: 'model-a' }] });
    const discovery = {
      baseUrl: 'https://proxy.example.test/v1',
      apiKey: 'synthetic-write-only-key',
    };
    await discoverProviderModelsV2(discovery);
    expect(mockInvoke).toHaveBeenCalledWith('discover_provider_models_v2', { req: discovery });
  });

  it('passes a Keychain secret only to the dedicated rotation request', async () => {
    mockInvoke.mockResolvedValue({});
    const rotation = {
      expectedRevision: 1,
      providerId: 'company-proxy',
      expectedCredential: { kind: 'env' as const, envKey: 'COMPANY_TOKEN' },
      secret: 'synthetic-input-only',
    };
    await rotateProviderCredentialV2(rotation);
    expect(mockInvoke).toHaveBeenCalledWith('rotate_provider_credential_v2', { req: rotation });
  });

  it('passes new Keychain secrets only to the atomic create command', async () => {
    mockInvoke.mockResolvedValue({});
    const keychainCreate = {
      ...request,
      provider: {
        ...request.provider,
        upstreamAuth: {
          kind: 'bearer' as const,
          credential: { kind: 'none' as const },
        },
      },
      secret: 'synthetic-create-input-only',
    };
    await createProviderWithKeychainV2(keychainCreate);
    expect(mockInvoke).toHaveBeenCalledWith('create_provider_with_keychain_v2', {
      req: keychainCreate,
    });
  });

  it('keeps legacy envKey conversion behind the compatibility command', async () => {
    mockInvoke.mockResolvedValue({});
    const legacy = {
      id: 'legacy',
      name: 'Legacy',
      baseUrl: 'https://legacy.example.test/v1',
      wireApi: 'openai',
      defaultModel: 'legacy-model',
      envKey: 'LEGACY_TOKEN',
      secret: { kind: 'env' as const, envKey: 'LEGACY_TOKEN' },
    };
    await createProviderLegacyCompatV2(legacy, 0);
    expect(mockInvoke).toHaveBeenCalledWith('create_provider_legacy_compat_v2', {
      req: legacy,
      expectedRevision: 0,
    });
  });

  it('contains no secret field in the V2 request/view contract', () => {
    expect(JSON.stringify(request)).not.toContain('secret');
    expect(JSON.stringify(request)).not.toContain('env_key');
  });

  it('keeps stable-port migration behind an explicit plan fingerprint', async () => {
    mockInvoke.mockResolvedValue({});
    const plan = {
      expectedStateRevision: 3,
      oldPort: 54321,
      newPort: 54322,
      profileIds: ['profile-a', 'profile-b'],
      fingerprint: 'port-plan-fingerprint',
    };
    await planGatewayPortMigrationV2(54322);
    await executeGatewayPortMigrationV2(plan);
    expect(mockInvoke.mock.calls).toEqual([
      ['plan_gateway_port_migration_v2', { newPort: 54322 }],
      ['execute_gateway_port_migration_v2', { plan, fingerprint: 'port-plan-fingerprint' }],
    ]);
  });
});
