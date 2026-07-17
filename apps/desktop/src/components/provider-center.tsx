import { useMemo, useState, type FormEvent } from 'react';
import type {
  ApiAccountConnectionViewV2,
  CredentialReferenceV2,
  ProfileProviderBindingViewV2,
  ProviderDefinitionV2,
  ProviderProfileViewV2,
  UpstreamAuthV2,
  UpdateApiAccountConnectionRequestV2,
} from '../lib/types';
import { UIButton } from './ui-button';
import { Modal } from './shell';
import { ProviderBindingDialog } from './provider-binding-dialog';
import { useProviderStore } from '../stores/providers';

type CredentialKind = CredentialReferenceV2['kind'];
type AuthKind = UpstreamAuthV2['kind'];
type AuthCommandDraft = { executable: string; args: string[] };

function credentialLabel(auth: UpstreamAuthV2): string {
  if (auth.kind === 'none') return 'None';
  const credential = auth.credential;
  if (credential.kind === 'env') return `Env · ${credential.envKey}`;
  if (credential.kind === 'keychain') {
    return `Keychain · v${credential.version}`;
  }
  if (credential.kind === 'auth_command') {
    return `Auth command · ${credential.approvalId}`;
  }
  if (credential.kind === 'codex_profile') {
    return `Codex login · ${credential.profileId}`;
  }
  return 'Missing credential';
}

function protocolLabel(protocol: ProviderProfileViewV2['protocol']): string {
  return protocol === 'responses' ? 'Responses' : 'Chat Completions';
}

export function ProviderCards({
  providers,
  bindings,
  onEdit,
  onAttach,
  onDetach,
  onTest,
}: {
  providers: ProviderProfileViewV2[];
  bindings: ProfileProviderBindingViewV2[];
  onEdit: (provider: ProviderProfileViewV2) => void;
  onAttach: (provider: ProviderProfileViewV2) => void;
  onDetach: (binding: ProfileProviderBindingViewV2) => void;
  onTest: (provider: ProviderProfileViewV2) => void;
}) {
  if (!providers.length) {
    return (
      <div className="emptyState">
        <strong>No API connections yet</strong>
        <p>Add an External API account to create its connection automatically.</p>
      </div>
    );
  }

  return (
    <div className="cardGrid">
      {providers.map((provider) => {
        const providerBindings = bindings.filter((binding) => binding.providerId === provider.id);
        const attachable = provider.readiness?.ready ?? provider.readinessBlockers.length === 0;
        return (
          <article className="card" key={provider.id}>
            <div className="cardHead">
              <h3>{provider.name}</h3>
              <span className="badge">{protocolLabel(provider.protocol)}</span>
            </div>
            <p className="mono cardPath">{provider.baseUrl}</p>
            <div className="kv">
              <span>Provider ID</span>
              <strong>{provider.id}</strong>
              <span>Default model</span>
              <strong>{provider.defaultModel}</strong>
              <span>Credential</span>
              <strong>{credentialLabel(provider.upstreamAuth)}</strong>
              <span>Usage</span>
              <strong>
                {providerBindings.length} {providerBindings.length === 1 ? 'binding' : 'bindings'}
              </strong>
            </div>
            {provider.readinessBlockers.length ? (
              <div className="notice providerBlockers" role="status">
                {provider.readinessBlockers.map((blocker) => (
                  <span key={blocker}>{blocker}</span>
                ))}
              </div>
            ) : null}
            {provider.protocol === 'chat_completions' && attachable ? (
              <p className="statusHint">Gateway adapter route ready</p>
            ) : null}
            <div className="cardActions">
              <UIButton size="sm" onClick={() => onTest(provider)}>
                Test upstream
              </UIButton>
              <UIButton size="sm" onClick={() => onEdit(provider)}>
                Edit
              </UIButton>
              <UIButton size="sm" disabled={!attachable} onClick={() => onAttach(provider)}>
                Attach
              </UIButton>
              {providerBindings.map((binding) => (
                <UIButton
                  key={binding.profileId}
                  size="sm"
                  variant="danger"
                  onClick={() => onDetach(binding)}
                >
                  Detach {binding.profileId}
                </UIButton>
              ))}
            </div>
          </article>
        );
      })}
    </div>
  );
}

