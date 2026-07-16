import { useRef, useState, type FormEvent } from 'react';
import type {
  ApiAccountPlanViewV2,
  PlanApiAccountRequestV2,
  ProviderProfileViewV2,
  ProviderProtocolV2,
} from '../lib/types';
import * as api from '../lib/api';
import { UIButton } from './ui-button';

export function ApiAccountFlow({
  providers,
  onCreated,
  onCancel,
}: {
  providers: ProviderProfileViewV2[];
  onCreated: (profileId: string) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [accountName, setAccountName] = useState('');
  const [reuse, setReuse] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [existingProviderId, setExistingProviderId] = useState(providers[0]?.id ?? '');
  const [protocol, setProtocol] = useState<ProviderProtocolV2>('responses');
  const [baseUrl, setBaseUrl] = useState('');
  const [selectedModel, setSelectedModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [compatibilityProfile, setCompatibilityProfile] = useState('openai_chat_completions');
  const [plan, setPlan] = useState<ApiAccountPlanViewV2 | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [selectedModels, setSelectedModels] = useState<string[]>([]);
  const [customModelInput, setCustomModelInput] = useState('');
  const [discoveredModels, setDiscoveredModels] = useState<Array<{ id: string; label: string }>>(
    [],
  );
  const [discovering, setDiscovering] = useState(false);
  const discoveryGeneration = useRef(0);

  const existingProvider = providers.find((provider) => provider.id === existingProviderId);

  function resetDiscovery() {
    discoveryGeneration.current += 1;
    setDiscovering(false);
    setDiscoveredModels([]);
    setSelectedModels([]);
    setSelectedModel('');
    setCustomModelInput('');
    setPlan(null);
    setError('');
  }

  async function fetchModels() {
    const url = baseUrl.trim();
    const key = apiKey.trim();
    if (!url || !key) {
      setError('Base URL and API key are required to fetch models');
      return;
    }
    const generation = discoveryGeneration.current + 1;
    discoveryGeneration.current = generation;
    setDiscovering(true);
    setDiscoveredModels([]);
    setSelectedModels([]);
    setSelectedModel('');
    setPlan(null);
    setError('');
    try {
      const result = await api.discoverProviderModelsV2({ baseUrl: url, apiKey: key });
      if (generation !== discoveryGeneration.current) return;
      if (!result.models.length) throw new Error('No models were returned');
      setDiscoveredModels(result.models);
    } catch (value) {
      if (generation !== discoveryGeneration.current) return;
      const message = value instanceof Error ? value.message : 'Could not fetch models';
      setApiKey('');
      setError(`${message}. Add a custom model manually.`);
    } finally {
      if (generation === discoveryGeneration.current) setDiscovering(false);
    }
  }

  function toggleModel(modelName: string) {
    const removing = selectedModels.includes(modelName);
    const nextList = removing
      ? selectedModels.filter((model) => model !== modelName)
      : [...selectedModels, modelName];
    setSelectedModels(nextList);
    if (removing && selectedModel === modelName) setSelectedModel('');
    setPlan(null);
  }

  function addCustomModel() {
    const modelName = customModelInput.trim();
    if (modelName && !selectedModels.includes(modelName)) {
      const nextList = [...selectedModels, modelName];
      setSelectedModels(nextList);
      setPlan(null);
      setCustomModelInput('');
    }
  }

  function buildRequest(): PlanApiAccountRequestV2 {
    if (reuse) {
      if (!existingProvider) throw new Error('Select an existing Provider');
      return {
        accountName: accountName.trim(),
        selectedModel: selectedModel || existingProvider.defaultModel,
        overwriteWrapper: false,
        provider: { kind: 'existing', providerId: existingProvider.id },
      };
    }
    const name = accountName.trim();
    const selectedModelsList = selectedModels;
    if (!name || !baseUrl.trim() || !selectedModel.trim() || !selectedModelsList.length) {
      throw new Error('Account name, Base URL, models and default model are required');
    }
    if (!selectedModelsList.includes(selectedModel.trim())) {
      throw new Error('Default model must be present in Models');
    }
    if (!apiKey.trim()) throw new Error('API key is required');
    const adapterRequired = protocol === 'chat_completions';
    return {
      accountName: name,
      selectedModel: selectedModel.trim(),
      overwriteWrapper: false,
      provider: {
        kind: 'new',
        provider: {
          id: `account-${name}`,
          name: `${name} API connection`,
          protocol,
          baseUrl: baseUrl.trim(),
          defaultModel: selectedModel.trim(),
          models: selectedModelsList.map((id) => ({ id, label: id })),
          upstreamAuth: { kind: 'bearer', credential: { kind: 'none' } },
          adapter: adapterRequired
            ? {
                kind: 'local',
                adapterId: 'responses_to_chat_completions',
                upstreamPath: '/chat/completions',
              }
            : { kind: 'none' },
          compatibilityProfile: adapterRequired ? compatibilityProfile : undefined,
          codex: {
            streamIdleTimeoutMs: 300_000,
            directRequestMaxRetries: 0,
            directStreamMaxRetries: 0,
            routeViaGateway: true,
            queryParams: {},
            envHttpHeaders: {},
          },
        },
      },
    };
  }

  async function review(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError('');
    try {
      setPlan(await api.planApiAccountV2(buildRequest()));
    } catch (value) {
      setError(value instanceof Error ? value.message : 'Could not plan API Account');
    } finally {
      setBusy(false);
    }
  }

  async function create() {
    if (!plan || plan.blockers.length) return;
    setBusy(true);
    setError('');
    try {
      const result = await api.executeApiAccountV2({
        planId: plan.planId,
        fingerprint: plan.fingerprint,
        keychainSecret: reuse ? null : apiKey,
      });
      setApiKey('');
      setPlan(null);
      await onCreated(result.account.profileId);
    } catch (value) {
      setPlan(null);
      setError(value instanceof Error ? value.message : 'Could not create API Account');
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={review} noValidate className="apiFlowContainer">
      <button
        type="button"
        className="apiAdvancedToggle"
        aria-expanded={advancedOpen}
        onClick={() => setAdvancedOpen((open) => !open)}
      >
        <span>Advanced options</span>
        <span aria-hidden>{advancedOpen ? '−' : '+'}</span>
      </button>

      {advancedOpen ? (
        <div className="apiAdvancedPanel">
          <label className="apiReuseOption">
            <input
              aria-label="Reuse existing Provider"
              type="checkbox"
              checked={reuse}
              disabled={providers.length === 0}
              onChange={(event) => {
                const checked = event.target.checked;
                setReuse(checked);
                resetDiscovery();
                if (checked && providers[0]) {
                  setExistingProviderId(providers[0].id);
                  setSelectedModel(providers[0].defaultModel);
                }
              }}
            />
            <span>
              <strong>Reuse an existing connection</strong>
              <small>Use a Provider connection that is already managed by LAM.</small>
            </span>
          </label>
          {providers.length === 0 ? (
            <p className="apiAdvancedEmpty">No existing connections are available to reuse.</p>
          ) : null}
        </div>
      ) : null}

      <div className="apiFlowGrid">
        <div className="apiFormSection">
          <div className="apiFormSectionHeader">
            <h3 className="apiFormSectionTitle" aria-label="1 Connection details">
              <span className="apiStepBadge" aria-hidden>
                1
              </span>
              <span>Connection details</span>
            </h3>
            <p>Name the account and authenticate the upstream endpoint.</p>
          </div>
          <label>
            Account name
            <input
              aria-label="Account name"
              value={accountName}
              onChange={(event) => {
                setAccountName(event.target.value);
                setPlan(null);
              }}
              placeholder="work-api"
              autoCapitalize="none"
              autoCorrect="off"
              required
            />
          </label>

          {reuse ? (
            <label>
              Existing Provider
              <select
                aria-label="Existing Provider"
                value={existingProviderId}
                onChange={(event) => {
                  const provider = providers.find((item) => item.id === event.target.value);
                  setExistingProviderId(event.target.value);
                  setSelectedModel(provider?.defaultModel ?? '');
                  setPlan(null);
                }}
              >
                {providers.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.name}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <>
              <label>
                API type
                <select
                  aria-label="API type"
                  value={protocol}
                  onChange={(event) => {
                    setProtocol(event.target.value as ProviderProtocolV2);
                    setPlan(null);
                  }}
                >
                  <option value="responses">Responses</option>
                  <option value="chat_completions">Chat Completions</option>
                </select>
              </label>
              <label>
                Base URL
                <div className="apiBaseUrlRow">
                  <input
                    aria-label="Base URL"
                    value={baseUrl}
                    onChange={(event) => {
                      setBaseUrl(event.target.value);
                      resetDiscovery();
                    }}
                    placeholder="https://api.openai.com/v1"
                    autoCapitalize="none"
                    autoCorrect="off"
                    required
                  />
                  <input
                    aria-label="Endpoint suffix"
                    className="apiEndpointSuffix"
                    value={protocol === 'responses' ? '/responses' : '/chat/completions'}
                    readOnly
                    tabIndex={-1}
                  />
                </div>
              </label>
              <label>
                API key
                <div className="apiInputWrapper">
                  <input
                    aria-label="API key"
                    type={showApiKey ? 'text' : 'password'}
                    autoComplete="off"
                    value={apiKey}
                    onChange={(event) => {
                      setApiKey(event.target.value);
                      resetDiscovery();
                    }}
                    autoCapitalize="none"
                    autoCorrect="off"
                    required
                  />
                  <button
                    type="button"
                    className="apiInputToggleBtn"
                    onClick={() => setShowApiKey(!showApiKey)}
                    title={showApiKey ? 'Hide API key' : 'Show API key'}
                  >
                    {showApiKey ? (
                      <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      >
                        <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"></path>
                        <line x1="1" y1="1" x2="23" y2="23"></line>
                      </svg>
                    ) : (
                      <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      >
                        <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path>
                        <circle cx="12" cy="12" r="3"></circle>
                      </svg>
                    )}
                  </button>
                </div>
              </label>
              {protocol === 'chat_completions' ? (
                <label>
                  Compatibility
                  <select
                    aria-label="Compatibility"
                    value={compatibilityProfile}
                    onChange={(event) => {
                      setCompatibilityProfile(event.target.value);
                      setPlan(null);
                    }}
                  >
                    <option value="openai_chat_completions">OpenAI-compatible</option>
                    <option value="deepseek_chat_completions">DeepSeek</option>
                  </select>
                </label>
              ) : null}
            </>
          )}
        </div>

        <div className="apiFormSection">
          <div className="apiFormSectionHeader">
            <h3 className="apiFormSectionTitle" aria-label="2 Model configuration">
              <span className="apiStepBadge" aria-hidden>
                2
              </span>
              <span>Model configuration</span>
            </h3>
            <p>Discover or add allowed models, then choose the default.</p>
          </div>
          {reuse ? (
            <div
              style={{
                padding: '8px 0 16px',
                color: 'var(--muted)',
                fontSize: '12px',
                lineHeight: '1.4',
              }}
            >
              Using the connection's models context. Select the default active model below.
            </div>
          ) : (
            <>
              <div className="apiModelDiscoveryRow">
                <UIButton
                  type="button"
                  size="sm"
                  className="apiFetchModelsBtn"
                  onClick={() => void fetchModels()}
                  disabled={discovering || !baseUrl.trim() || !apiKey.trim()}
                >
                  {discovering ? 'Fetching models…' : 'Fetch models'}
                </UIButton>
                <span>Reads the standard OpenAI /models endpoint</span>
              </div>

              {discoveredModels.length > 0 ? (
                <fieldset className="apiModelFieldset">
                  <legend>Select active models</legend>
                  <div className="apiModelChecklistViewport">
                    <div className="apiModelChecklist">
                      {discoveredModels.map((model) => {
                        const active = selectedModels.includes(model.id);
                        return (
                          <label
                            key={model.id}
                            className={`apiModelCheckItem ${active ? 'active' : ''}`}
                          >
                            <input
                              type="checkbox"
                              aria-label={`Select model ${model.id}`}
                              checked={active}
                              onChange={() => toggleModel(model.id)}
                            />
                            <span>{model.label}</span>
                          </label>
                        );
                      })}
                    </div>
                  </div>
                </fieldset>
              ) : (
                <div className="apiHelperText">
                  Fetch models from the API, or add a custom model below.
                </div>
              )}

              {selectedModels.length > 0 && (
                <div className="apiSelectedTagsRow">
                  {selectedModels.map((modelName) => (
                    <span key={modelName} className="apiSelectedTag">
                      {modelName}
                      <button
                        type="button"
                        className="apiSelectedTagRemove"
                        onClick={() => toggleModel(modelName)}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                </div>
              )}

              <div className="apiCustomModelInputGroup">
                <input
                  type="text"
                  aria-label="Custom model"
                  placeholder="Add custom model name..."
                  value={customModelInput}
                  onChange={(event) => setCustomModelInput(event.target.value)}
                  autoCapitalize="none"
                  autoCorrect="off"
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      addCustomModel();
                    }
                  }}
                />
                <button
                  type="button"
                  aria-label="Add custom model"
                  onClick={addCustomModel}
                  disabled={!customModelInput.trim()}
                >
                  Add custom model
                </button>
              </div>
            </>
          )}

          <label style={{ marginTop: '8px' }}>
            Default model
            {reuse && existingProvider ? (
              <select
                aria-label="Default model"
                value={selectedModel}
                onChange={(event) => {
                  setSelectedModel(event.target.value);
                  setPlan(null);
                }}
              >
                {existingProvider.models.map((model) => (
                  <option key={model.id} value={model.id}>
                    {model.label}
                  </option>
                ))}
              </select>
            ) : selectedModels.length > 0 ? (
              <select
                aria-label="Default model"
                value={selectedModel}
                onChange={(event) => {
                  setSelectedModel(event.target.value);
                  setPlan(null);
                }}
                required
              >
                <option value="">Select Default Model</option>
                {selectedModels.map((modelId) => (
                  <option key={modelId} value={modelId}>
                    {modelId}
                  </option>
                ))}
              </select>
            ) : (
              <input
                aria-label="Default model"
                value={selectedModel}
                onChange={(event) => {
                  setSelectedModel(event.target.value);
                  setPlan(null);
                }}
                placeholder="e.g. provider-model-id"
                required
              />
            )}
          </label>
        </div>
      </div>

      {error ? <div className="errorBanner">{error}</div> : null}

      {plan ? (
        <div className="apiVisualPlan" aria-label="API Account plan">
          <div className="apiVisualPlanHeader">
            <span>Proposed Account Plan</span>
            {plan.blockers.length > 0 && (
              <span style={{ color: 'var(--red)', fontWeight: 'bold' }}>Blocked</span>
            )}
          </div>
          <div className="apiVisualPlanContent">
            <div className="apiVisualPlanRow">
              <span className="apiVisualPlanLabel">CODEX_HOME</span>
              <span className="apiVisualPlanValue">~/.codex-{plan.accountName}</span>
            </div>
            <div className="apiVisualPlanRow">
              <span className="apiVisualPlanLabel">Provider ID</span>
              <span className="apiVisualPlanValue">{plan.providerId}</span>
            </div>
            <div className="apiVisualPlanRow">
              <span className="apiVisualPlanLabel">Default Model</span>
              <span className="apiVisualPlanValue">{plan.selectedModel}</span>
            </div>
            {plan.blockers.map((blocker) => (
              <div
                key={blocker}
                className="statusHint"
                style={{
                  color: 'var(--red)',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '4px',
                  marginTop: '4px',
                }}
              >
                <span>⚠️</span>
                <span>{blocker}</span>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      <div className="modalFoot">
        <UIButton type="button" variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </UIButton>
        <div className="modalFootPrimary">
          <UIButton type="submit" disabled={busy}>
            Review API Account
          </UIButton>
          <UIButton
            type="button"
            variant="primary"
            disabled={busy || !plan || Boolean(plan.blockers.length)}
            onClick={() => void create()}
          >
            Create API Account
          </UIButton>
        </div>
      </div>
    </form>
  );
}
