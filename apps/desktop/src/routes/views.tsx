import { countAccountsWithAvailableQuota, countAccountsWithQuotaData } from "../lib/quota";
import type { CodexAccount, CodexSession, DivergedSessionStrategy, HealthCheck, ProviderProfile, UsageQuotaSnapshot } from "../lib/types";
import { QuotaWindow } from "../components/quota-window";
import { IconCopy, IconProviders, IconInfo, MetricIcon, type MetricIconName } from "../components/icons";
import { UIButton } from "../components/ui-button";

export function Overview({
  accounts,
  quotas,
  providers,
  select,
  openSync,
  login,
  relayResume,
  currentSession,
  refreshAccountQuota,
  refreshingQuotaIds,
}: {
  accounts: CodexAccount[];
  quotas: UsageQuotaSnapshot[];
  providers: ProviderProfile[];
  select: (id: string) => void;
  openSync: (id: string) => void;
  login: (account: CodexAccount) => void;
  relayResume: (account: CodexAccount) => void;
  currentSession?: CodexSession;
  refreshAccountQuota: (profileId: string) => void;
  refreshingQuotaIds: string[];
}) {
  const accountsWithQuotaData = countAccountsWithQuotaData(quotas);
  const availableQuotaAccounts = countAccountsWithAvailableQuota(accounts, quotas);
  const sessionTotal = accounts.reduce((sum, account) => sum + account.sessionCount, 0);
  return (
    <div className="overviewPage">
      <div className="metricGrid">
        <Metric icon="accounts" label="Accounts" value={`${accountsWithQuotaData}/${accounts.length || 0}`} />
        <Metric icon="sessions" label="Sessions" value={sessionTotal} />
        <Metric icon="providers" label="Providers" value={providers.length} />
        <Metric icon="quota" label="Quota usable" value={availableQuotaAccounts} />
      </div>
      <Accounts
        accounts={accounts}
        quotas={quotas}
        select={select}
        openSync={openSync}
        login={login}
        relayResume={relayResume}
        currentSession={currentSession}
        refreshAccountQuota={refreshAccountQuota}
        refreshingQuotaIds={refreshingQuotaIds}
        variant="overview"
      />
    </div>
  );
}

export function Accounts({
  accounts,
  quotas,
  select,
  openSync,
  login,
  relayResume,
  currentSession,
  refreshAccountQuota,
  refreshingQuotaIds,
  variant = "default",
}: {
  accounts: CodexAccount[];
  quotas: UsageQuotaSnapshot[];
  select: (id: string) => void;
  openSync: (id: string) => void;
  login: (account: CodexAccount) => void;
  relayResume: (account: CodexAccount) => void;
  currentSession?: CodexSession;
  refreshAccountQuota: (profileId: string) => void;
  refreshingQuotaIds: string[];
  variant?: "default" | "overview";
}) {
  if (!accounts.length) return <div className="emptyBox">No Codex profiles found.</div>;
  const activeAccount = currentSession ? accounts.find((account) => account.id === currentSession.accountId) : undefined;
  const orderedAccounts = [...accounts].sort((a, b) => {
    const latestDiff = (b.latestSessionModifiedAt ?? 0) - (a.latestSessionModifiedAt ?? 0);
    if (latestDiff !== 0) return latestDiff;
    return b.sessionCount - a.sessionCount;
  });
  return (
    <section className={variant === "overview" ? "overviewAccountsPanel" : "panel pagePanel"}>
      <div className="panelHead">
        <h3 className="sectionTitle">Accounts</h3>
      </div>
      <div className="activeSessionBanner">
        <span>Active source</span>
        <strong>{activeAccount?.displayName ?? currentSession?.accountId ?? "No active session"}</strong>
        <em className="mono">{currentSession?.id ?? "No session found"}</em>
      </div>
      <div className="cardGrid accountCardGrid">
        {orderedAccounts.map((account) => {
          const isRefreshing = refreshingQuotaIds.includes(account.id);
          const quota = quotas.find((item) => item.profileId === account.id);
          const providerLabel = account.providerId ?? "unknown";
          const modelLabel = account.model ?? "unknown";
          const isActiveAccount = currentSession?.accountId === account.id;
          return (
            <article className="card accountCard" key={account.id} onClick={() => select(account.id)}>
              <div className="cardHead">
                <div className="cardTitleRow">
                  <h3>{account.displayName}</h3>
                  {isActiveAccount ? (
                    <span className="accountActiveBadge" aria-label="Active session account">
                      Active
                    </span>
                  ) : null}
                </div>
                <div className="cardHeadActions">
                  <UIButton
                    variant="icon"
                    size="sm"
                    className={`iconCircleBtn ${isRefreshing ? "isSpinning" : ""}`}
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
                  <span className={account.hasAuth ? "badge badge--auth" : "badge warn"}>
                    {account.hasAuth ? "Logged in" : "Login needed"}
                  </span>
                </div>
              </div>
              <p className="cardPath mono" title={account.codexHome}>{account.codexHome}</p>
              <p
                className="cardMeta"
                title={`${account.sessionCount} sessions · Provider: ${providerLabel} · ${modelLabel}`}
              >
                {account.sessionCount} sessions · Provider: {providerLabel} · {modelLabel}
              </p>
              <div className="accountQuota">
                <QuotaWindow
                  label="Session (5h)"
                  usedPercent={quota?.primaryUsedPercent}
                  resetAt={quota?.resetAt}
                  variant="session"
                />
                <QuotaWindow
                  label="Weekly (7d)"
                  usedPercent={quota?.secondaryUsedPercent}
                  resetAt={quota?.secondaryResetAt}
                  variant="weekly"
                />
              </div>
              <div className="cardActions">
                <UIButton
                  size="sm"
                  variant="primary"
                  disabled={!currentSession}
                  aria-label="Resume Here"
                  title={currentSession ? `Resume Here latest active session ${currentSession.id} with ${account.displayName}` : "No active session found"}
                  onClick={(e) => {
                    e.stopPropagation();
                    relayResume(account);
                  }}
                >
                  {currentSession?.accountId === account.id ? "Continue" : "Relay"}
                </UIButton>
                <UIButton size="sm" onClick={(e) => { e.stopPropagation(); openSync(account.id); }}>↑ Sync To...</UIButton>
                <UIButton size="sm" onClick={(e) => { e.stopPropagation(); login(account); }}>→ Login</UIButton>
              </div>
            </article>
          );
        })}
      </div>
    </section>
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
          <input placeholder="Search cwd / summary / id" value={query} onChange={(e) => setQuery(e.target.value)} />
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
              {sessions.map((session) => (
                <tr key={`${session.accountId}-${session.path}`} onClick={() => details(session)}>
                  <td>
                    <div className="sessionIdCell">
                      <strong className="cellTrunc">{session.id}</strong>
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
                    {session.providerMismatch ? <span className="badge warn">provider mismatch</span> : null}
                  </td>
                  <td>
                    <span className="mono cellTrunc" title={session.cwd ?? "unknown"}>{session.cwd ?? "unknown"}</span>
                  </td>
                  <td>
                    <strong className="cellTrunc" title={session.currentProviderId ?? session.model ?? "unknown"}>
                      {session.currentProviderId ?? session.model ?? "unknown"}
                    </strong>
                    <div className="cellSub mono cellTrunc" title={session.currentModel ?? session.model ?? "unknown"}>
                      {session.currentModel ?? session.model ?? "unknown"}
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
                          details(session);
                        }}
                      >
                        ⓘ Details
                      </UIButton>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="emptyBox">No sessions. Unknown cwd values will display as `unknown`.</div>
      )}
    </section>
  );
}