function credentialFromView(provider: ProviderProfileViewV2 | null): CredentialReferenceV2 {
  const auth = provider?.upstreamAuth;
  if (!auth || auth.kind === 'none') return { kind: 'env', envKey: 'COMPANY_PROXY_API_KEY' };
  return auth.credential;
}

function parseModels(value: string) {
  return [
    ...new Set(
      value
        .split(/[\n,]/)
        .map((model) => model.trim())
        .filter(Boolean),
    ),
  ].map((id) => ({ id, label: id }));
}

function parsePairs(value: string): Record<string, string> {
  return Object.fromEntries(
    value
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const separator = line.indexOf('=');
        return separator < 1
          ? [line, '']
          : [line.slice(0, separator).trim(), line.slice(separator + 1).trim()];
      }),
  );
}

export function ProviderEditor({
  provider,
  onSave,
  onCancel,
}: {
  provider: ProviderProfileViewV2 | null;
  onSave: (
    provider: ProviderDefinitionV2,
    keychainSecret: string,
    authCommand: AuthCommandDraft | null,
  ) => Promise<void>;
  onCancel: () => void;
}) {
  const initialCredential = credentialFromView(provider);
  const initialAuth = provider?.upstreamAuth.kind ?? 'bearer';
  const [id, setId] = useState(provider?.id ?? 'company-proxy');
  const [name, setName] = useState(provider?.name ?? 'Company Proxy');
  const [protocol, setProtocol] = useState<ProviderDefinitionV2['protocol']>(
    provider?.protocol ?? 'responses',
  );
  const [upstreamPath, setUpstreamPath] = useState(
    provider?.adapter.kind === 'local' ? provider.adapter.upstreamPath : '/chat/completions',
  );
  const [compatibilityProfile, setCompatibilityProfile] = useState(
    provider?.compatibilityProfile ?? 'openai_chat_completions',
  );
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? 'https://proxy.example.test/v1');
  const [modelsText, setModelsText] = useState(
    provider?.models.map((model) => model.id).join(', ') ?? 'gpt-5-codex',
  );
  const [defaultModel, setDefaultModel] = useState(provider?.defaultModel ?? 'gpt-5-codex');
  const [authKind, setAuthKind] = useState<AuthKind>(initialAuth);
  const [credentialKind, setCredentialKind] = useState<CredentialKind>(initialCredential.kind);
  const [envKey, setEnvKey] = useState(
    initialCredential.kind === 'env' ? initialCredential.envKey : '',
  );
  const [approvalId] = useState(
    initialCredential.kind === 'auth_command' ? initialCredential.approvalId : '',
  );
  const [authCommandExecutable, setAuthCommandExecutable] = useState('');
  const [authCommandArgs, setAuthCommandArgs] = useState('');
  const [headerName, setHeaderName] = useState(
    provider?.upstreamAuth.kind === 'header' ? provider.upstreamAuth.name : 'Authorization',
  );
  const [keychainSecret, setKeychainSecret] = useState('');
  const [streamIdleTimeout, setStreamIdleTimeout] = useState(
    String(provider?.codex.streamIdleTimeoutMs ?? 300_000),
  );
  const [requestRetries, setRequestRetries] = useState(
    String(provider?.codex.directRequestMaxRetries ?? 0),
  );
  const [streamRetries, setStreamRetries] = useState(
    String(provider?.codex.directStreamMaxRetries ?? 0),
  );
  const [queryParams, setQueryParams] = useState(
    Object.entries(provider?.codex.queryParams ?? {})
      .map(([key, value]) => `${key}=${value}`)
      .join('\\n'),
  );
  const [envHeaders, setEnvHeaders] = useState(
    Object.entries(provider?.codex.envHttpHeaders ?? {})
      .map(([key, value]) => `${key}=${value}`)
      .join('\\n'),
  );
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  const editing = Boolean(provider);
  const parsedModels = useMemo(() => parseModels(modelsText), [modelsText]);

  function selectedCredential(): CredentialReferenceV2 {
    if (credentialKind === 'env') return { kind: 'env', envKey: envKey.trim() };
    if (credentialKind === 'auth_command') {
      return { kind: 'auth_command', approvalId: approvalId.trim() };
    }
    if (credentialKind === 'keychain') {
      if (initialCredential.kind === 'keychain') return initialCredential;
      return { kind: 'none' };
    }
    return { kind: 'none' };
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError('');
    let normalizedUrl: string;
    try {
      const url = new URL(baseUrl.trim());
      if (url.protocol !== 'https:' || url.username || url.password) throw new Error('invalid');
      normalizedUrl = url.toString().replace(/\/$/, '');
    } catch {
      setError('Enter a valid HTTPS Provider URL');
      return;
    }
    if (!parsedModels.some((model) => model.id === defaultModel.trim())) {
      setError('Default model must be listed in Models');
      return;
    }
    if (authKind !== 'none' && credentialKind === 'env' && !envKey.trim()) {
      setError('Environment variable is required');
      return;
    }
    if (
      authKind !== 'none' &&
      credentialKind === 'auth_command' &&
      !approvalId.trim() &&
      !authCommandExecutable.trim()
    ) {
      setError('Absolute auth command executable is required');
      return;
    }
    if (authKind !== 'none' && credentialKind === 'keychain' && !editing && !keychainSecret) {
      setError('New Keychain secret is required');
      return;
    }
    if (authKind === 'header' && !headerName.trim()) {
      setError('Header name is required');
      return;
    }
    if (
      protocol === 'chat_completions' &&
      (Number(requestRetries) !== 0 || Number(streamRetries) !== 0)
    ) {
      setError('Gateway profiles require request and stream retries to be 0');
      return;
    }
    if (
      protocol === 'chat_completions' &&
      (!upstreamPath.startsWith('/') || upstreamPath.includes('..') || upstreamPath.includes('?'))
    ) {
      setError('Enter a controlled absolute upstream path');
      return;
    }
    const timeout = Number(streamIdleTimeout);
    if (!Number.isSafeInteger(timeout) || timeout < 1) {
      setError('Stream idle timeout must be a positive integer');
      return;
    }

    const credential = selectedCredential();
    const upstreamAuth: UpstreamAuthV2 =
      authKind === 'none'
        ? { kind: 'none' }
        : authKind === 'header'
          ? { kind: 'header', name: headerName.trim(), credential }
          : { kind: 'bearer', credential };
    const definition: ProviderDefinitionV2 = {
      id: id.trim(),
      name: name.trim(),
      protocol,
      baseUrl: normalizedUrl,
      defaultModel: defaultModel.trim(),
      models: parsedModels,
      upstreamAuth,
      adapter:
        protocol === 'responses'
          ? { kind: 'none' }
          : {
              kind: 'local',
              adapterId: 'responses_to_chat_completions',
              upstreamPath: upstreamPath.trim(),
            },
      compatibilityProfile: protocol === 'chat_completions' ? compatibilityProfile : undefined,
      codex: {
        displayName: name.trim(),
        streamIdleTimeoutMs: timeout,
        directRequestMaxRetries: protocol === 'chat_completions' ? 0 : Number(requestRetries),
        directStreamMaxRetries: protocol === 'chat_completions' ? 0 : Number(streamRetries),
        routeViaGateway: provider?.codex.routeViaGateway ?? false,
        queryParams: parsePairs(queryParams),
        envHttpHeaders: parsePairs(envHeaders),
      },
    };

    setSaving(true);
    try {
      await onSave(
        definition,
        keychainSecret,
        credentialKind === 'auth_command' && authCommandExecutable.trim()
          ? {
              executable: authCommandExecutable.trim(),
              args: authCommandArgs
                .split('\n')
                .map((value) => value.trim())
                .filter(Boolean),
            }
          : null,
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Provider operation failed');
    } finally {
      setKeychainSecret('');
      setSaving(false);
    }
  }

  return (
    <form noValidate onSubmit={submit}>
      <div className="formGrid">
        <label>
          Provider id
          <input value={id} disabled={editing} onChange={(event) => setId(event.target.value)} />
        </label>
        <label>
          Name
          <input value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label>
          Protocol
          <select
            aria-label="Protocol"
            value={protocol}
            onChange={(event) =>
              setProtocol(event.target.value as ProviderDefinitionV2['protocol'])
            }
          >
            <option value="responses">Responses (direct)</option>
            <option value="chat_completions">Chat Completions (Gateway)</option>
          </select>
        </label>
        {protocol === 'chat_completions' ? (
          <>
            <label>
              Upstream path
              <input
                value={upstreamPath}
                onChange={(event) => setUpstreamPath(event.target.value)}
              />
            </label>
            <label>
              Compatibility profile
              <select
                value={compatibilityProfile}
                onChange={(event) => setCompatibilityProfile(event.target.value)}
              >
                <option value="openai_chat_completions">OpenAI-compatible</option>
                <option value="deepseek_chat_completions">DeepSeek</option>
              </select>
            </label>
          </>
        ) : null}
        <label>
          Base URL
          <input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} />
        </label>
        <label>
          Models
          <textarea
            value={modelsText}
            onChange={(event) => setModelsText(event.target.value)}
            placeholder="model-a, model-b"
          />
        </label>
        <label>
          Default model
          <input value={defaultModel} onChange={(event) => setDefaultModel(event.target.value)} />
        </label>
        <label>
          Authentication
          <select
            value={authKind}
            onChange={(event) => setAuthKind(event.target.value as AuthKind)}
          >
            <option value="bearer">Bearer</option>
            <option value="header">Custom header</option>
            <option value="none">None</option>
          </select>
        </label>
        {authKind === 'header' ? (
          <label>
            Header name
            <input value={headerName} onChange={(event) => setHeaderName(event.target.value)} />
          </label>
        ) : null}
        {authKind !== 'none' ? (
          <label>
            Credential source
            <select
              value={credentialKind}
              onChange={(event) => setCredentialKind(event.target.value as CredentialKind)}
            >
              <option value="env">Environment</option>
              <option value="keychain">Keychain</option>
              <option value="auth_command">Approved auth command</option>
              <option value="none">Missing / configure later</option>
            </select>
          </label>
        ) : null}
        {authKind !== 'none' && credentialKind === 'env' ? (
          <label>
            Environment variable
            <input value={envKey} onChange={(event) => setEnvKey(event.target.value)} />
          </label>
        ) : null}
        {authKind !== 'none' && credentialKind === 'auth_command' ? (
          <>
            {approvalId ? (
              <div className="notice providerBlockers" role="status">
                Existing approved command: {approvalId}
              </div>
            ) : null}
            <label>
              Auth command executable
              <input
                value={authCommandExecutable}
                onChange={(event) => setAuthCommandExecutable(event.target.value)}
                placeholder="/absolute/path/to/token-helper"
              />
            </label>
            <label>
              Arguments (one per line)
              <textarea
                value={authCommandArgs}
                onChange={(event) => setAuthCommandArgs(event.target.value)}
                placeholder="--audience\ncodex"
              />
            </label>
          </>
        ) : null}
        {authKind !== 'none' && credentialKind === 'keychain' ? (
          <label>
            New Keychain secret
            <input
              type="password"
              autoComplete="new-password"
              value={keychainSecret}
              onChange={(event) => setKeychainSecret(event.target.value)}
            />
          </label>
        ) : null}
        <label>
          Stream idle timeout (ms)
          <input
            type="number"
            min="1"
            value={streamIdleTimeout}
            onChange={(event) => setStreamIdleTimeout(event.target.value)}
          />
        </label>
        <label>
          Request retries
          <input
            type="number"
            min="0"
            value={requestRetries}
            onChange={(event) => setRequestRetries(event.target.value)}
          />
        </label>
        <label>
          Stream retries
          <input
            type="number"
            min="0"
            value={streamRetries}
            onChange={(event) => setStreamRetries(event.target.value)}
          />
        </label>
        <label>
          Query params (key=value)
          <textarea value={queryParams} onChange={(event) => setQueryParams(event.target.value)} />
        </label>
        <label>
          Environment headers (header=ENV_KEY)
          <textarea value={envHeaders} onChange={(event) => setEnvHeaders(event.target.value)} />
        </label>
      </div>
      {error ? (
        <div className="notice" role="alert">
          {error}
        </div>
      ) : null}
      <div className="modalFoot">
        <UIButton type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </UIButton>
        <UIButton type="submit" variant="primary" disabled={saving}>
          {editing ? 'Save Provider' : 'Create Provider'}
        </UIButton>
      </div>
    </form>
  );
}

