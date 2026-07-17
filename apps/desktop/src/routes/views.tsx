import { useState, useEffect, useMemo } from 'react';
import { sessionDisplayName } from '../lib/format';
import {
  countAccountsWithAvailableQuota,
  countAccountsWithQuotaData,
  resetCreditDisplay,
  quotaDisplayWindows,
} from '../lib/quota';
import { authModeLabel } from '../lib/auth';
import {
  groupAntigravityModels,
  quotaBucketUsedPercent,
  quotaBucketVariant,
} from '../lib/antigravity';
import type {
  AccountNoteUpdate,
  CodexAccount,
  CodexSession,
  DivergedSessionStrategy,
  HealthCheck,
  ProviderProfileViewV2,
  UsageQuotaSnapshot,
  AntigravityQuotaResponse,
  TokenExpirationStatus,
  TerminalTarget,
  UsageRateCardEntry,
} from '../lib/types';
import { QuotaWindow } from '../components/quota-window';
import {
  IconCopy,
  MetricIcon,
  type MetricIconName,
  IconPlay,
  IconCloud,
  IconPencil,
  IconKey,
  IconSync,
  IconTrash,
  IconSliders,
  IconCoins,
  IconDevice,
  IconDots,
} from '../components/icons';
import { UIButton } from '../components/ui-button';
import { PlanTypeBadge } from '../components/plan-type-badge';
import { confirm as tauriConfirm } from '@tauri-apps/plugin-dialog';
import { checkProfileTokenExpiration, getUsageRateCard, inTauri } from '../lib/api';
import { formatCost } from '../lib/usage-pricing';

function shortenPath(path: string | null | undefined): string {
  if (!path) return '';
  return path
    .replace(/^\/Users\/[^/]+/, '~')
    .replace(/^\/home\/[^/]+/, '~')
    .replace(/^[a-zA-Z]:\\Users\\[^\\]+/, '~');
}

async function confirmResetQuota(displayName: string): Promise<boolean> {
  const message = `Reset quota for ${displayName}? This will consume one reset credit if one is available.`;
  if (inTauri()) {
    try {
      return await tauriConfirm(message, { title: 'Reset Quota', kind: 'warning' });
    } catch (err) {
      console.warn('Tauri confirm dialog failed; falling back to window.confirm', err);
    }
  }
  return window.confirm(message);
}

