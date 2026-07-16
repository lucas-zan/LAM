import { useState } from 'react';
import type {
  PlanAttachRequestV2,
  ProfileAttachPlanViewV2,
  ProfileDetachPlanViewV2,
  ProfileProviderBindingViewV2,
  ProviderProfileViewV2,
} from '../lib/types';
import { UIButton } from './ui-button';

export function ProviderBindingDialog({
  mode,
  provider,
  profiles,
  binding,
  attachPlan,
  detachPlan,
  recoveryMessage,
  nowMs,
  onPreviewAttach,
  onPreviewDetach,
  onExecute,
  onClearPlan,
  onClose,
}: {
  mode: 'attach' | 'detach';
  provider: ProviderProfileViewV2;
  profiles: string[];
  binding: ProfileProviderBindingViewV2 | null;
  attachPlan: ProfileAttachPlanViewV2 | null;
  detachPlan: ProfileDetachPlanViewV2 | null;
  recoveryMessage: string;
  nowMs?: number;
  onPreviewAttach: (request: PlanAttachRequestV2) => void | Promise<unknown>;
  onPreviewDetach: (profileId: string) => void | Promise<unknown>;
  onExecute: () => void | Promise<void>;
  onClearPlan: () => void;
  onClose: () => void;
}) {
  const [openedAt] = useState(() => Date.now());
  const effectiveNow = nowMs ?? openedAt;
  const [profileId, setProfileId] = useState(binding?.profileId ?? profiles[0] ?? '');
  const [selectedModel, setSelectedModel] = useState(
    binding?.selectedModel ?? provider.defaultModel,
  );
  const plan = mode === 'attach' ? attachPlan : detachPlan;
  const expired = Boolean(plan && plan.expiresAtMs <= effectiveNow);
  const attachMatches =
    attachPlan?.profileId === profileId &&
    attachPlan.providerId === provider.id &&
    attachPlan.selectedModel === selectedModel;
  const blocked = mode === 'attach' && Boolean(attachPlan?.blockers.length);
  const canExecute = Boolean(plan) && !expired && !blocked && (mode === 'detach' || attachMatches);
  const secondsRemaining = plan
    ? Math.max(0, Math.ceil((plan.expiresAtMs - effectiveNow) / 1000))
    : 0;
  const rebind = Boolean(binding);

  function changeProfile(nextProfileId: string) {
    setProfileId(nextProfileId);
    onClearPlan();
  }

  function changeModel(nextModel: string) {
    setSelectedModel(nextModel);
    onClearPlan();
  }

  return (
    <div>
      {mode === 'attach' ? (
        <div className="formGrid">
          <label>
            Profile
            <select value={profileId} onChange={(event) => changeProfile(event.target.value)}>
              {profiles.map((profile) => (
                <option key={profile} value={profile}>
                  {profile}
                </option>
              ))}
            </select>
          </label>
          <label>
            Provider
            <input value={provider.id} disabled />
          </label>
          <label>
            Model
            <select value={selectedModel} onChange={(event) => changeModel(event.target.value)}>
              {provider.models.map((model) => (
                <option key={model.id} value={model.id}>
                  {model.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      ) : (
        <div className="previewBox">
          <div className="previewLine">
            <span>Profile</span>
            <strong>{profileId}</strong>
          </div>
          <div className="previewLine">
            <span>Provider</span>
            <strong>{provider.id}</strong>
          </div>
        </div>
      )}

      {recoveryMessage ? (
        <div className="notice" role="status">
          {recoveryMessage}
        </div>
      ) : null}

      {plan ? (
        <div className="providerPlan">
          <div className="previewLine">
            <span>Plan</span>
            <strong>Expires in {secondsRemaining} seconds</strong>
          </div>
          {expired ? (
            <div className="notice" role="alert">
              Preview expired; create a new preview
            </div>
          ) : null}
          {mode === 'attach' && attachPlan ? (
            <>
              {attachPlan.blockers.map((blocker) => (
                <div className="notice" role="alert" key={blocker}>
                  {blocker}
                </div>
              ))}
              {attachPlan.warnings.map((warning) => (
                <div className="notice" role="status" key={warning}>
                  {warning}
                </div>
              ))}
              <div className="previewBox">
                {attachPlan.operations.map((operation) => (
                  <div className="previewLine" key={operation}>
                    <span>Operation</span>
                    <strong>{operation}</strong>
                  </div>
                ))}
              </div>
              <pre className="previewBox mono">{attachPlan.redactedPreview}</pre>
            </>
          ) : null}
          {mode === 'detach' && detachPlan ? (
            <div className="previewBox">
              <div className="previewLine">
                <span>Expected state</span>
                <strong>Binding revision {detachPlan.expectedBindingRevision}</strong>
              </div>
              <div className="previewLine">
                <span>Config safety</span>
                <strong>Managed projection only; unmanaged content is preserved</strong>
              </div>
            </div>
          ) : null}
        </div>
      ) : (
        <div className="notice">Preview is required before execution.</div>
      )}

      <div className="modalFoot">
        <UIButton type="button" variant="ghost" onClick={onClose}>
          Cancel
        </UIButton>
        <div className="modalFootPrimary">
          <UIButton
            type="button"
            onClick={() =>
              mode === 'attach'
                ? onPreviewAttach({ profileId, providerId: provider.id, selectedModel })
                : onPreviewDetach(profileId)
            }
          >
            {mode === 'attach' ? 'Preview changes' : 'Preview detach'}
          </UIButton>
          <UIButton
            type="button"
            variant={mode === 'detach' ? 'danger' : 'primary'}
            disabled={!canExecute}
            onClick={onExecute}
          >
            {mode === 'detach' ? 'Detach' : rebind ? 'Rebind' : 'Attach'}
          </UIButton>
        </div>
      </div>
    </div>
  );
}