export function ApiAccountConnectionEditor({
  connection,
  onSave,
  onCancel,
}: {
  connection: ApiAccountConnectionViewV2;
  onSave: (request: UpdateApiAccountConnectionRequestV2) => Promise<void>;
  onCancel: () => void;
}) {
  const [baseUrl, setBaseUrl] = useState(connection.baseUrl);
  const [apiKey, setApiKey] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError('');
    let normalizedUrl: string;
    try {
      const url = new URL(baseUrl.trim());
      if (url.protocol !== 'https:' || url.username || url.password) throw new Error('invalid');
      normalizedUrl = url.toString().replace(/\/$/, '');
    } catch {
      setError('Enter a valid HTTPS Provider URL');
      return;
    }
    if (apiKey.length > 0 && !apiKey.trim()) {
      setError('New API key cannot be blank');
      return;
    }
    setSaving(true);
    try {
      await onSave({
        profileId: connection.profileId,
        expectedProviderStoreRevision: connection.providerStoreRevision,
        baseUrl: normalizedUrl,
        ...(apiKey ? { apiKey } : {}),
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'API account update failed');
    } finally {
      setApiKey('');
      setSaving(false);
    }
  }

  return (
    <form noValidate onSubmit={submit}>
      <div className="kv apiAccountConnectionSummary">
        <span>Account</span>
        <strong>{connection.profileId}</strong>
        <span>Protocol</span>
        <strong>Responses</strong>
        <span>Model</span>
        <strong>{connection.selectedModel}</strong>
        <span>Credential</span>
        <strong>{connection.apiKeyConfigured ? 'API key configured' : 'API key missing'}</strong>
      </div>
      <div className="formGrid">
        <label>
          Base URL
          <input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} />
        </label>
        <label>
          New API key
          <input
            type="password"
            autoComplete="new-password"
            value={apiKey}
            placeholder="Leave empty to keep the current key"
            onChange={(event) => setApiKey(event.target.value)}
          />
        </label>
      </div>
      {error ? (
        <div className="notice" role="alert">
          {error}
        </div>
      ) : null}
      <div className="modalFoot">
        <UIButton type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </UIButton>
        <UIButton type="submit" variant="primary" disabled={saving}>
          Save API account
        </UIButton>
      </div>
    </form>
  );
}

