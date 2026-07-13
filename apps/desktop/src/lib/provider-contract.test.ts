import { describe, expect, it } from 'vitest';
import type {
  AttachProviderRequest,
  AttachProviderResult,
  CodexAccount,
  CreateProviderRequest,
  OperationPlan,
  ProviderProfile,
} from './types';
import accountCacheFixture from '../../src-tauri/tests/fixtures/legacy-provider-contract/account/accounts-cache.json';
import attachRequestFixture from '../../src-tauri/tests/fixtures/legacy-provider-contract/dto/attach-provider-request.json';
import attachResultFixture from '../../src-tauri/tests/fixtures/legacy-provider-contract/dto/attach-provider-result.json';
import createRequestFixture from '../../src-tauri/tests/fixtures/legacy-provider-contract/dto/create-provider-request.json';
import operationPlanFixture from '../../src-tauri/tests/fixtures/legacy-provider-contract/dto/operation-plan.json';
import providerFixture from '../../src-tauri/tests/fixtures/legacy-provider-contract/dto/provider-profile-view.json';

const expectedProvider = {
  id: 'legacy-responses-service',
  name: 'Example Responses Service',
  baseUrl: 'https://responses.example.test/v1',
  wireApi: 'responses',
  defaultModel: 'example-response-model',
  envKey: 'EXAMPLE_RESPONSES_API_KEY',
  secretStorage: 'env',
  health: 'untested',
} satisfies ProviderProfile;

const expectedCreateRequest = {
  id: 'legacy-responses-service',
  name: 'Example Responses Service',
  baseUrl: 'https://responses.example.test/v1',
  wireApi: 'responses',
  defaultModel: 'example-response-model',
  envKey: 'EXAMPLE_RESPONSES_API_KEY',
  secret: { kind: 'env', envKey: 'EXAMPLE_RESPONSES_API_KEY' },
} satisfies CreateProviderRequest;

const expectedAttachRequest = {
  profileId: 'fixture-profile',
  providerId: 'legacy-responses-service',
  model: 'example-response-model',
} satisfies AttachProviderRequest;

const expectedAttachResult = {
  profileId: 'fixture-profile',
  providerId: 'legacy-responses-service',
  configPath: '/fixture/codex-home/config.toml',
  backupPath: '/fixture/codex-home/config.toml.backup.20260710-120000',
  operations: ['backup config.toml /fixture/codex-home'],
  warnings: ['Fixture warning'],
} satisfies AttachProviderResult;

const expectedOperationPlan = {
  operations: ['backup config.toml /fixture/codex-home'],
  warnings: ['Fixture warning'],
  blocked: ['api_key', 'auth.json'],
} satisfies OperationPlan;

const expectedAccountCache = {
  homeRoot: '/fixture',
  fetchedAt: 1_700_000_000,
  accounts: [
    {
      id: 'fixture-profile',
      displayName: 'codex-fixture-profile',
      codexHome: '/fixture/.codex-fixture-profile',
      wrapperPath: '/fixture/bin/codex-fixture-profile',
      hasAuth: true,
      hasConfig: true,
      hasHistory: false,
      sessionCount: 0,
      latestSessionModifiedAt: null,
      managed: true,
      isRelay: false,
      relaySource: null,
      relayIdentity: null,
      providerId: 'legacy-responses-service',
      model: 'example-response-model',
      authMode: 'config',
      isActiveAuth: false,
      hasPersonalAccessToken: false,
      renewalDate: null,
      note: 'Synthetic contract fixture',
    },
  ],
} satisfies { homeRoot: string; fetchedAt: number; accounts: CodexAccount[] };

describe('legacy Provider API contracts', () => {
  it('freezes the current camelCase Provider and create request shapes', () => {
    expect(providerFixture).toEqual(expectedProvider);
    expect(createRequestFixture).toEqual(expectedCreateRequest);
    expect(providerFixture).not.toHaveProperty('secret');
    expect(JSON.stringify(createRequestFixture)).not.toContain('PLACEHOLDER');
  });

  it('freezes attach request and command result shapes', () => {
    expect(attachRequestFixture).toEqual(expectedAttachRequest);
    expect(attachResultFixture).toEqual(expectedAttachResult);
    expect(operationPlanFixture).toEqual(expectedOperationPlan);
  });

  it('freezes the account cache shape consumed by the frontend', () => {
    expect(accountCacheFixture).toEqual(expectedAccountCache);
  });
});
