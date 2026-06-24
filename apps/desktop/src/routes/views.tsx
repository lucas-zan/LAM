import { useState, useEffect } from 'react';
import { sessionDisplayName } from '../lib/format';
import {
  countAccountsWithAvailableQuota,
  countAccountsWithQuotaData,
  quotaDisplayWindows,
} from '../lib/quota';
import type {
  AccountNoteUpdate,
  CodexAccount,
  CodexSession,
  DivergedSessionStrategy,
  HealthCheck,
  ProviderProfile,
  UsageQuotaSnapshot,
  AntigravityQuotaResponse,
  TokenExpirationStatus,
} from '../lib/types';
import { QuotaWindow } from '../components/quota-window';
import {
  IconCopy,
  IconProviders,
  IconInfo,
  MetricIcon,
  type MetricIconName,
  IconPlay,
  IconCloud,
  IconPencil,
  IconKey,
} from '../components/icons';
import { UIButton } from '../components/ui-button';
import { PlanTypeBadge } from '../components/plan-type-badge';
import { checkProfileTokenExpiration } from '../lib/api';

export function AntigravityModels({
  quota,
  refreshing,
  onRefresh,
}: {
  quota: AntigravityQuotaResponse | null;
  refreshing: boolean;
  onRefresh: () => void;
}) {
  if (!quota) {
    return <div className="emptyBox">Loading Antigravity status...</div>;
  }

  if (!quota.ok) {
    return (
      <div className="panel pagePanel" style={{ padding: '40px 24px', textAlign: 'center' }}>
        <div style={{ fontSize: '48px', marginBottom: '16px' }}>🔌</div>
        <h3 style={{ fontSize: '18px', margin: '0 0 8px 0', fontWeight: 600 }}>
          Antigravity Offline
        </h3>
        <p style={{ margin: 0, fontSize: '13px', color: 'var(--text-muted)' }}>
          {quota.error || 'Server is not running'}
        </p>
        <UIButton
          variant="primary"
          size="sm"
          style={{ marginTop: '16px' }}
          onClick={onRefresh}
          disabled={refreshing}
        >
          Retry Connection
        </UIButton>
      </div>
    );
  }

  if (quota.models.length === 0) {
    return <div className="emptyBox">No Antigravity models found.</div>;
  }

  return (
    <section className="overviewAccountsPanel">
      <div className="panelHead">
        <h3 className="sectionTitle">Antigravity</h3>
      </div>
      <div className="cardGrid accountCardGrid">
        {quota.models.map((model) => {
          const remainingFraction = model.remainingFraction ?? null;
          const remainingPercent =
            remainingFraction !== null ? Math.round(remainingFraction * 100) : null;
          const isDepleted = remainingPercent === 0;

          // For the progress bar/quota window, we map usedPercent = 100 - remainingPercent
          const usedPercent = remainingPercent !== null ? 100 - remainingPercent : null;

          return (
            <article className="card accountCard" key={model.label}>
              <div className="cardHead">
                <div className="cardTitleRow">
                  <span
                    className="trayAccountStatusDot"
                    style={{
                      display: 'inline-block',
                      width: '8px',
                      height: '8px',
                      borderRadius: '50%',
                      marginRight: '8px',
                      backgroundColor: isDepleted ? '#ef4444' : '#22c55e',
                      boxShadow: isDepleted
                        ? '0 0 4px rgba(239, 68, 68, 0.5)'
                        : '0 0 4px rgba(34, 197, 94, 0.5)',
                    }}
                  />
                  <h3>{model.label}</h3>
                  {isDepleted && (
                    <span
                      style={{ fontSize: '12px', marginLeft: '6px' }}
                      title="Quota depleted"
                      role="img"
                      aria-label="warning"
                    >
                      ⚠️
                    </span>
                  )}
                </div>
              </div>
              <p className="cardPath mono">Local language server model</p>
              <p className="cardMeta">Connect API model</p>
              <div className="accountQuota">
                <QuotaWindow
                  label="Remaining Quota"
                  usedPercent={usedPercent}
                  resetAt={model.resetTime}
                  variant="session"
                />
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}

export function Overview({
  accounts,
  quotas,
  providers,
  select,
  openSync,
  rename,
  login,
  openHandoff,
  relayLatest,
  currentSession,
  refreshAccountQuota,
  refreshingQuotaIds,
  antigravityQuota,
  refreshingAntigravity,
  onRefreshAntigravity,
  onSaveAccountNote,
  openUploadPat,
}: {
  accounts: CodexAccount[];
  quotas: UsageQuotaSnapshot[];
  providers: ProviderProfile[];
  select: (id: string) => void;
  openSync: (id: string) => void;
  rename: (account: CodexAccount) => void;
  login: (account: CodexAccount) => void;
  openHandoff: (targetAccount: CodexAccount) => void;
  relayLatest: (targetAccount: CodexAccount) => void;
  currentSession?: CodexSession;
  refreshAccountQuota: (profileId: string) => void;
  refreshingQuotaIds: string[];
  antigravityQuota: AntigravityQuotaResponse | null;
  refreshingAntigravity: boolean;
  onRefreshAntigravity: () => void;
  onSaveAccountNote: (req: AccountNoteUpdate) => Promise<void> | void;
  openUploadPat: (accountId: string) => void;
}) {
  const [activeTab, setActiveTab] = useState<'codex' | 'antigravity'>('codex');

  const isAntigravity = activeTab === 'antigravity';

  const accountsWithQuotaData = isAntigravity
    ? (antigravityQuota?.models.filter((m) => (m.remainingFraction ?? 0) > 0).length ?? 0)
    : countAccountsWithQuotaData(accounts, quotas);

  const availableQuotaAccounts = isAntigravity
    ? (antigravityQuota?.models.filter((m) => (m.remainingFraction ?? 0) > 0).length ?? 0)
    : countAccountsWithAvailableQuota(accounts, quotas);

  const sessionTotal = isAntigravity
    ? 0
    : accounts.reduce((sum, account) => sum + account.sessionCount, 0);

  const totalCount = isAntigravity ? (antigravityQuota?.models.length ?? 0) : accounts.length;

  const providersCount = isAntigravity ? 1 : providers.length;

  return (
    <div className="overviewPage">
      <div className="metricGrid">
        <Metric
          icon="accounts"
          label={isAntigravity ? 'Models' : 'Accounts'}
          value={`${accountsWithQuotaData}/${totalCount}`}
        />
        <Metric icon="sessions" label="Sessions" value={isAntigravity ? 'N/A' : sessionTotal} />
        <Metric icon="providers" label="Providers" value={providersCount} />
        <Metric
          icon="quota"
          label={isAntigravity ? 'Models usable' : 'Quota usable'}
          value={availableQuotaAccounts}
        />
      </div>

      <div className="overviewTabs">
        <button
          type="button"
          className={`overviewTab ${activeTab === 'codex' ? 'active' : ''}`}
          onClick={() => setActiveTab('codex')}
        >
          Codex
        </button>
        <button
          type="button"
          className={`overviewTab ${activeTab === 'antigravity' ? 'active' : ''}`}
          onClick={() => setActiveTab('antigravity')}
        >
          Antigravity
        </button>
      </div>

      {activeTab === 'codex' ? (
        <Accounts
          accounts={accounts}
          quotas={quotas}
          select={select}
          openSync={openSync}
          rename={rename}
          login={login}
          openHandoff={openHandoff}
          relayLatest={relayLatest}
          currentSession={currentSession}
          refreshAccountQuota={refreshAccountQuota}
          refreshingQuotaIds={refreshingQuotaIds}
          onSaveAccountNote={onSaveAccountNote}
          openUploadPat={openUploadPat}
          variant="overview"
        />
      ) : (
        <AntigravityModels
          quota={antigravityQuota}
          refreshing={refreshingAntigravity}
          onRefresh={onRefreshAntigravity}
        />
      )}
    </div>
  );
}


function AuthModeBadge({ authMode }: { authMode?: string | null }) {
  if (!authMode) return null;
  
  const modeLabels: Record<string, string> = {
    personal_token: 'PAT',
    oauth: 'OAuth',
    api_key: 'API Key',
    config: 'Config',
  };
  
  const label = modeLabels[authMode] ?? authMode;
  
  return (
    <span className="badge badge--authMode" title={`Auth mode: ${label}`}>
      {label}
    </span>
  );
}

function TokenExpirationBadge({ 
  status 
}: { 
  status?: { isExpired: boolean; daysUntilExpiration?: number | null; warningLevel: string } | null 
}) {
  if (!status) return null;
  
  const { isExpired, daysUntilExpiration, warningLevel } = status;
  
  if (warningLevel === 'ok') return null; // Don't show badge when >30 days
  
  let badgeClass = 'badge';
  let label = '';
  
  if (isExpired) {
    badgeClass += ' badge--expired';
    label = 'Token expired';
  } else if (warningLevel === 'critical') {
    badgeClass += ' badge--critical';
    label = `Expires in ${daysUntilExpiration}d`;
  } else if (warningLevel === 'warning') {
    badgeClass += ' badge--warning';
    label = `Expires in ${daysUntilExpiration}d`;
  }
  
  return (
    <span className={badgeClass} title="PAT token expiration">
      {label}
    </span>
  );
}

export function Accounts({
  accounts,
  quotas,
  select,
  openSync,
  rename,
  login,
  openHandoff,
  relayLatest,
  currentSession,
  refreshAccountQuota,
  refreshingQuotaIds,
  onSaveAccountNote,
  openUploadPat,
  variant = 'default',
}: {
  accounts: CodexAccount[];
  quotas: UsageQuotaSnapshot[];
  select: (id: string) => void;
  openSync: (id: string) => void;
  rename: (account: CodexAccount) => void;
  login: (account: CodexAccount) => void;
  openHandoff: (targetAccount: CodexAccount) => void;
  relayLatest: (targetAccount: CodexAccount) => void;
  currentSession?: CodexSession;
  refreshAccountQuota: (profileId: string) => void;
  refreshingQuotaIds: string[];
  onSaveAccountNote: (req: AccountNoteUpdate) => Promise<void> | void;
  openUploadPat: (accountId: string) => void;
  variant?: 'default' | 'overview';
}) {
  const [tokenStatuses, setTokenStatuses] = useState<Record<string, TokenExpirationStatus>>({});
  useEffect(() => {
    // Fetch token expiration status for accounts with personal_token auth mode
    const fetchTokenStatuses = async () => {
      const patAccounts = accounts.filter((acc) => acc.authMode === 'personal_token');
      
      for (const account of patAccounts) {
        try {
          const status = await checkProfileTokenExpiration(account.id);
          setTokenStatuses((prev) => ({ ...prev, [account.id]: status }));
        } catch (err) {
          // Silently ignore errors - badge won't show if fetch fails
          console.warn(`Failed to fetch token status for ${account.id}:`, err);
        }
      }
    };
    
    if (accounts.length > 0) {
      fetchTokenStatuses();
    }
  }, [accounts]);
  if (!accounts.length) return <div className="emptyBox">No Codex profiles found.</div>;
  const activeAccount = currentSession
    ? accounts.find((account) => account.id === currentSession.accountId)
    : undefined;
  const orderedAccounts = [...accounts].sort((a, b) => {
    const latestDiff = (b.latestSessionModifiedAt ?? 0) - (a.latestSessionModifiedAt ?? 0);
    if (latestDiff !== 0) return latestDiff;
    return b.sessionCount - a.sessionCount;
  });
  return (
    <section className={variant === 'overview' ? 'overviewAccountsPanel' : 'panel pagePanel'}>
      <div className="panelHead">
        <h3 className="sectionTitle">Accounts</h3>
      </div>
      <div className="activeSessionBanner">
        <span>Active source</span>
        <strong>
          {activeAccount?.displayName ?? currentSession?.accountId ?? 'No active session'}
        </strong>
        <em className="mono">{currentSession?.id ?? 'No session found'}</em>
      </div>
      <div className="cardGrid accountCardGrid">
        {orderedAccounts.map((account) => {
          const isRefreshing = refreshingQuotaIds.includes(account.id);
          const quota = quotas.find((item) => item.profileId === account.id);
          const providerLabel = account.providerId ?? 'unknown';
          const modelLabel = account.model ?? 'unknown';
          const isActiveAccount = currentSession?.accountId === account.id;
          return (
            <article
              className="card accountCard"
              key={account.id}
              onClick={() => select(account.id)}
            >
              <div className="cardHead">
                <div className="cardTitleRow">
                  <h3>{account.displayName}</h3>
                  <PlanTypeBadge planType={quota?.planType} />
                  {isActiveAccount ? (
                    <span className="accountActiveBadge" aria-label="Active session account">
                      Active
                    </span>
                  ) : null}
                </div>
                <div className="cardHeadActions">
                  <span className={account.hasAuth ? 'badge badge--auth' : 'badge warn'}>
                    {account.hasAuth ? 'Logged in' : 'Login needed'}
                  </span>
                  <AuthModeBadge authMode={account.authMode} />
                  <TokenExpirationBadge status={tokenStatuses[account.id]} />
                  <UIButton
                    variant="icon"
                    size="sm"
                    className={`iconCircleBtn ${isRefreshing ? 'isSpinning' : ''}`}
                    title={`Refresh ${account.displayName} quota`}
                    aria-label={`Refresh ${account.displayName} quota`}
                    disabled={isRefreshing}
                    onClick={(e) => {
                      e.stopPropagation();
                      refreshAccountQuota(account.id);
                    }}
                  >
                    ↻
                  </UIButton>
                </div>
              </div>
              <p className="cardPath mono" title={account.codexHome}>
                {account.codexHome}
              </p>
              <p
                className="cardMeta"
                title={`${account.sessionCount} sessions · Provider: ${providerLabel} · ${modelLabel}`}
              >
                {account.sessionCount} sessions · Provider: {providerLabel} · {modelLabel}
              </p>
              <AccountNotePanel account={account} onSave={onSaveAccountNote} />
              <div className="accountQuota">
                {quotaDisplayWindows(quota).map((window) => (
                  <QuotaWindow
                    key={window.key}
                    label={window.label}
                    usedPercent={window.usedPercent}
                    resetAt={window.resetAt}
                    variant={window.variant}
                  />
                ))}
              </div>
              <div className="cardActions">
                <UIButton
                  size="sm"
                  variant="primary"
                  className="accountActionBtn"
                  disabled={!currentSession}
                  aria-label="Relay Latest"
                  title={
                    currentSession
                      ? `Relay latest active session ${currentSession.id} with ${account.displayName}`
                      : 'No active session found'
                  }
                  onClick={(e) => {
                    e.stopPropagation();
                    relayLatest(account);
                  }}
                >
                  <IconPlay size={13} />
                  Relay Latest
                </UIButton>
                <UIButton
                  size="sm"
                  className="accountActionBtn"
                  disabled={accounts.length < 2}
                  aria-label="Handoff"
                  title={`Choose a session to continue with ${account.displayName}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    openHandoff(account);
                  }}
                >
                  <IconPlay size={13} />
                  Handoff
                </UIButton>
                <UIButton
                  size="sm"
                  className="accountActionBtn"
                  onClick={(e) => {
                    e.stopPropagation();
                    openSync(account.id);
                  }}
                >
                  <IconCloud size={13} />
                  Sync Sessions...
                </UIButton>
                <UIButton
                  size="sm"
                  className="accountActionBtn"
                  disabled={account.id === 'main'}
                  title={
                    account.id === 'main'
                      ? 'Main profile cannot be renamed'
                      : `Rename ${account.displayName}`
                  }
                  onClick={(e) => {
                    e.stopPropagation();
                    rename(account);
                  }}
                >
                  <IconPencil size={13} />
                  Rename
                </UIButton>
                <UIButton
                  size="sm"
                  className="accountActionBtn"
                  onClick={(e) => {
                    e.stopPropagation();
                    login(account);
                  }}
                >
                  <IconKey size={13} />
                  Login
                </UIButton>
                <UIButton
                  size="sm"
                  className="accountActionBtn"
                  aria-label="Upload PAT"
                  title="Upload personal access token credentials"
                  onClick={(e) => {
                    e.stopPropagation();
                    openUploadPat(account.id);
                  }}
                >
                  <IconKey size={13} />
                  Upload PAT
                </UIButton>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}

function AccountNotePanel({
  account,
  onSave,
}: {
  account: CodexAccount;
  onSave: (req: AccountNoteUpdate) => Promise<void> | void;
}) {
  const [editing, setEditing] = useState(false);
  const [renewalDate, setRenewalDate] = useState(account.renewalDate ?? '');
  const [note, setNote] = useState(account.note ?? '');
  const [saving, setSaving] = useState(false);

  function startEditing() {
    setRenewalDate(account.renewalDate ?? '');
    setNote(account.note ?? '');
    setEditing(true);
  }

  async function save() {
    setSaving(true);
    try {
      await onSave({
        profileId: account.id,
        renewalDate: renewalDate || null,
        note: note || null,
      });
      setEditing(false);
    } finally {
      setSaving(false);
    }
  }

  if (editing) {
    return (
      <form
        className="accountNoteForm"
        onClick={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <label>
          <span>Renewal date</span>
          <input
            type="date"
            value={renewalDate}
            onChange={(event) => setRenewalDate(event.target.value)}
          />
        </label>
        <label>
          <span>Account note</span>
          <textarea
            value={note}
            maxLength={500}
            rows={3}
            onChange={(event) => setNote(event.target.value)}
          />
        </label>
        <div className="accountNoteActions">
          <UIButton size="sm" variant="primary" type="submit" disabled={saving}>
            Save
          </UIButton>
          <UIButton size="sm" type="button" onClick={() => setEditing(false)} disabled={saving}>
            Cancel
          </UIButton>
        </div>
      </form>
    );
  }

  return (
    <div className="accountNoteSummary" onClick={(event) => event.stopPropagation()}>
      <div>
        <strong>{account.renewalDate ? `Renews ${account.renewalDate}` : 'No renewal date'}</strong>
        <span>{account.note || 'No note'}</span>
      </div>
      <UIButton size="sm" className="accountNoteEditBtn" onClick={startEditing}>
        <IconPencil size={13} />
        Edit note
      </UIButton>
    </div>
  );
}

export function Sessions({
  sessions,
  accounts,
  selectedAccountId,
  setSelectedAccountId,
  query,
  setQuery,
  copy,
  open,
  details,
  openHandoff,
}: {
  sessions: CodexSession[];
  accounts: CodexAccount[];
  selectedAccountId: string;
  setSelectedAccountId: (id: string) => void;
  query: string;
  setQuery: (q: string) => void;
  copy: (session: CodexSession) => void;
  open: (session: CodexSession) => void;
  details: (session: CodexSession) => void;
  openHandoff: (session: CodexSession) => void;
}) {
  return (
    <section className="panel pagePanel">
      <div className="panelHead panelHead--stack">
        <h3 className="sectionTitle">Sessions</h3>
        <div className="sessionsTools">
          <select
            value={selectedAccountId}
            onChange={(e) => setSelectedAccountId(e.target.value)}
            aria-label="Filter by account"
          >
            {accounts.map((account) => (
              <option key={account.id} value={account.id}>
                {account.displayName}
              </option>
            ))}
          </select>
          <input
            placeholder="Search cwd / summary / id"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>

      {sessions.length ? (
        <div className="tableWrap">
          <table className="sessionsTable">
            <thead>
              <tr>
                <th>Session</th>
                <th>cwd</th>
                <th>Provider</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((session) => {
                const displayName = sessionDisplayName(session);
                return (
                  <tr key={`${session.accountId}-${session.path}`} onClick={() => details(session)}>
                    <td>
                      <div className="sessionIdCell">
                        <strong className="cellTrunc" title={displayName}>
                          {displayName}
                        </strong>
                        <button
                          type="button"
                          className="iconGhostBtn"
                          title="Copy resume command"
                          aria-label="Copy resume command"
                          onClick={(e) => {
                            e.stopPropagation();
                            copy(session);
                          }}
                        >
                          <IconCopy size={14} />
                        </button>
                      </div>
                      {displayName !== session.id ? (
                        <div className="cellSub mono cellTrunc" title={session.id}>
                          {session.id}
                        </div>
                      ) : null}
                      {session.providerMismatch ? (
                        <span className="badge warn">provider mismatch</span>
                      ) : null}
                    </td>
                    <td>
                      <span className="mono cellTrunc" title={session.cwd ?? 'unknown'}>
                        {session.cwd ?? 'unknown'}
                      </span>
                    </td>
                    <td>
                      <strong
                        className="cellTrunc"
                        title={session.currentProviderId ?? session.model ?? 'unknown'}
                      >
                        {session.currentProviderId ?? session.model ?? 'unknown'}
                      </strong>
                      <div
                        className="cellSub mono cellTrunc"
                        title={session.currentModel ?? session.model ?? 'unknown'}
                      >
                        {session.currentModel ?? session.model ?? 'unknown'}
                      </div>
                    </td>
                    <td>
                      <div className="rowActions">
                        <UIButton
                          size="sm"
                          onClick={(e) => {
                            e.stopPropagation();
                            copy(session);
                          }}
                        >
                          Copy
                        </UIButton>
                        <UIButton
                          size="sm"
                          onClick={(e) => {
                            e.stopPropagation();
                            open(session);
                          }}
                        >
                          &gt;_ Terminal
                        </UIButton>
                        <UIButton
                          size="sm"
                          onClick={(e) => {
                            e.stopPropagation();
                            openHandoff(session);
                          }}
                        >
                          Relay To...
                        </UIButton>
                        <UIButton
                          size="sm"
                          onClick={(e) => {
                            e.stopPropagation();
                            details(session);
                          }}
                        >
                          ⓘ Details
                        </UIButton>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="emptyBox">No sessions. Unknown cwd values will display as `unknown`.</div>
      )}
    </section>
  );
}

export function Providers({
  accounts,
  providers,
  create,
  test,
  remove,
  attach,
}: {
  accounts: CodexAccount[];
  providers: ProviderProfile[];
  create: () => void;
  test: (providerId: string) => void;
  remove: (providerId: string) => void;
  attach: (providerId: string) => void;
}) {
  return (
    <div className="providersPage">
      <section className="panel pagePanel">
        <div className="panelHead">
          <h3 className="sectionTitle">Providers</h3>
          <UIButton variant="primary" onClick={create}>
            Add Provider
          </UIButton>
        </div>
        {providers.length ? (
          <div className="cardGrid">
            {providers.map((provider) => (
              <article className="card" key={provider.id}>
                <div className="cardHead">
                  <h3>{provider.name}</h3>
                  <span className="badge">{provider.health}</span>
                </div>
                <p className="mono cardPath">{provider.baseUrl}</p>
                <div className="kv">
                  <span>ID</span>
                  <strong>{provider.id}</strong>
                  <span>Model</span>
                  <strong>{provider.defaultModel}</strong>
                  <span>Secret</span>
                  <strong>
                    {provider.secretStorage}
                    {provider.envKey ? ` · ${provider.envKey}` : ''}
                  </strong>
                </div>
                <div className="cardActions">
                  <UIButton size="sm" onClick={() => test(provider.id)}>
                    Test
                  </UIButton>
                  <UIButton size="sm" onClick={() => attach(provider.id)}>
                    Attach
                  </UIButton>
                  <UIButton variant="danger" size="sm" onClick={() => remove(provider.id)}>
                    Delete
                  </UIButton>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <div className="emptyState">
            <div className="emptyStateIcon" aria-hidden>
              <IconProviders size={32} />
            </div>
            <strong>No provider profiles yet</strong>
            <p>Add a provider profile to start managing API keys and bindings.</p>
          </div>
        )}
        <div className="infoBanner">
          <IconInfo size={16} />
          <span>
            API keys are never returned to the UI. Provider store contains metadata and secret
            references only.
          </span>
        </div>
      </section>
      <section className="panel pagePanel">
        <div className="panelHead">
          <h3 className="sectionTitle">Bindings</h3>
        </div>
        <div className="bindingList">
          {accounts.map((account) => (
            <div className="bindingRow" key={account.id}>
              <span className="bindingName">{account.id}</span>
              <strong>
                {account.providerId ?? 'unknown'} · {account.model ?? 'unknown'}
              </strong>
              <em>{account.authMode ?? 'unknown'}</em>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

export function SyncHome({
  accounts,
  openSync,
}: {
  accounts: CodexAccount[];
  openSync: () => void;
}) {
  return (
    <section className="panel pagePanel">
      <div className="panelHead">
        <h3 className="sectionTitle">Sync</h3>
        <UIButton variant="primary" disabled={accounts.length < 2} onClick={() => openSync()}>
          Open Sync
        </UIButton>
      </div>
      <div className="rows">
        <div>
          <span>Default include</span>
          <strong>sessions/</strong>
          <em>Phase 1</em>
        </div>
        <div>
          <span>Blocked</span>
          <strong>auth.json, config.toml, sqlite, cache, tmp, logs</strong>
          <em>Strict</em>
        </div>
        <div>
          <span>History</span>
          <strong>sidecar backup only</strong>
          <em>No merge</em>
        </div>
      </div>
    </section>
  );
}

export function Settings({
  health,
  themeMode,
  resolvedTheme,
  divergedStrategy,
  setDivergedStrategy,
}: {
  health: HealthCheck | null;
  themeMode: 'system' | 'light' | 'dark';
  resolvedTheme: 'light' | 'dark';
  divergedStrategy: DivergedSessionStrategy;
  setDivergedStrategy: (strategy: DivergedSessionStrategy) => void;
}) {
  return (
    <section className="panel pagePanel">
      <h3 className="sectionTitle">Settings</h3>
      <div className="rows">
        <div>
          <span>Status</span>
          <strong>{health?.ok ? 'connected' : 'not connected'}</strong>
          <em>{health?.version ?? 'unknown'}</em>
        </div>
        <div>
          <span>Home root</span>
          <strong>{health?.homeRoot ?? 'unknown'}</strong>
          <em>LAM_HOME or HOME</em>
        </div>
        <label className="settingsSelectRow">
          <span>Diverged session strategy</span>
          <select
            value={divergedStrategy}
            onChange={(event) => setDivergedStrategy(event.target.value as DivergedSessionStrategy)}
          >
            <option value="summarize_fork_with_target_account">
              Summarize fork with target account
            </option>
            <option value="stop_and_ask">Stop and ask</option>
            <option value="timeline_merge_to_fork">Timeline merge to fork</option>
            <option value="prefer_source">Prefer source with backup</option>
            <option value="prefer_target">Prefer target and save source fork</option>
          </select>
          <em>Used when both accounts continued the same session differently.</em>
        </label>
      </div>
    </section>
  );
}

export { PlanView } from '../components/plan-view';

function Metric({
  icon,
  label,
  value,
}: {
  icon: MetricIconName;
  label: string;
  value: number | string;
}) {
  return (
    <article className="metricCard">
      <div className={`metricIcon metricIcon--${icon}`} aria-hidden>
        <MetricIcon name={icon} size={20} />
      </div>
      <div>
        <span>{label}</span>
        <strong>{value}</strong>
      </div>
    </article>
  );
}