type CenterDialog =
  | { kind: 'editor'; provider: ProviderProfileViewV2 }
  | { kind: 'account-editor'; profileId: string }
  | { kind: 'attach'; provider: ProviderProfileViewV2 }
  | {
      kind: 'detach';
      provider: ProviderProfileViewV2;
      binding: ProfileProviderBindingViewV2;
    }
  | null;

function authCredential(auth: UpstreamAuthV2): CredentialReferenceV2 {
  return auth.kind === 'none' ? { kind: 'none' } : auth.credential;
}

export function ProviderCenter({
  profiles,
  onAddExternalApi,
}: {
  profiles: string[];
  onAddExternalApi: () => void;
}) {
  const {
    providers,
    bindings,
    attachPlan,
    detachPlan,
    loading,
    recoveryMessage,
    apiAccountConnection,
    saveProvider,
    approveAuthCommand,
    rotateKeychainCredential,
    testProvider,
    previewAttach,
    executeAttach,
    previewDetach,
    executeDetach,
    clearPlans,
    loadApiAccountConnection,
    updateApiAccountConnection,
    clearApiAccountConnection,
  } = useProviderStore();
  const [dialog, setDialog] = useState<CenterDialog>(null);

  function closeDialog() {
    clearPlans();
    clearApiAccountConnection();
    setDialog(null);
  }

  function editProvider(provider: ProviderProfileViewV2) {
    const binding = bindings.find(
      (item) => item.providerId === provider.id && provider.id === `account-${item.profileId}`,
    );
    const credential = authCredential(provider.upstreamAuth);
    if (binding && credential.kind === 'codex_profile') {
      setDialog({ kind: 'account-editor', profileId: binding.profileId });
      void loadApiAccountConnection(binding.profileId);
      return;
    }
    setDialog({ kind: 'editor', provider });
  }

  async function save(
    definition: ProviderDefinitionV2,
    keychainSecret: string,
    authCommand: AuthCommandDraft | null,
  ) {
    if (dialog?.kind !== 'editor') return;
    if (authCommand && definition.upstreamAuth.kind !== 'none') {
      const approvalId = await approveAuthCommand(authCommand.executable, authCommand.args);
      definition = {
        ...definition,
        upstreamAuth: {
          ...definition.upstreamAuth,
          credential: { kind: 'auth_command', approvalId },
        },
      };
    }
    const expectedCredential = authCredential(definition.upstreamAuth);
    await saveProvider(definition, true);
    if (keychainSecret) {
      await rotateKeychainCredential(definition.id, expectedCredential, keychainSecret);
    }
    closeDialog();
  }

  const activeBinding =
    dialog?.kind === 'attach'
      ? (bindings.find(
          (binding) =>
            binding.profileId === profiles[0] && binding.providerId === dialog.provider.id,
        ) ?? null)
      : null;

  return (
    <div className="providersPage">
      <section className="panel pagePanel">
        <div className="panelHead">
          <div>
            <h3 className="sectionTitle">Advanced Provider Connections</h3>
            <p className="statusHint">
              Daily API accounts are created from Add Account. Reuse and diagnose connections here.
            </p>
          </div>
          <UIButton variant="primary" onClick={onAddExternalApi}>
            Add External API
          </UIButton>
        </div>
        {loading ? <div className="emptyBox">Refreshing Providers…</div> : null}
        <ProviderCards
          providers={providers}
          bindings={bindings}
          onEdit={editProvider}
          onAttach={(provider) => {
            clearPlans();
            setDialog({ kind: 'attach', provider });
          }}
          onDetach={(binding) => {
            const provider = providers.find((item) => item.id === binding.providerId);
            if (!provider) return;
            clearPlans();
            setDialog({ kind: 'detach', provider, binding });
          }}
          onTest={(provider) => void testProvider(provider.id)}
        />
        <div className="infoBanner">
          Secret values are write-only. Provider views contain credential references only.
        </div>
      </section>

      {dialog?.kind === 'editor' ? (
        <Modal title="Edit Provider" close={closeDialog} wide>
          <ProviderEditor provider={dialog.provider} onSave={save} onCancel={closeDialog} />
        </Modal>
      ) : null}

      {dialog?.kind === 'account-editor' ? (
        <Modal title="Edit API Account" close={closeDialog} wide>
          {apiAccountConnection?.profileId === dialog.profileId ? (
            <ApiAccountConnectionEditor
              connection={apiAccountConnection}
              onSave={async (request) => {
                await updateApiAccountConnection(request);
                closeDialog();
              }}
              onCancel={closeDialog}
            />
          ) : (
            <div className="emptyBox">Loading API account…</div>
          )}
        </Modal>
      ) : null}

      {dialog?.kind === 'attach' ? (
        <Modal
          title={activeBinding ? 'Rebind Provider' : 'Attach Provider'}
          close={closeDialog}
          wide
        >
          <ProviderBindingDialog
            mode="attach"
            provider={dialog.provider}
            profiles={profiles}
            binding={activeBinding}
            attachPlan={attachPlan}
            detachPlan={null}
            recoveryMessage={recoveryMessage}
            onPreviewAttach={previewAttach}
            onPreviewDetach={previewDetach}
            onExecute={async () => {
              await executeAttach();
              closeDialog();
            }}
            onClearPlan={clearPlans}
            onClose={closeDialog}
          />
        </Modal>
      ) : null}

      {dialog?.kind === 'detach' ? (
        <Modal title="Detach Provider" close={closeDialog} wide>
          <ProviderBindingDialog
            mode="detach"
            provider={dialog.provider}
            profiles={[dialog.binding.profileId]}
            binding={dialog.binding}
            attachPlan={null}
            detachPlan={detachPlan}
            recoveryMessage={recoveryMessage}
            onPreviewAttach={previewAttach}
            onPreviewDetach={previewDetach}
            onExecute={async () => {
              await executeDetach();
              closeDialog();
            }}
            onClearPlan={clearPlans}
            onClose={closeDialog}
          />
        </Modal>
      ) : null}
    </div>
  );
}
