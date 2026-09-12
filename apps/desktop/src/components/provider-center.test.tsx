import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ApiAccountConnectionEditor, ProviderCards, ProviderEditor } from './provider-center';
import type { ProviderDefinitionV2, ProviderProfileViewV2 } from '../lib/types';

const provider: ProviderProfileViewV2 = {
  id: 'company',
  name: 'Company Responses',
  protocol: 'responses',
  baseUrl: 'https://company.example.test/v1',
  defaultModel: 'model-a',
  models: [
    { id: 'model-a', label: 'Model A' },
    { id: 'model-b', label: 'Model B' },
  ],
  upstreamAuth: {
    kind: 'bearer',
    credential: { kind: 'env', envKey: 'COMPANY_TOKEN' },
  },
  adapter: { kind: 'none' },
  codex: {
    routeViaGateway: true,
    streamIdleTimeoutMs: 300_000,
    directRequestMaxRetries: 0,
    directStreamMaxRetries: 0,
    queryParams: { region: 'us' },
    envHttpHeaders: { 'x-feature': 'FEATURE_FLAG' },
  },
  storeRevision: 3,
  usedBy: ['profile-a'],
  readinessBlockers: ['ENV_MISSING:COMPANY_TOKEN'],
};

describe('ProviderCards', () => {
  it('shows the V2 contract, credential reference, binding count, blockers, and actions', () => {
    const onTest = vi.fn();
    render(
      <ProviderCards
        providers={[provider]}
        bindings={[
          {
            profileId: 'profile-a',
            providerId: 'company',
            selectedModel: 'model-b',
            routeKind: 'direct',
            revision: 1,
            providerRevision: 3,
          },
        ]}
        onEdit={vi.fn()}
        onAttach={vi.fn()}
        onDetach={vi.fn()}
        onDelete={vi.fn()}
        onTest={onTest}
      />,
    );

    expect(screen.getByText('Responses')).toBeTruthy();
    expect(screen.getByText('model-a')).toBeTruthy();
    expect(screen.getByText('Env · COMPANY_TOKEN')).toBeTruthy();
    expect(screen.getByText('1 binding')).toBeTruthy();
    expect(screen.getByText('ENV_MISSING:COMPANY_TOKEN')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Test upstream' }));
    expect(onTest).toHaveBeenCalledWith(provider);
  });

  it('presents a ready Chat Completions compatibility profile as Gateway-attachable', () => {
    render(
      <ProviderCards
        providers={[
          {
            ...provider,
            protocol: 'chat_completions',
            readinessBlockers: [],
            readiness: { ready: true, blockers: [], bindingCount: 0 },
          },
        ]}
        bindings={[]}
        onEdit={vi.fn()}
        onAttach={vi.fn()}
        onDetach={vi.fn()}
        onDelete={vi.fn()}
        onTest={vi.fn()}
      />,
    );

    expect(screen.getByText('Chat Completions')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Attach' }) as HTMLButtonElement).disabled).toBe(
      false,
    );
    expect(screen.getByText('Gateway adapter route ready')).toBeTruthy();
  });

  it('offers deletion for an unbound Provider connection', () => {
    const onDelete = vi.fn();
    render(
      <ProviderCards
        providers={[{ ...provider, usedBy: [], readinessBlockers: [] }]}
        bindings={[]}
        onEdit={vi.fn()}
        onAttach={vi.fn()}
        onDetach={vi.fn()}
        onDelete={onDelete}
        onTest={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Delete Provider' }));
    expect(onDelete).toHaveBeenCalledWith(expect.objectContaining({ id: 'company' }));
  });

  it('requires bound accounts to be detached before Provider deletion', () => {
    render(
      <ProviderCards
        providers={[provider]}
        bindings={[
          {
            profileId: 'profile-a',
            providerId: 'company',
            selectedModel: 'model-a',
            routeKind: 'direct',
            revision: 1,
            providerRevision: 3,
          },
        ]}
        onEdit={vi.fn()}
        onAttach={vi.fn()}
        onDetach={vi.fn()}
        onDelete={vi.fn()}
        onTest={vi.fn()}
      />,
    );

    const button = screen.getByRole('button', { name: 'Delete Provider' }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.title).toBe('Detach all accounts before deleting');
  });
});

describe('ProviderEditor', () => {
  it('normalizes a valid Responses/env form into the independent V2 DTO', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<ProviderEditor provider={null} onSave={onSave} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Provider id'), { target: { value: 'acme' } });
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Acme' } });
    fireEvent.change(screen.getByLabelText('Base URL'), {
      target: { value: 'https://api.acme.test/v1/' },
    });
    fireEvent.change(screen.getByLabelText('Models'), {
      target: { value: 'model-a, model-b' },
    });
    fireEvent.change(screen.getByLabelText('Default model'), {
      target: { value: 'model-b' },
    });
    fireEvent.change(screen.getByLabelText('Environment variable'), {
      target: { value: 'ACME_TOKEN' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Create Provider' }));

    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    const submitted = onSave.mock.calls[0][0] as ProviderDefinitionV2;
    expect(submitted).toMatchObject({
      id: 'acme',
      name: 'Acme',
      protocol: 'responses',
      baseUrl: 'https://api.acme.test/v1',
      defaultModel: 'model-b',
      models: [
        { id: 'model-a', label: 'model-a' },
        { id: 'model-b', label: 'model-b' },
      ],
      upstreamAuth: {
        kind: 'bearer',
        credential: { kind: 'env', envKey: 'ACME_TOKEN' },
      },
      adapter: { kind: 'none' },
    });
    expect(JSON.stringify(submitted)).not.toContain('secret');
  });

  it.each([
    ['Base URL', 'not-a-url', 'Enter a valid HTTPS Provider URL'],
    ['Default model', 'missing', 'Default model must be listed in Models'],
    ['Environment variable', '', 'Environment variable is required'],
  ])('rejects invalid %s values', async (label, value, message) => {
    const onSave = vi.fn();
    render(<ProviderEditor provider={null} onSave={onSave} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText(label), { target: { value } });
    fireEvent.click(screen.getByRole('button', { name: 'Create Provider' }));

    expect(await screen.findByText(message)).toBeTruthy();
    expect(onSave).not.toHaveBeenCalled();
  });

  it('rejects invalid retry and timeout options', async () => {
    const onSave = vi.fn();
    render(<ProviderEditor provider={null} onSave={onSave} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Stream idle timeout (ms)'), {
      target: { value: '0' },
    });
    fireEvent.change(screen.getByLabelText('Request retries'), {
      target: { value: '2' },
    });
    fireEvent.change(screen.getByLabelText('Protocol'), {
      target: { value: 'chat_completions' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Create Provider' }));

    expect(
      await screen.findByText('Gateway profiles require request and stream retries to be 0'),
    ).toBeTruthy();
    expect(onSave).not.toHaveBeenCalled();
  });

  it('creates a controlled Chat Completions Gateway profile', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<ProviderEditor provider={null} onSave={onSave} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Protocol'), {
      target: { value: 'chat_completions' },
    });
    fireEvent.change(screen.getByLabelText('Compatibility profile'), {
      target: { value: 'deepseek_chat_completions' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Create Provider' }));

    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave.mock.calls[0][0]).toMatchObject({
      protocol: 'chat_completions',
      adapter: {
        kind: 'local',
        adapterId: 'responses_to_chat_completions',
        upstreamPath: '/chat/completions',
      },
      compatibilityProfile: 'deepseek_chat_completions',
      codex: { directRequestMaxRetries: 0, directStreamMaxRetries: 0 },
    });
  });

  it('collects a structured auth command request instead of accepting a manual approval id', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<ProviderEditor provider={null} onSave={onSave} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Credential source'), {
      target: { value: 'auth_command' },
    });
    expect(screen.queryByLabelText('Approved auth command id')).toBeNull();
    fireEvent.change(screen.getByLabelText('Auth command executable'), {
      target: { value: '/usr/local/bin/fetch-token' },
    });
    fireEvent.change(screen.getByLabelText('Arguments (one per line)'), {
      target: { value: '--audience\ncodex' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Create Provider' }));

    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave.mock.calls[0][2]).toEqual({
      executable: '/usr/local/bin/fetch-token',
      args: ['--audience', 'codex'],
    });
  });

  it.each(['resolve', 'reject'])(
    'keeps Keychain secret write-only and clears it after %s',
    async (outcome) => {
      const onSave =
        outcome === 'resolve'
          ? vi.fn().mockResolvedValue(undefined)
          : vi.fn().mockRejectedValue(new Error('server failed'));
      render(<ProviderEditor provider={provider} onSave={onSave} onCancel={vi.fn()} />);

      fireEvent.change(screen.getByLabelText('Credential source'), {
        target: { value: 'keychain' },
      });
      const secret = screen.getByLabelText('API key');
      expect((secret as HTMLInputElement).value).toBe('');
      fireEvent.change(secret, { target: { value: 'synthetic-write-only-secret' } });
      fireEvent.click(screen.getByRole('button', { name: 'Save Provider' }));

      await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
      await waitFor(() => expect((secret as HTMLInputElement).value).toBe(''));
      expect(onSave.mock.calls[0][1]).toBe('synthetic-write-only-secret');
      expect(onSave.mock.calls[0][0].codex).toMatchObject({
        routeViaGateway: true,
        streamIdleTimeoutMs: 300_000,
        directRequestMaxRetries: 0,
        directStreamMaxRetries: 0,
        queryParams: { region: 'us' },
        envHttpHeaders: { 'x-feature': 'FEATURE_FLAG' },
      });
      expect(screen.queryByDisplayValue('COMPANY_TOKEN')).toBeNull();
    },
  );
});

describe('ProviderEditor context window', () => {
  it('applies a context window preset to all models and forwards reasoning effort', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <ProviderEditor
        provider={{
          ...provider,
          models: [
            { id: 'model-a', label: 'Model A' },
            { id: 'model-b', label: 'Model B' },
          ],
          codex: { ...provider.codex, reasoningEffort: 'high' },
        }}
        onSave={onSave}
        onCancel={vi.fn()}
      />,
    );

    const modelsField = screen.getByLabelText('Models') as HTMLTextAreaElement;
    expect(modelsField.value).toBe('model-a, model-b');

    fireEvent.click(screen.getByRole('button', { name: '272K' }));
    expect(modelsField.value).toBe('model-a:272000, model-b:272000');

    const effort = screen.getByLabelText('Default reasoning effort') as HTMLSelectElement;
    expect(effort.value).toBe('high');

    fireEvent.click(screen.getByRole('button', { name: 'Save Provider' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    const submitted = onSave.mock.calls[0][0] as ProviderDefinitionV2;
    expect(submitted.models).toEqual([
      { id: 'model-a', label: 'model-a', contextWindow: 272_000 },
      { id: 'model-b', label: 'model-b', contextWindow: 272_000 },
    ]);
    expect(submitted.codex.reasoningEffort).toBe('high');
  });
});

describe('ApiAccountConnectionEditor', () => {
  const detail = {
    profileId: 'work-api',
    providerId: 'account-work-api',
    protocol: 'responses' as const,
    baseUrl: 'https://api.example.test/v1',
    selectedModel: 'model-a',
    providerStoreRevision: 7,
    apiKeyConfigured: true,
    models: [
      { id: 'model-a', label: 'Model A' },
      { id: 'model-b', label: 'Model B' },
    ],
  };

  it('shows redacted account configuration and supports URL-only updates', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<ApiAccountConnectionEditor connection={detail} onSave={onSave} onCancel={vi.fn()} />);

    expect(screen.getByText('work-api')).toBeTruthy();
    expect(screen.getByText('API key configured')).toBeTruthy();
    const key = screen.getByLabelText('API key');
    expect((key as HTMLInputElement).value).toBe('');
    fireEvent.change(screen.getByLabelText('Base URL'), {
      target: { value: 'https://new.example.test/v1/' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save API account' }));

    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave).toHaveBeenCalledWith({
      profileId: 'work-api',
      expectedProviderStoreRevision: 7,
      baseUrl: 'https://new.example.test/v1',
    });
  });

  it('sends a replacement key once, clears it, and rejects whitespace keys', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<ApiAccountConnectionEditor connection={detail} onSave={onSave} onCancel={vi.fn()} />);
    const key = screen.getByLabelText('API key');
    fireEvent.change(key, { target: { value: 'sk-replacement-not-real' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save API account' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave.mock.calls[0][0].apiKey).toBe('sk-replacement-not-real');
    await waitFor(() => expect((key as HTMLInputElement).value).toBe(''));

    fireEvent.change(key, { target: { value: '   ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save API account' }));
    expect(await screen.findByText('API key cannot be blank')).toBeTruthy();
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it('shows the saved selection first and fetches live models without selecting all of them', async () => {
    const onRefreshModels = vi.fn().mockResolvedValue([
      { id: 'model-a', label: 'Model A' },
      { id: 'model-c', label: 'Model C' },
    ]);
    render(
      <ApiAccountConnectionEditor
        connection={detail}
        onSave={vi.fn()}
        onRefreshModels={onRefreshModels}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText('Selected models')).toBeTruthy();
    expect(screen.getByText('Model A')).toBeTruthy();
    expect(screen.getByText('Model B')).toBeTruthy();
    expect(screen.queryByText('Model C')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    await waitFor(() => expect(onRefreshModels).toHaveBeenCalledOnce());
    expect(await screen.findByText('Fetched models')).toBeTruthy();
    expect(await screen.findByText('Model C')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Replace all' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Customize selection' })).toBeTruthy();
  });

  it('only hints when fetched models match the saved allowlist', async () => {
    const onRefreshModels = vi.fn().mockResolvedValue([
      { id: 'model-b', label: 'Model B renamed' },
      { id: 'model-a', label: 'Model A renamed' },
    ]);
    render(
      <ApiAccountConnectionEditor
        connection={detail}
        onSave={vi.fn()}
        onRefreshModels={onRefreshModels}
        onCancel={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    expect(await screen.findByText('Fetched models match the current allowlist.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Replace all' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Customize selection' })).toBeNull();
  });

  it('applies a full replace with a required default when the current model is missing', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const onRefreshModels = vi.fn().mockResolvedValue([
      { id: 'model-c', label: 'Model C' },
      { id: 'model-d', label: 'Model D' },
    ]);
    render(
      <ApiAccountConnectionEditor
        connection={detail}
        onSave={onSave}
        onRefreshModels={onRefreshModels}
        onCancel={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Replace all' }));
    expect(screen.getByRole('button', { name: 'Apply models' })).toHaveProperty('disabled', true);
    fireEvent.change(screen.getByLabelText('Default model for allowlist'), {
      target: { value: 'model-d' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Apply models' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave).toHaveBeenCalledWith({
      profileId: 'work-api',
      expectedProviderStoreRevision: 7,
      baseUrl: 'https://api.example.test/v1',
      models: [
        { id: 'model-c', label: 'Model C', contextWindow: undefined },
        { id: 'model-d', label: 'Model D', contextWindow: undefined },
      ],
      selectedModel: 'model-d',
      reasoningEffort: 'medium',
    });
  });

  it('customizes the allowlist from the union of saved and fetched models', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const onRefreshModels = vi.fn().mockResolvedValue([
      { id: 'model-a', label: 'Model A' },
      { id: 'model-c', label: 'Model C' },
    ]);
    render(
      <ApiAccountConnectionEditor
        connection={detail}
        onSave={onSave}
        onRefreshModels={onRefreshModels}
        onCancel={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Customize selection' }));
    fireEvent.click(screen.getByLabelText('Select model model-b'));
    fireEvent.click(screen.getByLabelText('Select model model-c'));
    fireEvent.click(screen.getByRole('button', { name: 'Apply models' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave).toHaveBeenCalledWith({
      profileId: 'work-api',
      expectedProviderStoreRevision: 7,
      baseUrl: 'https://api.example.test/v1',
      models: [
        { id: 'model-a', label: 'Model A', contextWindow: undefined },
        { id: 'model-c', label: 'Model C', contextWindow: undefined },
      ],
      selectedModel: 'model-a',
      reasoningEffort: 'medium',
    });
  });
});

describe('Gateway account model saving', () => {
  function renderGateway(onSave = vi.fn().mockResolvedValue(undefined)) {
    render(
      <ProviderEditor
        provider={{
          ...provider,
          protocol: 'chat_completions',
          adapter: {
            kind: 'local',
            adapterId: 'responses_to_chat_completions',
            upstreamPath: '/chat/completions',
          },
          compatibilityProfile: 'openai_chat_completions',
        }}
        simpleCredentials
        onSave={onSave}
        onCancel={vi.fn()}
        onRefreshModels={async () => [
          { id: 'model-a', label: 'Model A' },
          { id: 'model-c', label: 'Model C' },
        ]}
      />,
    );
    return onSave;
  }

  async function customize() {
    fireEvent.click(screen.getByRole('button', { name: 'Fetch models' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Customize selection' }));
    fireEvent.click(screen.getByLabelText('Select model model-c'));
  }

  it.each(['model-a', 'model-c'])('persists Apply models with default %s', async (selected) => {
    const onSave = renderGateway();
    await customize();
    fireEvent.change(screen.getByLabelText('Default model for allowlist'), {
      target: { value: selected },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Apply models' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave.mock.calls[0][0]).toMatchObject({
      defaultModel: selected,
      models: expect.arrayContaining([
        expect.objectContaining({ id: 'model-c', label: 'Model C' }),
      ]),
    });
  });

  it('preserves the chosen context window when applying freshly fetched models', async () => {
    const onSave = renderGateway();
    fireEvent.click(screen.getByRole('button', { name: '1M' }));
    await customize();
    fireEvent.click(screen.getByRole('button', { name: 'Apply models' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(
      onSave.mock.calls[0][0].models.every(
        (model: { contextWindow?: number }) => model.contextWindow === 1_000_000,
      ),
    ).toBe(true);
  });

  it('keeps the failed selection available for retry and displays structured errors', async () => {
    const onSave = renderGateway(
      vi.fn().mockRejectedValue({
        code: 'STORE_REVISION_CONFLICT',
        message: 'Provider changed; refresh and retry',
        recoverable: true,
      }),
    );
    await customize();
    fireEvent.click(screen.getByRole('button', { name: 'Apply models' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect((await screen.findByRole('alert')).textContent).toMatch(
      /Provider changed|STORE_REVISION_CONFLICT/,
    );
    expect(screen.getByLabelText('Select model model-c')).toHaveProperty('checked', true);
    expect(screen.getByRole('button', { name: 'Apply models' })).toHaveProperty('disabled', false);
  });

  it('disables both save actions and fetching while applying a selection', async () => {
    const onSave = renderGateway(vi.fn(() => new Promise<void>(() => {})));
    await customize();
    fireEvent.click(screen.getByRole('button', { name: 'Apply models' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(screen.getByRole('button', { name: 'Applying…' })).toHaveProperty('disabled', true);
    expect(screen.getByRole('button', { name: 'Save Provider' })).toHaveProperty('disabled', true);
    expect(screen.getByRole('button', { name: 'Fetch models' })).toHaveProperty('disabled', true);
  });
});
