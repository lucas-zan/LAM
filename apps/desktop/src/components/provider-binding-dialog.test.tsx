import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ProviderBindingDialog } from './provider-binding-dialog';
import type {
  ProfileAttachPlanViewV2,
  ProfileDetachPlanViewV2,
  ProfileProviderBindingViewV2,
  ProviderProfileViewV2,
} from '../lib/types';

const provider: ProviderProfileViewV2 = {
  id: 'company',
  name: 'Company',
  protocol: 'responses',
  baseUrl: 'https://company.example.test/v1',
  defaultModel: 'model-a',
  models: [
    { id: 'model-a', label: 'Model A' },
    { id: 'model-b', label: 'Model B' },
  ],
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

const attachPlan: ProfileAttachPlanViewV2 = {
  planId: 'attach-1',
  fingerprint: 'fingerprint',
  expiresAtMs: 50_000,
  profileId: 'profile-a',
  providerId: 'company',
  selectedModel: 'model-a',
  routeKind: 'direct',
  blockers: [],
  warnings: ['DNS will be revalidated at launch'],
  operations: ['backup /tmp/config.toml.bak', 'write managed projection'],
  redactedPreview: '[model_providers.company]\nenv_key = "[REFERENCE]"',
  expectedProviderStoreRevision: 2,
  expectedBindingStoreRevision: 0,
  expectedBindingRevision: null,
  sourceConfigHash: 'hash',
};

const binding: ProfileProviderBindingViewV2 = {
  profileId: 'profile-a',
  providerId: 'company',
  selectedModel: 'model-a',
  routeKind: 'direct',
  revision: 1,
  providerRevision: 2,
};

const detachPlan: ProfileDetachPlanViewV2 = {
  planId: 'detach-1',
  fingerprint: 'detach-fingerprint',
  expiresAtMs: 50_000,
  profileId: 'profile-a',
  expectedBindingStoreRevision: 1,
  expectedBindingRevision: 1,
  sourceConfigHash: 'hash',
};

describe('ProviderBindingDialog', () => {
  it('updates the live countdown and disables execution when the plan expires', () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const livePlan = { ...attachPlan, expiresAtMs: 2_500 };
    const view = render(
      <ProviderBindingDialog
        mode="attach"
        provider={provider}
        profiles={['profile-a']}
        binding={null}
        attachPlan={livePlan}
        detachPlan={null}
        recoveryMessage=""
        onPreviewAttach={vi.fn()}
        onPreviewDetach={vi.fn()}
        onExecute={vi.fn()}
        onClearPlan={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('Expires in 2 seconds')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Attach' }) as HTMLButtonElement).disabled).toBe(
      false,
    );

    act(() => vi.advanceTimersByTime(2_000));

    expect(screen.getByText('Expires in 0 seconds')).toBeTruthy();
    expect(screen.getByText('Preview expired; create a new preview')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Attach' }) as HTMLButtonElement).disabled).toBe(
      true,
    );

    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
  });

  it('requires attach preview and renders the redacted plan before execution', () => {
    const onPreviewAttach = vi.fn();
    const onExecute = vi.fn();
    const view = render(
      <ProviderBindingDialog
        mode="attach"
        provider={provider}
        profiles={['profile-a']}
        binding={null}
        attachPlan={null}
        detachPlan={null}
        recoveryMessage=""
        nowMs={1_000}
        onPreviewAttach={onPreviewAttach}
        onPreviewDetach={vi.fn()}
        onExecute={onExecute}
        onClearPlan={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect((screen.getByRole('button', { name: 'Attach' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Preview changes' }));
    expect(onPreviewAttach).toHaveBeenCalledWith({
      profileId: 'profile-a',
      providerId: 'company',
      selectedModel: 'model-a',
    });

    view.rerender(
      <ProviderBindingDialog
        mode="attach"
        provider={provider}
        profiles={['profile-a']}
        binding={null}
        attachPlan={attachPlan}
        detachPlan={null}
        recoveryMessage=""
        nowMs={1_000}
        onPreviewAttach={onPreviewAttach}
        onPreviewDetach={vi.fn()}
        onExecute={onExecute}
        onClearPlan={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText('backup /tmp/config.toml.bak')).toBeTruthy();
    expect(screen.getByText('DNS will be revalidated at launch')).toBeTruthy();
    expect(screen.getByText(/env_key =/)).toBeTruthy();
    expect(screen.getByText(/Expires in 49 seconds/)).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Attach' }) as HTMLButtonElement).disabled).toBe(
      false,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Attach' }));
    expect(onExecute).toHaveBeenCalledOnce();
  });

  it('invalidates the plan when model selection changes and labels an existing binding as rebind', () => {
    const onClearPlan = vi.fn();
    render(
      <ProviderBindingDialog
        mode="attach"
        provider={provider}
        profiles={['profile-a']}
        binding={binding}
        attachPlan={attachPlan}
        detachPlan={null}
        recoveryMessage=""
        nowMs={1_000}
        onPreviewAttach={vi.fn()}
        onPreviewDetach={vi.fn()}
        onExecute={vi.fn()}
        onClearPlan={onClearPlan}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: 'Rebind' })).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Model'), { target: { value: 'model-b' } });
    expect(onClearPlan).toHaveBeenCalledOnce();
  });

  it.each([
    [{ ...attachPlan, blockers: ['ENV_MISSING'] }, 1_000, 'ENV_MISSING'],
    [attachPlan, 50_001, 'Preview expired; create a new preview'],
  ])('disables execution for blockers or an expired ticket', (plan, nowMs, message) => {
    render(
      <ProviderBindingDialog
        mode="attach"
        provider={provider}
        profiles={['profile-a']}
        binding={null}
        attachPlan={plan}
        detachPlan={null}
        recoveryMessage=""
        nowMs={nowMs}
        onPreviewAttach={vi.fn()}
        onPreviewDetach={vi.fn()}
        onExecute={vi.fn()}
        onClearPlan={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText(message)).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Attach' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it('previews and executes detach with revision details and recovery guidance', () => {
    const onPreviewDetach = vi.fn();
    const onExecute = vi.fn();
    render(
      <ProviderBindingDialog
        mode="detach"
        provider={provider}
        profiles={['profile-a']}
        binding={binding}
        attachPlan={null}
        detachPlan={detachPlan}
        recoveryMessage="Provider or profile changed. Refresh completed; preview again."
        nowMs={1_000}
        onPreviewAttach={vi.fn()}
        onPreviewDetach={onPreviewDetach}
        onExecute={onExecute}
        onClearPlan={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('Binding revision 1')).toBeTruthy();
    expect(screen.getByText(/preview again/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Preview detach' }));
    expect(onPreviewDetach).toHaveBeenCalledWith('profile-a');
    fireEvent.click(screen.getByRole('button', { name: 'Detach' }));
    expect(onExecute).toHaveBeenCalledOnce();
  });
});