const getModelTileClass = (label: string) => {
  const l = label.toLowerCase();
  if (l.includes('gemini')) return 'antigravityModelTile--gemini';
  if (l.includes('claude')) return 'antigravityModelTile--claude';
  if (l.includes('gpt')) return 'antigravityModelTile--gpt';
  return '';
};

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

  const groups = quota.groups ?? [];
  if (groups.length === 0 && quota.models.length === 0) {
    return <div className="emptyBox">No Antigravity models found.</div>;
  }

  if (groups.length > 0) {
    const groupedModels = groupAntigravityModels(quota);
    return (
      <section className="overviewAccountsPanel">
        <div className="panelHead">
          <h3 className="sectionTitle">Antigravity</h3>
          <UIButton variant="default" size="sm" onClick={onRefresh} disabled={refreshing}>
            {refreshing ? 'Refreshing...' : 'Refresh'}
          </UIButton>
        </div>

        <div className="cardGrid accountCardGrid">
          {groupedModels.map(({ group, models }) => (
            <article className="card accountCard antigravityGroupCard" key={group.displayName}>
              <div className="cardHead">
                <div className="cardTitleRow">
                  <h3>{group.displayName}</h3>
                </div>
              </div>

              <div className="accountQuota">
                {[...group.buckets]
                  .sort((a, b) => {
                    const aIs5h =
                      a.displayName.toLowerCase().includes('five') ||
                      a.displayName.toLowerCase().includes('5h');
                    const bIs5h =
                      b.displayName.toLowerCase().includes('five') ||
                      b.displayName.toLowerCase().includes('5h');
                    if (aIs5h && !bIs5h) return -1;
                    if (!aIs5h && bIs5h) return 1;
                    return 0;
                  })
                  .map((bucket) => (
                    <QuotaWindow
                      key={bucket.bucketId ?? bucket.displayName}
                      label={bucket.displayName}
                      usedPercent={quotaBucketUsedPercent(bucket)}
                      resetAt={bucket.resetTime}
                      variant={quotaBucketVariant(bucket)}
                    />
                  ))}
              </div>
              {models.length > 0 ? (
                <div className="antigravityModelSection">
                  <div className="antigravityModelSectionHeader">
                    <span>Models in this group</span>
                  </div>
                  <div className="antigravityModelGrid" aria-label={`${group.displayName} models`}>
                    {models.map((model) => (
                      <div
                        className={`antigravityModelTile ${getModelTileClass(model.label)}`}
                        key={model.label}
                      >
                        <strong>{model.label}</strong>
                        <span>Shares group limits</span>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}
            </article>
          ))}
        </div>
      </section>
    );
  }

  return (
    <section className="overviewAccountsPanel">
      <div className="panelHead">
        <h3 className="sectionTitle">Antigravity</h3>
        <UIButton variant="default" size="sm" onClick={onRefresh} disabled={refreshing}>
          {refreshing ? 'Refreshing...' : 'Refresh'}
        </UIButton>
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
  deleteAccount,
  switchModel = () => {},
  apiAccountIds = [],
  login,
  switchAccount,
  exportCpa,
  openHandoff,
  relayLatest,
  currentSession,
  refreshAccountQuota,
  resetAccountQuota = async () => {},
  refreshingQuotaIds,
  resettingQuotaIds = [],
  antigravityQuota,
  refreshingAntigravity,
  onRefreshAntigravity,
  onSaveAccountNote,
  authMode,
  compactButtons,
}: {
  accounts: CodexAccount[];
  quotas: UsageQuotaSnapshot[];
  providers: ProviderProfileViewV2[];
  select: (id: string) => void;
  openSync: (id: string) => void;
  rename: (account: CodexAccount) => void;
  deleteAccount: (account: CodexAccount) => void;
  switchModel?: (account: CodexAccount) => void;
  apiAccountIds?: string[];
  login: (account: CodexAccount) => void;
  switchAccount: (account: CodexAccount) => void;
  exportCpa: (account: CodexAccount) => void;
  openHandoff: (targetAccount: CodexAccount) => void;
  relayLatest: (targetAccount: CodexAccount) => void;
  currentSession?: CodexSession;
  refreshAccountQuota: (profileId: string) => void;
  resetAccountQuota?: (profileId: string) => Promise<void>;
  refreshingQuotaIds: string[];
  resettingQuotaIds?: string[];
  antigravityQuota: AntigravityQuotaResponse | null;
  refreshingAntigravity: boolean;
  onRefreshAntigravity: () => void;
  onSaveAccountNote: (req: AccountNoteUpdate) => Promise<void> | void;
  authMode?: 'oauth' | 'pat';
  compactButtons?: boolean;
}) {
  const [activeTab, setActiveTab] = useState<'codex' | 'antigravity'>('codex');

  const isAntigravity = activeTab === 'antigravity';
  const antigravityGroups = antigravityQuota?.groups ?? [];
  const antigravityGroupedModels = antigravityQuota ? groupAntigravityModels(antigravityQuota) : [];
  const antigravityModelCount = antigravityQuota?.models.length ?? 0;
  const antigravityGroupCount = antigravityGroups.length;
  const antigravityUsableModelCount = antigravityGroupedModels.length
    ? antigravityGroupedModels.reduce((sum, { group, models }) => {
        const groupHasQuota = group.buckets.some((bucket) => (bucket.remainingFraction ?? 0) > 0);
        return groupHasQuota ? sum + models.length : sum;
      }, 0)
    : (antigravityQuota?.models.filter((m) => (m.remainingFraction ?? 0) > 0).length ?? 0);

  const accountsWithQuotaData = isAntigravity
    ? antigravityUsableModelCount
    : countAccountsWithQuotaData(accounts, quotas);

  const availableQuotaAccounts = isAntigravity
    ? antigravityUsableModelCount
    : countAccountsWithAvailableQuota(accounts, quotas);

  const sessionTotal = isAntigravity
    ? 0
    : accounts.reduce((sum, account) => sum + account.sessionCount, 0);

  const totalCount = isAntigravity ? antigravityModelCount : accounts.length;

  const providersCount = isAntigravity ? 1 : providers.length;

  return (
    <div className="overviewPage">
      <div className="metricGrid">
        <Metric
          icon="accounts"
          label={isAntigravity ? 'Models' : 'Accounts'}
          value={`${accountsWithQuotaData}/${totalCount}`}
        />
        <Metric
          icon="sessions"
          label={isAntigravity ? 'Groups' : 'Sessions'}
          value={isAntigravity ? antigravityGroupCount : sessionTotal}
        />
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
          deleteAccount={deleteAccount}
          switchModel={switchModel}
          apiAccountIds={apiAccountIds}
          login={login}
          switchAccount={switchAccount}
          exportCpa={exportCpa}
          openHandoff={openHandoff}
          relayLatest={relayLatest}
          currentSession={currentSession}
          refreshAccountQuota={refreshAccountQuota}
          resetAccountQuota={resetAccountQuota}
          refreshingQuotaIds={refreshingQuotaIds}
          resettingQuotaIds={resettingQuotaIds}
          onSaveAccountNote={onSaveAccountNote}
          variant="overview"
          authMode={authMode}
          compactButtons={compactButtons}
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
  const label = authModeLabel(authMode);
  if (!label) return null;

  return (
    <span className="badge badge--authMode" title={`Auth mode: ${label}`}>
      {label}
    </span>
  );
}

function TokenExpirationBadge({
  status,
}: {
  status?: { isExpired: boolean; daysUntilExpiration?: number | null; warningLevel: string } | null;
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
  deleteAccount,
  switchModel = () => {},
  apiAccountIds = [],
  login,
  switchAccount,
  exportCpa,
  openHandoff,
  relayLatest,
  currentSession,
  refreshAccountQuota,
  resetAccountQuota = async () => {},
  refreshingQuotaIds,
  resettingQuotaIds = [],
  onSaveAccountNote,
  variant = 'default',
  authMode = 'oauth',
  compactButtons = true,
}: {
  accounts: CodexAccount[];
  quotas: UsageQuotaSnapshot[];
  select: (id: string) => void;
  openSync: (id: string) => void;
  rename: (account: CodexAccount) => void;
  deleteAccount: (account: CodexAccount) => void;
  switchModel?: (account: CodexAccount) => void;
  apiAccountIds?: string[];
  login: (account: CodexAccount) => void;
  switchAccount: (account: CodexAccount) => void;
  exportCpa: (account: CodexAccount) => void;
  openHandoff: (targetAccount: CodexAccount) => void;
  relayLatest: (targetAccount: CodexAccount) => void;
  currentSession?: CodexSession;
  refreshAccountQuota: (profileId: string) => void;
  resetAccountQuota?: (profileId: string) => Promise<void>;
  refreshingQuotaIds: string[];
  resettingQuotaIds?: string[];
  onSaveAccountNote: (req: AccountNoteUpdate) => Promise<void> | void;
  variant?: 'default' | 'overview';
  authMode?: 'oauth' | 'pat';
  compactButtons?: boolean;
}) {
  const [tokenStatuses, setTokenStatuses] = useState<Record<string, TokenExpirationStatus>>({});
  const [activeMenuId, setActiveMenuId] = useState<string | null>(null);

  useEffect(() => {
    const handleOutsideClick = (e: MouseEvent) => {
      if (activeMenuId !== null) {
        const target = e.target as HTMLElement;
        if (!target.closest('.cardMenuDropdown') && !target.closest('.cardMenuBtn')) {
          setActiveMenuId(null);
        }
      }
    };
    window.addEventListener('click', handleOutsideClick);
    return () => window.removeEventListener('click', handleOutsideClick);
  }, [activeMenuId]);

  useEffect(() => {
    const fetchTokenStatuses = async () => {
      const patAccounts = accounts.filter(
        (acc) => acc.authMode === 'personal_token' || acc.authMode === 'uploaded',
      );

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
  const activeAccount =
    authMode === 'pat'
      ? accounts.find((account) => account.isActiveAuth)
      : currentSession
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
        <span>{authMode === 'pat' ? 'Active auth' : 'Active source'}</span>
        <strong>
          {activeAccount?.displayName ??
            (authMode === 'pat'
              ? 'Unrecognized'
              : (currentSession?.accountId ?? 'No active session'))}
        </strong>
        <em className="mono">
          {authMode === 'pat'
            ? (activeAccount?.codexHome ?? 'No unique tokens.account_id match')
            : (currentSession?.id ?? 'No session found')}
        </em>
      </div>
      <div className="cardGrid accountCardGrid">
        {orderedAccounts.map((account) => {
          const isRefreshing = refreshingQuotaIds.includes(account.id);
          const isResetting = resettingQuotaIds.includes(account.id);
          const isApiAccount = apiAccountIds.includes(account.id);
          const quota = quotas.find((item) => item.profileId === account.id);
          const resetCredits = isApiAccount ? null : resetCreditDisplay(quota);
          const canResetQuota = (quota?.resetCreditCount ?? 0) > 0 && !isResetting;
          const modelLabel = account.model ?? 'unknown';
          const hasProvider = account.providerId && account.providerId !== 'unknown';
          const providerPart = hasProvider ? `Provider: ${account.providerId}` : '';
          const metaText = [`${account.sessionCount} sessions`, providerPart, modelLabel]
            .filter(Boolean)
            .join(' · ');
          const isActiveAccount =
            authMode === 'pat'
              ? account.isActiveAuth === true
              : currentSession?.accountId === account.id;
          const primaryBtnClass = compactButtons
            ? 'accountActionBtn accountActionBtn--primary'
            : 'accountActionBtn';
          const secondaryBtnClass = compactButtons
            ? 'accountActionBtn accountActionBtn--secondary'
            : 'accountActionBtn';
          return (
            <article
              className="card accountCard"
              key={account.id}
              onClick={() => select(account.id)}
            >
              <div className="cardHead">
                <div className="cardTitleRow">
                  <h3>{account.displayName}</h3>
                  {resetCredits ? (
                    <span className="resetCreditBadge" aria-label={resetCredits.title}>
                      <span className="resetCreditDots">
                        {resetCredits.dots.map((dot) => (
                          <span
                            key={dot.key}
                            className={`resetCreditDot resetCreditDot--${dot.color}`}
                            data-tooltip={dot.title}
                            aria-label={dot.title}
                            tabIndex={0}
                          />
                        ))}
                        {resetCredits.overflow > 0 ? (
                          <span className="resetCreditMore">+{resetCredits.overflow}</span>
                        ) : null}
                      </span>
                      {resetCredits.nearestExpiry ? (
                        <span className="resetCreditExpiry">{resetCredits.nearestExpiry}</span>
                      ) : null}
                    </span>
                  ) : null}
                </div>
                <div className="cardHeadActions">
                  {!isApiAccount ? (
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
                  ) : null}
                  <div className="cardMenuContainer">
                    <UIButton
                      variant="icon"
                      size="sm"
                      className="iconCircleBtn cardMenuBtn"
                      title="More options"
                      aria-label="More options"
                      onClick={(e) => {
                        e.stopPropagation();
                        setActiveMenuId(activeMenuId === account.id ? null : account.id);
                      }}
                    >
                      <IconDots size={14} />
                    </UIButton>
                    {activeMenuId === account.id && (
                      <div className="cardMenuDropdown" onClick={(e) => e.stopPropagation()}>
                        {authMode === 'oauth' ? (
                          <>
                            <button
                              type="button"
                              className="cardMenuDropdownItem"
                              onClick={() => {
                                setActiveMenuId(null);
                                openSync(account.id);
                              }}
                            >
                              <IconCloud size={13} />
                              <span>Sync Sessions...</span>
                            </button>
                            {!isApiAccount ? (
                              <button
                                type="button"
                                className="cardMenuDropdownItem"
                                disabled={!canResetQuota}
                                title={
                                  quota?.resetCreditCount
                                    ? `Reset ${account.displayName} quota`
                                    : 'No reset credits available'
                                }
                                onClick={async () => {
                                  setActiveMenuId(null);
                                  if (await confirmResetQuota(account.displayName)) {
                                    void resetAccountQuota(account.id);
                                  }
                                }}
                              >
                                <IconPlay size={13} />
                                <span>{isResetting ? 'Resetting...' : 'Reset Quota'}</span>
                              </button>
                            ) : null}
                            {account.hasAuth && (
                              <button
                                type="button"
                                className="cardMenuDropdownItem"
                                aria-label="Login"
                                onClick={() => {
                                  setActiveMenuId(null);
                                  login(account);
                                }}
                              >
                                <IconKey size={13} />
                                <span>Login</span>
                              </button>
                            )}
                            <button
                              type="button"
                              className="cardMenuDropdownItem"
                              disabled={account.id === 'main'}
                              onClick={() => {
                                setActiveMenuId(null);
                                rename(account);
                              }}
                            >
                              <IconPencil size={13} />
                              <span>Rename</span>
                            </button>
                            {isApiAccount ? (
                              <button
                                type="button"
                                className="cardMenuDropdownItem"
                                onClick={() => {
                                  setActiveMenuId(null);
                                  switchModel(account);
                                }}
                              >
                                <IconPlay size={13} />
                                <span>Switch model...</span>
                              </button>
                            ) : null}
                            <button
                              type="button"
                              className="cardMenuDropdownItem cardMenuDropdownItem--danger"
                              disabled={account.id === 'main'}
                              aria-label={`Delete ${account.displayName}`}
                              onClick={() => {
                                setActiveMenuId(null);
                                deleteAccount(account);
                              }}
                            >
                              <IconTrash size={13} />
                              <span>Delete</span>
                            </button>
                          </>
                        ) : (
                          <>
                            <button
                              type="button"
                              className="cardMenuDropdownItem"
                              onClick={() => {
                                setActiveMenuId(null);
                                exportCpa(account);
                              }}
                            >
                              <IconCloud size={13} />
                              <span>Export CPA</span>
                            </button>
                            {!isApiAccount ? (
                              <button
                                type="button"
                                className="cardMenuDropdownItem"
                                disabled={!canResetQuota}
                                title={
                                  quota?.resetCreditCount
                                    ? `Reset ${account.displayName} quota`
                                    : 'No reset credits available'
                                }
                                onClick={async () => {
                                  setActiveMenuId(null);
                                  if (await confirmResetQuota(account.displayName)) {
                                    void resetAccountQuota(account.id);
                                  }
                                }}
                              >
                                <IconPlay size={13} />
                                <span>{isResetting ? 'Resetting...' : 'Reset Quota'}</span>
                              </button>
                            ) : null}
                            <button
                              type="button"
                              className="cardMenuDropdownItem"
                              disabled={account.id === 'main' || isActiveAccount}
                              onClick={() => {
                                setActiveMenuId(null);
                                rename(account);
                              }}
                            >
                              <IconPencil size={13} />
                              <span>Rename</span>
                            </button>
                          </>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              </div>
              <div className="cardTagsRow">
                {!isApiAccount ? <PlanTypeBadge planType={quota?.planType} /> : null}
                {isActiveAccount ? (
                  <span className="accountActiveBadge" aria-label="Active session account">
                    Active
                  </span>
                ) : account.hasAuth ? (
                  <span className="badge badge--auth" aria-label="Logged in account">
                    Logged in
                  </span>
                ) : !isApiAccount ? (
                  <span className="badge warn" aria-label="Login needed account">
                    Login needed
                  </span>
                ) : null}
                {isApiAccount ? (
                  <span className="badge badge--auth" aria-label="External API account">
                    External API
                  </span>
                ) : null}
                {!isApiAccount ? <AuthModeBadge authMode={account.authMode} /> : null}
                <TokenExpirationBadge status={tokenStatuses[account.id]} />
              </div>
              <p className="cardPath mono" title={account.codexHome}>
                {shortenPath(account.codexHome)}
              </p>
              <p className="cardMeta" title={metaText}>
                {metaText}
              </p>
              <AccountNotePanel account={account} onSave={onSaveAccountNote} />
              {!isApiAccount ? (
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
              ) : null}
              <div
                className={compactButtons ? 'cardActions cardActions--singleRow' : 'cardActions'}
              >
                {authMode === 'oauth' ? (
                  isApiAccount ? (
                    <>
                      <UIButton
                        size="sm"
                        variant="primary"
                        className="accountActionBtn accountActionBtn--primary"
                        onClick={(event) => {
                          event.stopPropagation();
                          switchModel(account);
                        }}
                      >
                        Switch Model
                      </UIButton>
                      <UIButton
                        size="sm"
                        variant="default"
                        className="accountActionBtn accountActionBtn--secondary"
                        disabled={accounts.length < 2}
                        onClick={(event) => {
                          event.stopPropagation();
                          openHandoff(account);
                        }}
                      >
                        <IconPlay size={13} /> Handoff
                      </UIButton>
                    </>
                  ) : account.hasAuth ? (
                    <>
                      <UIButton
                        size="sm"
                        variant="primary"
                        className={primaryBtnClass}
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
                        variant="default"
                        className={secondaryBtnClass}
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
                      {!compactButtons && (
                        <>
                          <UIButton
                            size="sm"
                            variant="default"
                            className="accountActionBtn"
                            title="Sync sessions"
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
                            variant="default"
                            className="accountActionBtn"
                            disabled={!canResetQuota}
                            title={
                              quota?.resetCreditCount
                                ? `Reset ${account.displayName} quota`
                                : 'No reset credits available'
                            }
                            onClick={async (e) => {
                              e.stopPropagation();
                              if (await confirmResetQuota(account.displayName)) {
                                void resetAccountQuota(account.id);
                              }
                            }}
                          >
                            <IconPlay size={13} />
                            {isResetting ? 'Resetting' : 'Reset Quota'}
                          </UIButton>
                        </>
                      )}
                    </>
                  ) : (
                    <UIButton
                      size="sm"
                      variant="primary"
                      className="accountActionBtn accountActionBtn--full"
                      title="Login to this account"
                      onClick={(e) => {
                        e.stopPropagation();
                        login(account);
                      }}
                    >
                      <IconKey size={13} />
                      Login
                    </UIButton>
                  )
                ) : (
                  // PAT mode
                  <>
                    {isActiveAccount ? (
                      <UIButton
                        size="sm"
                        variant="primary"
                        className={primaryBtnClass}
                        disabled={!canResetQuota}
                        aria-label={`Reset ${account.displayName} quota`}
                        title={
                          quota?.resetCreditCount
                            ? `Reset ${account.displayName} quota`
                            : 'No reset credits available'
                        }
                        onClick={async (e) => {
                          e.stopPropagation();
                          if (await confirmResetQuota(account.displayName)) {
                            void resetAccountQuota(account.id);
                          }
                        }}
                      >
                        <IconPlay size={13} />
                        {isResetting ? 'Resetting' : 'Reset Quota'}
                      </UIButton>
                    ) : (
                      <UIButton
                        size="sm"
                        variant="primary"
                        className={primaryBtnClass}
                        disabled={account.id === 'main'}
                        aria-label="Switch to this account"
                        title={
                          account.id === 'main'
                            ? 'Main profile is the active auth slot'
                            : 'Switch to this account'
                        }
                        onClick={(e) => {
                          e.stopPropagation();
                          switchAccount(account);
                        }}
                      >
                        <IconSync size={13} />
                        Switch
                      </UIButton>
                    )}
                    <UIButton
                      size="sm"
                      variant="default"
                      className={secondaryBtnClass}
                      title={
                        account.hasPersonalAccessToken
                          ? 'Update session auth JSON'
                          : 'Login to this account'
                      }
                      onClick={(e) => {
                        e.stopPropagation();
                        login(account);
                      }}
                    >
                      <IconKey size={13} />
                      {account.hasPersonalAccessToken ? 'Update' : 'Login'}
                    </UIButton>
                    {!compactButtons && (
                      <>
                        <UIButton
                          size="sm"
                          variant="default"
                          className="accountActionBtn"
                          title="Export CPA auth JSON"
                          onClick={(e) => {
                            e.stopPropagation();
                            exportCpa(account);
                          }}
                        >
                          <IconCloud size={13} />
                          Export CPA
                        </UIButton>
                        {isActiveAccount ? (
                          <UIButton
                            size="sm"
                            variant="default"
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
                        ) : (
                          <UIButton
                            size="sm"
                            variant="default"
                            className="accountActionBtn"
                            disabled={!canResetQuota}
                            title={
                              quota?.resetCreditCount
                                ? `Reset ${account.displayName} quota`
                                : 'No reset credits available'
                            }
                            onClick={async (e) => {
                              e.stopPropagation();
                              if (await confirmResetQuota(account.displayName)) {
                                void resetAccountQuota(account.id);
                              }
                            }}
                          >
                            <IconPlay size={13} />
                            {isResetting ? 'Resetting' : 'Reset Quota'}
                          </UIButton>
                        )}
                      </>
                    )}
                  </>
                )}
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

  const hasNotes = !!(account.renewalDate || account.note);
  return (
    <div
      role="button"
      tabIndex={0}
      className={`accountNoteSummary accountNoteSummary--editable ${!hasNotes ? 'accountNoteSummary--empty' : ''}`}
      title="Click to edit renewal date and notes"
      aria-label="Edit note"
      onClick={(event) => {
        event.stopPropagation();
        startEditing();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.stopPropagation();
          event.preventDefault();
          startEditing();
        }
      }}
    >
      <div className="accountNoteSummaryContent">
        {hasNotes ? (
          <>
            {account.renewalDate && <strong>Renews {account.renewalDate}</strong>}
            {account.note && <span>{account.note}</span>}
          </>
        ) : (
          <span className="accountNotePlaceholder">+ Add renewal date or note</span>
        )}
      </div>
      <span className="accountNoteSummaryEditIndicator" aria-hidden>
        <IconPencil size={12} />
      </span>
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
  themeMode: _themeMode,
  resolvedTheme: _resolvedTheme,
  divergedStrategy,
  setDivergedStrategy,
  hideDockIcon,
  setHideDockIcon,
  terminalTargets,
  terminalTargetId,
  setTerminalTargetId,
  modeAvailability,
  setModeAvailability,
  resetUsageStatistics,
  compactButtons,
  setCompactButtons,
  gatewayFirstResponseTimeoutSeconds,
  setGatewayFirstResponseTimeoutSeconds,
}: {
  health: HealthCheck | null;
  themeMode: 'system' | 'light' | 'dark';
  resolvedTheme: 'light' | 'dark';
  divergedStrategy: DivergedSessionStrategy;
  setDivergedStrategy: (strategy: DivergedSessionStrategy) => void;
  hideDockIcon: boolean;
  setHideDockIcon: (hide: boolean) => void;
  terminalTargets: TerminalTarget[];
  terminalTargetId: string;
  setTerminalTargetId: (targetId: string) => void;
  modeAvailability: 'profile' | 'pat' | 'both';
  setModeAvailability: (availability: 'profile' | 'pat' | 'both') => void;
  resetUsageStatistics: () => void;
  compactButtons: boolean;
  setCompactButtons: (compact: boolean) => void;
  gatewayFirstResponseTimeoutSeconds: number;
  setGatewayFirstResponseTimeoutSeconds: (seconds: number) => void;
}) {
  const [rateCard, setRateCard] = useState<UsageRateCardEntry[]>([]);
  const [activeTab, setActiveTab] = useState<'general' | 'advanced' | 'rate-card' | 'system'>(
    'general',
  );
  const [rateCardSearch, setRateCardSearch] = useState('');
  const [copiedHomeRoot, setCopiedHomeRoot] = useState(false);
  const [gatewayTimeoutDraft, setGatewayTimeoutDraft] = useState(
    String(gatewayFirstResponseTimeoutSeconds),
  );

  useEffect(() => {
    setGatewayTimeoutDraft(String(gatewayFirstResponseTimeoutSeconds));
  }, [gatewayFirstResponseTimeoutSeconds]);

  const commitGatewayTimeout = () => {
    const seconds = Number(gatewayTimeoutDraft);
    if (!Number.isInteger(seconds) || seconds < 10 || seconds > 600) {
      setGatewayTimeoutDraft(String(gatewayFirstResponseTimeoutSeconds));
      return;
    }
    setGatewayFirstResponseTimeoutSeconds(seconds);
  };

  const installedTerminalTargets = terminalTargets.filter((target) => target.installed);
  useEffect(() => {
    let active = true;
    getUsageRateCard()
      .then((entries) => {
        if (active) setRateCard(entries);
      })
      .catch(() => {
        if (active) setRateCard([]);
      });
    return () => {
      active = false;
    };
  }, []);

  const handleCopyHomeRoot = () => {
    if (health?.homeRoot) {
      navigator.clipboard.writeText(health.homeRoot);
      setCopiedHomeRoot(true);
      setTimeout(() => setCopiedHomeRoot(false), 2000);
    }
  };

  const filteredRateCard = useMemo(() => {
    if (!rateCardSearch.trim()) return rateCard;
    const q = rateCardSearch.toLowerCase();
    return rateCard.filter(
      (entry) =>
        entry.model.toLowerCase().includes(q) || entry.pricingModel.toLowerCase().includes(q),
    );
  }, [rateCard, rateCardSearch]);

  return (
    <section className="panel pagePanel settingsPagePanel">
      <div className="settingsLayout">
        {/* Left Sidebar */}
        <aside className="settingsSidebar">
          <nav className="settingsSidebarNav" aria-label="Settings Categories">
            <div className="settingsSidebarSectionHeader">App Settings</div>
            <button
              type="button"
              className={`settingsSidebarBtn ${activeTab === 'general' ? 'active' : ''}`}
              onClick={() => setActiveTab('general')}
            >
              <IconSliders size={16} />
              <span>General</span>
            </button>
            <button
              type="button"
              className={`settingsSidebarBtn ${activeTab === 'advanced' ? 'active' : ''}`}
              onClick={() => setActiveTab('advanced')}
            >
              <IconKey size={16} />
              <span>Advanced</span>
            </button>

            <div className="settingsSidebarSectionHeader">Integrations</div>
            <button
              type="button"
              className={`settingsSidebarBtn ${activeTab === 'system' ? 'active' : ''}`}
              onClick={() => setActiveTab('system')}
            >
              <IconDevice size={16} />
              <span>System & Desktop</span>
            </button>

            <div className="settingsSidebarSectionHeader">Resources</div>
            <button
              type="button"
              className={`settingsSidebarBtn ${activeTab === 'rate-card' ? 'active' : ''}`}
              onClick={() => setActiveTab('rate-card')}
            >
              <IconCoins size={16} />
              <span>Rate Card</span>
            </button>
          </nav>
          <div className="settingsSidebarFooter">
            <div className="settingsSidebarFooterRow">
              <span className="settingsSidebarFooterLabel">Status</span>
              <span
                className={`settingsSidebarFooterValue ${health?.ok ? 'status--ok' : 'status--error'}`}
              >
                <span className="statusDot" />
                {health?.ok ? 'Connected' : 'Disconnected'}
              </span>
            </div>
            <div className="settingsSidebarFooterRow">
              <span className="settingsSidebarFooterLabel">Version</span>
              <span className="settingsSidebarFooterValue">{health?.version ?? '0.1.1'}</span>
            </div>
            {health?.homeRoot && (
              <div
                className="settingsSidebarFooterRow settingsSidebarFooterRow--path"
                onClick={handleCopyHomeRoot}
                title="Click to copy Home Root path"
              >
                <span className="settingsSidebarFooterLabel">Home Root</span>
                <span className="settingsSidebarFooterValue pathValue">
                  {health.homeRoot}
                  {copiedHomeRoot ? (
                    <span className="copiedHint">Copied!</span>
                  ) : (
                    <IconCopy size={11} className="copyIcon" />
                  )}
                </span>
              </div>
            )}
          </div>
        </aside>

        {/* Right Content Pane */}
        <div className="settingsContent">
          {activeTab === 'general' && (
            <div className="settingsTabContent">
              <div className="settingsContentHeader">
                <h4>General Settings</h4>
                <p>Configure core application parameters and system details.</p>
              </div>

              {/* App Preferences */}
              <div className="settingsSectionTitle">App Preferences</div>
              <div className="settingsGroupCard">
                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <label htmlFor="modeAvailabilitySelect" className="settingsRowTitle">
                      Mode availability
                    </label>
                    <p className="settingsRowDesc">
                      Controls which account mode information and titlebar controls are shown.
                    </p>
                  </div>
                  <div className="settingsRowControl">
                    <select
                      id="modeAvailabilitySelect"
                      value={modeAvailability}
                      onChange={(event) =>
                        setModeAvailability(event.target.value as 'profile' | 'pat' | 'both')
                      }
                    >
                      <option value="profile">Profile Only</option>
                      <option value="pat">PAT Only</option>
                      <option value="both">Profile & PAT</option>
                    </select>
                  </div>
                </div>

                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <label htmlFor="terminalTargetSelect" className="settingsRowTitle">
                      Handoff terminal
                    </label>
                    <p className="settingsRowDesc">
                      Terminal app used by profile relay, resume, and login commands.
                    </p>
                  </div>
                  <div className="settingsRowControl">
                    <select
                      id="terminalTargetSelect"
                      value={terminalTargetId}
                      onChange={(event) => setTerminalTargetId(event.target.value)}
                    >
                      {(installedTerminalTargets.length
                        ? installedTerminalTargets
                        : [
                            {
                              id: 'terminal',
                              displayName: 'Terminal.app',
                              kind: 'terminal',
                              installed: true,
                            },
                          ]
                      ).map((target) => (
                        <option key={target.id} value={target.id}>
                          {target.displayName}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>

                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <label htmlFor="compactButtonsSelect" className="settingsRowTitle">
                      Compact card actions
                    </label>
                    <p className="settingsRowDesc">
                      Use a compact 2-button layout on account cards. Disable to display more
                      actions directly.
                    </p>
                  </div>
                  <div className="settingsRowControl">
                    <select
                      id="compactButtonsSelect"
                      value={compactButtons ? 'true' : 'false'}
                      onChange={(event) => setCompactButtons(event.target.value === 'true')}
                    >
                      <option value="true">Enabled (Compact)</option>
                      <option value="false">Disabled (Show all actions)</option>
                    </select>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'advanced' && (
            <div className="settingsTabContent">
              <div className="settingsContentHeader">
                <h4>Advanced Settings</h4>
                <p>Configure conflict strategies, statistics tracking, and background processes.</p>
              </div>

              {/* Session Strategy */}
              <div className="settingsSectionTitle">Session Conflict Resolution</div>
              <div className="settingsGroupCard">
                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <label htmlFor="divergedStrategySelect" className="settingsRowTitle">
                      Diverged session strategy
                    </label>
                    <p className="settingsRowDesc">
                      Strategy used when both local and remote accounts continued the same session
                      differently.
                    </p>
                  </div>
                  <div className="settingsRowControl">
                    <select
                      id="divergedStrategySelect"
                      value={divergedStrategy}
                      onChange={(event) =>
                        setDivergedStrategy(event.target.value as DivergedSessionStrategy)
                      }
                    >
                      <option value="summarize_fork_with_target_account">
                        Summarize fork with target account
                      </option>
                      <option value="stop_and_ask">Stop and ask</option>
                      <option value="timeline_merge_to_fork">Timeline merge to fork</option>
                      <option value="prefer_source">Prefer source with backup</option>
                      <option value="prefer_target">Prefer target and save source fork</option>
                    </select>
                  </div>
                </div>
              </div>

              <div className="settingsSectionTitle">Gateway Timeouts</div>
              <div className="settingsGroupCard">
                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <label htmlFor="gatewayFirstResponseTimeout" className="settingsRowTitle">
                      Gateway first response timeout
                    </label>
                    <p className="settingsRowDesc">
                      Maximum seconds to wait for upstream response headers. Valid range: 10–600.
                      Changes apply on the next Gateway launch; LAM will not restart it
                      automatically.
                    </p>
                  </div>
                  <div className="settingsRowControl">
                    <input
                      id="gatewayFirstResponseTimeout"
                      type="number"
                      min={10}
                      max={600}
                      step={1}
                      value={gatewayTimeoutDraft}
                      onChange={(event) => setGatewayTimeoutDraft(event.target.value)}
                      onBlur={commitGatewayTimeout}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') commitGatewayTimeout();
                      }}
                    />
                  </div>
                </div>
              </div>

              {/* Data & History */}
              <div className="settingsSectionTitle">Data & History (Danger Zone)</div>
              <div className="settingsGroupCard">
                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <span className="settingsRowTitle">Usage statistics</span>
                    <p className="settingsRowDesc">LAM-owned token tracker database state.</p>
                  </div>
                  <div className="settingsRowControl">
                    <span className="settingsStaticText">Active</span>
                  </div>
                </div>

                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <span className="settingsRowTitle">Reset usage statistics history</span>
                    <p className="settingsRowDesc">
                      Clear all local estimates and cached events from SQLite databases.
                    </p>
                  </div>
                  <div className="settingsRowControl">
                    <UIButton variant="danger" size="sm" onClick={resetUsageStatistics}>
                      Reset Usage Statistics
                    </UIButton>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'rate-card' && (
            <div className="settingsTabContent">
              <div className="settingsContentHeader">
                <h4>Usage Rate Card</h4>
                <p>Built-in local estimates for token pricing. Prices are in USD per 1M tokens.</p>
              </div>

              <div className="rateCardFilterRow">
                <input
                  type="text"
                  placeholder="Search models..."
                  value={rateCardSearch}
                  onChange={(e) => setRateCardSearch(e.target.value)}
                  className="rateCardSearchInput"
                />
              </div>

              <div className="usageTableWrap settingsRateCardTable">
                <table className="usageTable">
                  <thead>
                    <tr>
                      <th>Model</th>
                      <th>Context</th>
                      <th>Input</th>
                      <th>Cached input</th>
                      <th>Output</th>
                      <th>Pricing model</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredRateCard.map((entry) => (
                      <tr key={`${entry.model}-${entry.contextWindow}`}>
                        <td>
                          <strong>{entry.model}</strong>
                        </td>
                        <td>{entry.contextWindow}</td>
                        <td>{formatCost(entry.inputPerMillion)}</td>
                        <td>{formatCost(entry.cachedInputPerMillion)}</td>
                        <td>{formatCost(entry.outputPerMillion)}</td>
                        <td>
                          {entry.pricingModel}
                          {entry.estimated ? ' (estimated)' : ''}
                        </td>
                      </tr>
                    ))}
                    {filteredRateCard.length === 0 ? (
                      <tr>
                        <td
                          colSpan={6}
                          style={{ textAlign: 'center', padding: '36px 0', color: 'var(--muted)' }}
                        >
                          No matching rate card entries found.
                        </td>
                      </tr>
                    ) : null}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {activeTab === 'system' && (
            <div className="settingsTabContent">
              <div className="settingsContentHeader">
                <h4>System & Desktop Integration</h4>
                <p>Configure macOS system integration, Dock behaviour, and tray sync schedules.</p>
              </div>

              {/* Desktop Integration */}
              <div className="settingsSectionTitle">macOS System Integration</div>
              <div className="settingsGroupCard">
                <div className="settingsRowLayout">
                  <div className="settingsRowInfo">
                    <label htmlFor="hideDockIconSelect" className="settingsRowTitle">
                      Dock icon behavior
                    </label>
                    <p className="settingsRowDesc">
                      Hide Dock icon on macOS while maintaining the status bar menu tray (Accessory
                      mode).
                    </p>
                  </div>
                  <div className="settingsRowControl">
                    <select
                      id="hideDockIconSelect"
                      value={hideDockIcon ? 'true' : 'false'}
                      onChange={(event) => setHideDockIcon(event.target.value === 'true')}
                    >
                      <option value="false">Show Dock icon</option>
                      <option value="true">Hide Dock icon (Accessory mode)</option>
                    </select>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
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
