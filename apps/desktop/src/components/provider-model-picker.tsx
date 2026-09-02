import { useMemo, useState } from 'react';
import type { ProviderModelV2 } from '../lib/types';
import { formatProviderError } from '../lib/format';
import { sameModelIdSet } from '../lib/provider-models';
import { UIButton } from './ui-button';

type ModelApplyMode = 'replace' | 'customize';

function SavedModelAllowlist({
  models,
  defaultModelId,
}: {
  models: ProviderModelV2[];
  defaultModelId: string;
}) {
  if (!models.length) {
    return (
      <p className="apiHelperText">No models saved yet. Fetch models to build an allowlist.</p>
    );
  }
  return (
    <ul className="apiModelAllowlist" aria-label="Saved models">
      {models.map((model) => {
        const isDefault = model.id === defaultModelId;
        return (
          <li
            key={model.id}
            className={`apiModelAllowlistItem${isDefault ? ' isDefault' : ''}`}
          >
            <div className="apiModelAllowlistMeta">
              <span className="apiModelAllowlistId">{model.id}</span>
              {model.label && model.label !== model.id ? (
                <span className="apiModelAllowlistLabel">{model.label}</span>
              ) : null}
            </div>
            {isDefault ? <span className="apiModelDefaultBadge">Default</span> : null}
          </li>
        );
      })}
    </ul>
  );
}