export function Relay({ accounts, openRelay, openSync }: { accounts: CodexAccount[]; openRelay: () => void; openSync: (id?: string) => void }) {
  const relays = accounts.filter((account) => account.isRelay);
  return (
    <section className="panel pagePanel">
      <div className="panelHead">
        <h3 className="sectionTitle">Relay</h3>
        <UIButton variant="primary" onClick={openRelay}>+ Create Relay</UIButton>
      </div>
      {relays.length ? (
        relays.map((relay) => (
          <div className="configRow" key={relay.id}>
            <span>{relay.id}</span>
            <strong>{relay.relayIdentity ?? "?"} relays {relay.relaySource ?? "?"}</strong>
            <UIButton size="sm" onClick={() => openSync(relay.relaySource ?? undefined)}>Safe Sync</UIButton>
          </div>
        ))
      ) : (
        <div className="emptyState">
          <p>No relay profiles yet.</p>
        </div>
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
          <UIButton variant="primary" onClick={create}>Add Provider</UIButton>
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
                  <span>ID</span><strong>{provider.id}</strong>
                  <span>Model</span><strong>{provider.defaultModel}</strong>
                  <span>Secret</span><strong>{provider.secretStorage}{provider.envKey ? ` · ${provider.envKey}` : ""}</strong>
                </div>
                <div className="cardActions">
                  <UIButton size="sm" onClick={() => test(provider.id)}>Test</UIButton>
                  <UIButton size="sm" onClick={() => attach(provider.id)}>Attach</UIButton>
                  <UIButton variant="danger" size="sm" onClick={() => remove(provider.id)}>Delete</UIButton>
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
          <span>API keys are never returned to the UI. Provider store contains metadata and secret references only.</span>
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
              <strong>{account.providerId ?? "unknown"} · {account.model ?? "unknown"}</strong>
              <em>{account.authMode ?? "unknown"}</em>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

export function SyncHome({ accounts, openSync }: { accounts: CodexAccount[]; openSync: () => void }) {
  return (
    <section className="panel pagePanel">
      <div className="panelHead">
        <h3 className="sectionTitle">Sync</h3>
        <UIButton variant="primary" disabled={accounts.length < 2} onClick={() => openSync()}>Open Sync</UIButton>
      </div>
      <div className="rows">
        <div><span>Default include</span><strong>sessions/</strong><em>Phase 1</em></div>
        <div><span>Blocked</span><strong>auth.json, config.toml, sqlite, cache, tmp, logs</strong><em>Strict</em></div>
        <div><span>History</span><strong>sidecar backup only</strong><em>No merge</em></div>
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
  themeMode: "system" | "light" | "dark";
  resolvedTheme: "light" | "dark";
  divergedStrategy: DivergedSessionStrategy;
  setDivergedStrategy: (strategy: DivergedSessionStrategy) => void;
}) {
  return (
    <section className="panel pagePanel">
      <h3 className="sectionTitle">Settings</h3>
      <div className="rows">
        <div><span>Status</span><strong>{health?.ok ? "connected" : "not connected"}</strong><em>{health?.version ?? "unknown"}</em></div>
        <div><span>Home root</span><strong>{health?.homeRoot ?? "unknown"}</strong><em>LAM_HOME or HOME</em></div>
        <div><span>Theme mode</span><strong>{themeMode}</strong><em>resolved: {resolvedTheme}</em></div>
        <label className="settingsSelectRow">
          <span>Diverged session strategy</span>
          <select
            value={divergedStrategy}
            onChange={(event) => setDivergedStrategy(event.target.value as DivergedSessionStrategy)}
          >
            <option value="summarize_fork_with_target_account">Summarize fork with target account</option>
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

export { PlanView } from "../components/plan-view";

function Metric({ icon, label, value }: { icon: MetricIconName; label: string; value: number | string }) {
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
