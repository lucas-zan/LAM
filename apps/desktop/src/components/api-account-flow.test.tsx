import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiAccountFlow } from './api-account-flow';
import * as api from '../lib/api';

vi.mock('../lib/api', () => ({
  discoverProviderModelsV2: vi.fn(),
  planApiAccountV2: vi.fn(),
  executeApiAccountV2: vi.fn(),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function fillConnection(apiKey = 'write-only-synthetic-secret') {
  fireEvent.change(screen.getByLabelText('Account name'), { target: { value: 'work-api' } });
  fireEvent.change(screen.getByLabelText('Base URL'), {
    target: { value: 'https://api.example.test/v1' },
  });
  fireEvent.change(screen.getByLabelText('API key'), { target: { value: apiKey } });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.discoverProviderModelsV2).mockResolvedValue({
    models: [
      { id: 'model-a', label: 'model-a' },
      { id: 'model-b', label: 'model-b' },
    ],
  });
  vi.mocked(api.planApiAccountV2).mockResolvedValue({
    planId: 'plan-1',
    fingerprint: 'fingerprint-1',
    expiresAtMs: Date.now() + 60_000,
    accountName: 'work-api',
    providerId: 'account-work-api',
    selectedModel: 'model-a',
    routeKind: 'gateway',
    operations: ['create Account', 'create Provider', 'attach'],
    warnings: [],
    blockers: [],
  });
  vi.mocked(api.executeApiAccountV2).mockResolvedValue({
    account: {
      profileId: 'work-api',
      homePath: '/tmp/.codex-work-api',
      wrapperPath: '/tmp/bin/codex-work-api',
      operations: [],
      warnings: [],
    },
    provider: {} as never,
    binding: {
      profileId: 'work-api',
      providerId: 'account-work-api',
      selectedModel: 'model-a',
      routeKind: 'gateway',
      revision: 1,
      providerRevision: 1,
    },
    attach: { state: 'committed', idempotent: false },
  });
});

describe('ApiAccountFlow', () => {
  it('groups API type with Base URL and exposes the derived read-only endpoint suffix', () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);

    expect(screen.queryByRole('group', { name: 'API protocol' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Responses' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Chat Completions' })).toBeNull();

    const apiType = screen.getByLabelText('API type') as HTMLSelectElement;
    const baseUrl = screen.getByLabelText('Base URL') as HTMLInputElement;
    const endpointSuffix = screen.getByLabelText('Endpoint suffix') as HTMLInputElement;

    expect(apiType.value).toBe('responses');
    expect(
      apiType.compareDocumentPosition(baseUrl) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(endpointSuffix.value).toBe('/responses');
    expect(endpointSuffix.readOnly).toBe(true);
    expect(baseUrl.nextElementSibling).toBe(endpointSuffix);

    fireEvent.change(apiType, { target: { value: 'chat_completions' } });
    expect(endpointSuffix.value).toBe('/chat/completions');
    expect(screen.getByLabelText('Compatibility')).toBeTruthy();
  });

  it('keeps Chat Completions request construction while storing only the editable Base URL', async () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection();
    fireEvent.change(screen.getByLabelText('API type'), {
      target: { value: 'chat_completions' },
    });
    fireEvent.change(screen.getByLabelText('Compatibility'), {
      target: { value: 'deepseek_chat_completions' },
    });
    fireEvent.change(screen.getByLabelText('Custom model'), { target: { value: 'deepseek-chat' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add custom model' }));
    fireEvent.change(screen.getByLabelText('Default model'), {
      target: { value: 'deepseek-chat' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Review API Account' }));

    await waitFor(() => expect(api.planApiAccountV2).toHaveBeenCalledOnce());
    expect(vi.mocked(api.planApiAccountV2).mock.calls[0][0]).toMatchObject({
      provider: {
        kind: 'new',
        provider: {
          protocol: 'chat_completions',
          baseUrl: 'https://api.example.test/v1',
          compatibilityProfile: 'deepseek_chat_completions',
          adapter: {
            kind: 'local',
            adapterId: 'responses_to_chat_completions',
            upstreamPath: '/chat/completions',
          },
        },
      },
    });
  });

  it('renders Fetch models as a compact discovery action', () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);

    const fetchModels = screen.getByRole('button', { name: 'Fetch models' });
    expect(fetchModels.classList.contains('uiBtn--sm')).toBe(true);
    expect(fetchModels.classList.contains('apiFetchModelsBtn')).toBe(true);
  });

  it('presents numbered setup sections in an API-specific layout grid', () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);

    const connectionHeading = screen.getByRole('heading', { name: '1 Connection details' });
    const modelHeading = screen.getByRole('heading', { name: '2 Model configuration' });
    expect(
      connectionHeading
        .closest('.apiFormSection')
        ?.parentElement?.classList.contains('apiFlowGrid'),
    ).toBe(true);
    expect(modelHeading).toBeTruthy();
  });

  it('disables unavailable Provider reuse and explains the empty state', () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: 'Advanced options' }));

    expect((screen.getByLabelText('Reuse existing Provider') as HTMLInputElement).disabled).toBe(
      true,
    );
    expect(screen.getByText('No existing connections are available to reuse.')).toBeTruthy();
  });

  it('renders discovered models inside a bounded checklist viewport', async () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection();
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));

    const modelOption = await screen.findByLabelText('Select model model-a');
    expect(
      modelOption
        .closest('.apiModelChecklist')
        ?.parentElement?.classList.contains('apiModelChecklistViewport'),
    ).toBe(true);
  });

  it('discovers, explicitly selects, plans and creates a strict Gateway API Account', async () => {
    const created = vi.fn();
    render(<ApiAccountFlow providers={[]} onCreated={created} onCancel={() => {}} />);
    expect(screen.queryByText('5.6 Sol')).toBeNull();
    fillConnection();

    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    await waitFor(() => expect(api.discoverProviderModelsV2).toHaveBeenCalledOnce());
    expect(api.discoverProviderModelsV2).toHaveBeenCalledWith({
      baseUrl: 'https://api.example.test/v1',
      apiKey: 'write-only-synthetic-secret',
    });
    expect(await screen.findByLabelText('Select model model-a')).toBeTruthy();
    expect((screen.getByLabelText('Select model model-a') as HTMLInputElement).checked).toBe(false);

    fireEvent.click(screen.getByLabelText('Select model model-a'));
    fireEvent.change(screen.getByLabelText('Default model'), { target: { value: 'model-a' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review API Account' }));

    await waitFor(() => expect(api.planApiAccountV2).toHaveBeenCalledOnce());
    const planRequest = vi.mocked(api.planApiAccountV2).mock.calls[0][0];
    expect(planRequest).toMatchObject({
      accountName: 'work-api',
      selectedModel: 'model-a',
      provider: {
        kind: 'new',
        provider: {
          id: 'account-work-api',
          models: [{ id: 'model-a', label: 'model-a' }],
          upstreamAuth: { kind: 'bearer', credential: { kind: 'none' } },
          codex: {
            routeViaGateway: true,
            directRequestMaxRetries: 0,
            directStreamMaxRetries: 0,
          },
        },
      },
    });
    expect(JSON.stringify(planRequest)).not.toContain('write-only-synthetic-secret');

    fireEvent.click(screen.getByRole('button', { name: 'Create API Account' }));
    await waitFor(() => expect(api.executeApiAccountV2).toHaveBeenCalledOnce());
    expect(vi.mocked(api.executeApiAccountV2).mock.calls[0][0]).toEqual({
      planId: 'plan-1',
      fingerprint: 'fingerprint-1',
      apiKey: 'write-only-synthetic-secret',
    });
    await waitFor(() => expect(created).toHaveBeenCalledWith('work-api'));
    expect(screen.queryByDisplayValue('write-only-synthetic-secret')).toBeNull();
  });

  it('clears discovered and selected models when URL or API key changes', async () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection();
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    fireEvent.click(await screen.findByLabelText('Select model model-a'));
    fireEvent.change(screen.getByLabelText('Default model'), { target: { value: 'model-a' } });

    fireEvent.change(screen.getByLabelText('Base URL'), {
      target: { value: 'https://new.example.test/v1' },
    });
    expect(screen.queryByLabelText('Select model model-a')).toBeNull();
    expect((screen.getByLabelText('Default model') as HTMLInputElement).value).toBe('');
  });

  it('ignores an older discovery response after credentials start a newer request', async () => {
    const oldRequest = deferred<Awaited<ReturnType<typeof api.discoverProviderModelsV2>>>();
    const newRequest = deferred<Awaited<ReturnType<typeof api.discoverProviderModelsV2>>>();
    vi.mocked(api.discoverProviderModelsV2)
      .mockReturnValueOnce(oldRequest.promise)
      .mockReturnValueOnce(newRequest.promise);
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection('old-key');
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));

    fireEvent.change(screen.getByLabelText('API key'), { target: { value: 'new-key' } });
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    await act(async () => {
      newRequest.resolve({ models: [{ id: 'new-model', label: 'new-model' }] });
    });
    expect(await screen.findByLabelText('Select model new-model')).toBeTruthy();

    await act(async () => {
      oldRequest.resolve({ models: [{ id: 'old-model', label: 'old-model' }] });
    });
    expect(screen.queryByLabelText('Select model old-model')).toBeNull();
    expect(screen.getByLabelText('Select model new-model')).toBeTruthy();
  });

  it.each([
    ['backend failure', () => Promise.reject(new Error('Discovery failed'))],
    ['empty response', () => Promise.resolve({ models: [] })],
  ])('keeps an actionable custom-model fallback after %s', async (_label, result) => {
    vi.mocked(api.discoverProviderModelsV2).mockReturnValueOnce(result());
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection();
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));

    expect(await screen.findByText(/Add a custom model/)).toBeTruthy();
    expect((screen.getByLabelText('API key') as HTMLInputElement).value).toBe('');
    fireEvent.change(screen.getByLabelText('Custom model'), { target: { value: 'manual-model' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add custom model' }));
    expect(screen.getAllByText('manual-model').length).toBeGreaterThan(0);
  });

  it('does not plan when the default model is outside the selected allowlist', async () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection();
    fireEvent.change(screen.getByLabelText('Default model'), { target: { value: 'outside' } });
    fireEvent.change(screen.getByLabelText('Custom model'), { target: { value: 'allowed' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add custom model' }));
    fireEvent.click(screen.getByRole('button', { name: 'Review API Account' }));

    expect(await screen.findByText('Default model must be present in Models')).toBeTruthy();
    expect(api.planApiAccountV2).not.toHaveBeenCalled();
  });

  it('makes Provider reuse an explicit advanced choice', async () => {
    render(
      <ApiAccountFlow
        providers={[
          {
            id: 'shared',
            name: 'Shared connection',
            protocol: 'responses',
            baseUrl: 'https://shared.example.test/v1',
            defaultModel: 'shared-model',
            models: [{ id: 'shared-model', label: 'Shared Model' }],
            upstreamAuth: { kind: 'none' },
            adapter: { kind: 'none' },
            codex: { queryParams: {}, envHttpHeaders: {} },
            storeRevision: 1,
            usedBy: [],
            readinessBlockers: [],
          },
        ]}
        onCreated={() => {}}
        onCancel={() => {}}
      />,
    );
    fireEvent.change(screen.getByLabelText('Account name'), { target: { value: 'second' } });
    fireEvent.click(screen.getByRole('button', { name: 'Advanced options' }));
    fireEvent.click(screen.getByLabelText('Reuse existing Provider'));
    fireEvent.change(screen.getByLabelText('Existing Provider'), { target: { value: 'shared' } });
    fireEvent.change(screen.getByLabelText('Default model'), {
      target: { value: 'shared-model' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Review API Account' }));
    await waitFor(() => expect(api.planApiAccountV2).toHaveBeenCalledOnce());
    expect(vi.mocked(api.planApiAccountV2).mock.calls[0][0].provider).toEqual({
      kind: 'existing',
      providerId: 'shared',
    });
    expect(screen.queryByLabelText('API key')).toBeNull();
    expect(api.discoverProviderModelsV2).not.toHaveBeenCalled();
  });
  it('applies context window preset and reasoning effort to planned models', async () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection();
    fireEvent.change(screen.getByLabelText('Custom model'), {
      target: { value: 'deepseek-chat' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add custom model' }));
    fireEvent.change(screen.getByLabelText('Default model'), {
      target: { value: 'deepseek-chat' },
    });

    // Choose the 272K context window preset and a high reasoning effort.
    fireEvent.click(screen.getByRole('button', { name: '272K' }));
    fireEvent.change(screen.getByLabelText('Default reasoning effort'), {
      target: { value: 'high' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Review API Account' }));

    await waitFor(() => expect(api.planApiAccountV2).toHaveBeenCalledOnce());
    const planRequest = vi.mocked(api.planApiAccountV2).mock.calls[0][0];
    expect(planRequest.provider).toMatchObject({
      kind: 'new',
      provider: {
        models: [{ id: 'deepseek-chat', label: 'deepseek-chat', contextWindow: 272_000 }],
        codex: { reasoningEffort: 'high' },
      },
    });
  });

  it('applies a custom context window value when Custom is chosen', async () => {
    render(<ApiAccountFlow providers={[]} onCreated={() => {}} onCancel={() => {}} />);
    fillConnection();
    fireEvent.change(screen.getByLabelText('Custom model'), {
      target: { value: 'custom-model' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add custom model' }));
    fireEvent.change(screen.getByLabelText('Default model'), {
      target: { value: 'custom-model' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'Custom' }));
    fireEvent.change(screen.getByLabelText('Custom context window'), {
      target: { value: '200000' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Review API Account' }));

    await waitFor(() => expect(api.planApiAccountV2).toHaveBeenCalledOnce());
    const planRequest = vi.mocked(api.planApiAccountV2).mock.calls[0][0];
    expect(planRequest.provider).toMatchObject({
      kind: 'new',
      provider: {
        models: [{ id: 'custom-model', label: 'custom-model', contextWindow: 200_000 }],
      },
    });
  });

});