export function ProviderModelPicker({
  savedModels,
  selectedModelId,
  onRefreshModels,
  onApply,
  applying = false,
}: {
  savedModels: ProviderModelV2[];
  selectedModelId: string;
  onRefreshModels: () => Promise<ProviderModelV2[]>;
  onApply: (models: ProviderModelV2[], selectedModel: string) => Promise<void> | void;
  applying?: boolean;
}) {
  const [refreshingModels, setRefreshingModels] = useState(false);
  const [fetchedModels, setFetchedModels] = useState<ProviderModelV2[]>([]);
  const [applyMode, setApplyMode] = useState<ModelApplyMode | null>(null);
  const [draftModelIds, setDraftModelIds] = useState<string[]>([]);
  const [draftSelectedModel, setDraftSelectedModel] = useState('');
  const [error, setError] = useState('');

  const fetchedMatchesSaved =
    fetchedModels.length > 0 && sameModelIdSet(fetchedModels, savedModels);

  const checklistModels = useMemo(() => {
    const byId = new Map<string, ProviderModelV2>();
    for (const model of savedModels) byId.set(model.id, model);
    for (const model of fetchedModels) byId.set(model.id, model);
    return [...byId.values()];
  }, [fetchedModels, savedModels]);

  const candidateModels = useMemo(() => {
    if (applyMode === 'replace') return fetchedModels;
    if (applyMode === 'customize') {
      return checklistModels.filter((model) => draftModelIds.includes(model.id));
    }
    return [];
  }, [applyMode, checklistModels, draftModelIds, fetchedModels]);

  const needsDefaultPick =
    candidateModels.length > 0 &&
    !candidateModels.some((model) => model.id === draftSelectedModel);

  function beginReplaceAll() {
    setApplyMode('replace');
    setDraftModelIds(fetchedModels.map((model) => model.id));
    setDraftSelectedModel(
      fetchedModels.some((model) => model.id === selectedModelId) ? selectedModelId : '',
    );
    setError('');
  }

  function beginCustomize() {
    setApplyMode('customize');
    setDraftModelIds(savedModels.map((model) => model.id));
    setDraftSelectedModel(
      savedModels.some((model) => model.id === selectedModelId) ? selectedModelId : '',
    );
    setError('');
  }

  function toggleDraftModel(modelId: string) {
    setDraftModelIds((current) => {
      const removing = current.includes(modelId);
      const next = removing ? current.filter((id) => id !== modelId) : [...current, modelId];
      if (removing && draftSelectedModel === modelId) setDraftSelectedModel('');
      return next;
    });
  }

  async function applyModels() {
    setError('');
    if (!candidateModels.length) {
      setError('Select at least one model before applying');
      return;
    }
    if (needsDefaultPick || !draftSelectedModel.trim()) {
      setError('Choose a default model from the new allowlist');
      return;
    }
    if (!candidateModels.some((model) => model.id === draftSelectedModel)) {
      setError('Default model must be present in the selected models');
      return;
    }
    try {
      await onApply(candidateModels, draftSelectedModel.trim());
      setApplyMode(null);
      setDraftModelIds([]);
      setDraftSelectedModel('');
      setFetchedModels([]);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not apply models');
    }
  }

  return (
    <div className="apiAccountModels">
      <div className="panelHead">
        <div>
          <strong>Selected models</strong>
          <p className="statusHint">Only these saved models are shown by Codex /model.</p>
        </div>
        <UIButton
          type="button"
          size="sm"
          disabled={refreshingModels}
          onClick={async () => {
            setRefreshingModels(true);
            setError('');
            setApplyMode(null);
            setDraftModelIds([]);
            setDraftSelectedModel('');
            try {
              const models = await onRefreshModels();
              setFetchedModels(models);
              if (!models.length) setError('No models were returned');
            } catch (reason) {
              setFetchedModels([]);
              setError(formatProviderError(reason));
            } finally {
              setRefreshingModels(false);
            }
          }}
        >
          {refreshingModels ? 'Fetching…' : 'Fetch models'}
        </UIButton>
      </div>
      <SavedModelAllowlist models={savedModels} defaultModelId={selectedModelId} />
      {fetchedModels.length ? (
        <div className="apiFetchedModels">
          <strong>Fetched models</strong>
          <p className="statusHint">Live result from the standard /models endpoint.</p>
          <div className="apiModelChecklistViewport">
            <ul className="apiModelAllowlist apiModelAllowlist--fetched" aria-label="Fetched models">
              {fetchedModels.map((model) => (
                <li key={model.id} className="apiModelAllowlistItem">
                  <div className="apiModelAllowlistMeta">
                    <span className="apiModelAllowlistId">{model.id}</span>
                    {model.label && model.label !== model.id ? (
                      <span className="apiModelAllowlistLabel">{model.label}</span>
                    ) : null}
                  </div>
                </li>
              ))}
            </ul>
          </div>
          {fetchedMatchesSaved ? (
            <p className="statusHint" role="status">
              Fetched models match the current allowlist.
            </p>
          ) : (
            <div className="apiModelApplyActions">
              <UIButton type="button" size="sm" onClick={beginReplaceAll}>
                Replace all
              </UIButton>
              <UIButton type="button" size="sm" onClick={beginCustomize}>
                Customize selection
              </UIButton>
            </div>
          )}
        </div>
      ) : null}
      {applyMode && !fetchedMatchesSaved ? (
        <div className="apiModelApplyPanel">
          {applyMode === 'customize' ? (
            <fieldset className="apiModelFieldset">
              <legend>Select active models</legend>
              <div className="apiModelChecklistViewport">
                <div className="apiModelChecklist">
                  {checklistModels.map((model) => {
                    const active = draftModelIds.includes(model.id);
                    return (
                      <label
                        key={model.id}
                        className={`apiModelCheckItem ${active ? 'active' : ''}`}
                      >
                        <input
                          type="checkbox"
                          aria-label={`Select model ${model.id}`}
                          checked={active}
                          onChange={() => toggleDraftModel(model.id)}
                        />
                        <span>{model.label || model.id}</span>
                      </label>
                    );
                  })}
                </div>
              </div>
            </fieldset>
          ) : (
            <p className="statusHint">
              Replace the saved allowlist with all {fetchedModels.length} fetched models.
            </p>
          )}
          {candidateModels.length && (needsDefaultPick || draftSelectedModel) ? (
            <label>
              Default model
              <select
                aria-label="Default model for allowlist"
                value={needsDefaultPick ? '' : draftSelectedModel}
                onChange={(event) => setDraftSelectedModel(event.target.value)}
              >
                {needsDefaultPick ? <option value="">Select a default model</option> : null}
                {candidateModels.map((model) => (
                  <option key={model.id} value={model.id}>
                    {model.label || model.id}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          <div className="apiModelApplyActions">
            <UIButton
              type="button"
              variant="primary"
              size="sm"
              disabled={applying || !candidateModels.length || needsDefaultPick}
              onClick={() => void applyModels()}
            >
              {applying ? 'Applying…' : 'Apply models'}
            </UIButton>
          </div>
        </div>
      ) : null}
      {error ? (
        <div className="notice" role="alert">
          {error}
        </div>
      ) : null}
    </div>
  );
}
