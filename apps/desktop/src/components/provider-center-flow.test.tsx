import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ProviderCenter } from './provider-center';
import { useProviderStore } from '../stores/providers';
import { useAppStore } from '../stores/app';
import * as api from '../lib/api';
import type { ProviderProfileViewV2 } from '../lib/types';

vi.mock('../lib/api', () => ({
  listProvidersV2: vi.fn(),
  listProfileProviderBindingsV2: vi.fn(),
  createProviderV2: vi.fn(),
  createProviderWithKeychainV2: vi.fn(),
  updateProviderV2: vi.fn(),
  rotateProviderCredentialV2: vi.fn(),
  planAttachProviderV2: vi.fn(),
  executeAttachProviderV2: vi.fn(),
  planDetachProviderV2: vi.fn(),
  executeDetachProviderV2: vi.fn(),
  testProviderUpstreamV2: vi.fn(),
  getApiAccountConnectionV2: vi.fn(),
  updateApiAccountConnectionV2: vi.fn(),
}));

const provider: ProviderProfileViewV2 = {
  id: 'company',
  name: 'Company',
  protocol: 'responses',
  baseUrl: 'https://company.example.test/v1',
  defaultModel: 'model-a',
  models: [{ id: 'model-a', label: 'Model A' }],
  upstreamAuth: { kind: 'none' },
  adapter: { kind: 'none' },
  codex: {
    streamIdleTimeoutMs: 300_000,
    directRequestMaxRetries: 0,
    directStreamMaxRetries: 0,
    queryParams: {},
    envHttpHeaders: {},
  },
  storeRevision: 2,
  usedBy: [],
  readinessBlockers: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  useProviderStore.setState({
    providers: [provider],
    bindings: [],
    attachPlan: null,
    detachPlan: null,
    loading: false,
    recoveryMessage: '',
  });
  useAppStore.setState({ status: 'Ready', error: '' });
  vi.mocked(api.listProvidersV2).mockResolvedValue({ revision: 4, providers: [provider] });
  vi.mocked(api.listProfileProviderBindingsV2).mockResolvedValue([]);
  vi.mocked(api.testProviderUpstreamV2).mockResolvedValue({
    providerId: 'company',
    ok: true,
    routeKind: 'direct',
    modelsEndpoint: 'https://company.example.test/v1/models',
    redactedSummary: 'Direct Responses route and credential reference validated',
    modelCount: 1,
  });
  vi.mocked(api.createProviderWithKeychainV2).mockResolvedValue({
    ...provider,
    upstreamAuth: {
      kind: 'bearer',
      credential: {
        kind: 'keychain',
        service: 'lam.remote-provider',
        account: 'opaque',
        version: 1,
      },
    },
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
});

describe('ProviderCenter integrated flow', () => {
  it('routes top-level creation into the complete External API account flow', () => {
    const onAddExternalApi = vi.fn();
    render(<ProviderCenter profiles={['profile-a']} onAddExternalApi={onAddExternalApi} />);

    fireEvent.click(screen.getByRole('button', { name: 'Add External API' }));
    expect(onAddExternalApi).toHaveBeenCalledOnce();
    expect(screen.queryByRole('heading', { name: 'Add Provider' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Add Provider' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(screen.getByRole('heading', { name: 'Edit Provider' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));

    fireEvent.click(screen.getByRole('button', { name: 'Attach' }));
    expect(screen.getByRole('heading', { name: 'Attach Provider' })).toBeTruthy();
    expect(screen.getByLabelText('Profile')).toBeTruthy();
  });

  it('runs the V2 upstream test action and reports only its redacted summary', async () => {
    render(<ProviderCenter profiles={['profile-a']} onAddExternalApi={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Test upstream' }));

    await waitFor(() => expect(api.testProviderUpstreamV2).toHaveBeenCalledWith('company'));
    expect(useAppStore.getState().status).toBe(
      'Direct Responses route and credential reference validated',
    );
    expect(useAppStore.getState().status).not.toContain('token');
  });

  it('routes an exclusive native Responses connection to the API account editor', async () => {
    useProviderStore.setState({
      providers: [
        {
          ...provider,
          id: 'account-work-api',
          upstreamAuth: {
            kind: 'bearer',
            credential: { kind: 'codex_profile', profileId: 'work-api' },
          },
        },
      ],
      bindings: [
        {
          profileId: 'work-api',
          providerId: 'account-work-api',
          selectedModel: 'model-a',
          routeKind: 'direct',
          revision: 1,
          providerRevision: 1,
        },
      ],
    });
    render(<ProviderCenter profiles={['work-api']} onAddExternalApi={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    expect(screen.getByRole('heading', { name: 'Edit API Account' })).toBeTruthy();
    await waitFor(() => expect(api.getApiAccountConnectionV2).toHaveBeenCalledWith('work-api'));
    expect(await screen.findByDisplayValue('https://api.example.test/v1')).toBeTruthy();
    expect(screen.getByText('API key configured')).toBeTruthy();
  });

  it('does not expose standalone Provider creation even when the list is empty', () => {
    const onAddExternalApi = vi.fn();
    useProviderStore.setState({ providers: [] });
    render(<ProviderCenter profiles={['profile-a']} onAddExternalApi={onAddExternalApi} />);

    fireEvent.click(screen.getByRole('button', { name: 'Add External API' }));
    expect(onAddExternalApi).toHaveBeenCalledOnce();
    expect(api.createProviderV2).not.toHaveBeenCalled();
    expect(api.createProviderWithKeychainV2).not.toHaveBeenCalled();
  });
});
