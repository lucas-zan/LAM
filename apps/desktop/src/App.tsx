import { useEffect, useMemo, useRef, useState } from "react";
import {
  attachProviderToProfile,
  buildLoginCommand,
  buildResumeCommand,
  buildSyncPlan,
  createProvider,
  deleteProvider,
  executeCreateAccount,
  executeCreateRelay,
  executeSync,
  healthCheck,
  inTauri,
  listAccounts,
  listCachedQuotas,
  listProviders,
  listSessions,
  openTerminalForLogin,
  openTerminalWithCommand,
  openTerminalWithResume,
  planAttachProviderToProfile,
  planCreateAccount,
  planCreateRelay,
  relayResumeSession,
  getProfileQuota,
  syncTrayQuota,
  testProvider,
} from "./lib/api";
import * as Shell from "./components/shell";
import { IconClock, IconLogo, IconRefresh } from "./components/icons";
import { SyncModal } from "./components/sync-modal";
import { ThemeToggle } from "./components/theme-toggle";
import { UIButton } from "./components/ui-button";
import type { ThemeMode } from "./lib/theme";
import type {
  CodexAccount,
  CodexSession,
  AttachProviderRequest,
  CreateAccountRequest,
  CreateProviderRequest,
  CreateRelayRequest,
  DivergedSessionStrategy,
  HealthCheck,
  OperationPlan,
  ProviderProfile,
  ResumeCommand,
  SyncPlan,
  SyncRequest,
  SyncResult,
  UsageQuotaSnapshot,
} from "./lib/types";
import { routeTitle as routeTitleFromModule } from "./routes/types";
import * as Views from "./routes/views";

type Route = "overview" | "sessions" | "relay" | "providers" | "sync" | "settings";
type Modal = "account" | "relay" | "sync" | "provider" | "attachProvider" | "sessionDetail" | null;
const emptyAccountReq: CreateAccountRequest = {
  name: "luna",
  copyConfigFrom: null,
  overwriteWrapper: false,
};

const emptyProviderReq: CreateProviderRequest = {
  id: "company-proxy",
  name: "Company Proxy",
  baseUrl: "https://proxy.example.test/v1",
  wireApi: "openai",
  defaultModel: "gpt-5-codex",
  envKey: "COMPANY_PROXY_API_KEY",
  secret: { kind: "env", envKey: "COMPANY_PROXY_API_KEY" },
};

const QUOTA_INITIAL_DELAY_MS = 8_000;
const QUOTA_REFRESH_INTERVAL_MS = 2 * 60_000;

function readDivergedStrategy(): DivergedSessionStrategy {
  const saved = localStorage.getItem("lam-diverged-session-strategy");
  if (
    saved === "stop_and_ask" ||
    saved === "summarize_fork_with_target_account" ||
    saved === "timeline_merge_to_fork" ||
    saved === "prefer_source" ||
    saved === "prefer_target"
  ) {
    return saved;
  }
  return "summarize_fork_with_target_account";
}

export function App() {
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => {
    const saved = localStorage.getItem("lam-theme");
    if (saved === "system" || saved === "light" || saved === "dark") return saved;
    return "system";
  });
  const [route, setRoute] = useState<Route>("overview");
  const [accounts, setAccounts] = useState<CodexAccount[]>([]);
  const [selectedAccountId, setSelectedAccountId] = useState<string>("");
  const [sessions, setSessions] = useState<CodexSession[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string>("");
  const [activeSession, setActiveSession] = useState<CodexSession | undefined>(undefined);
  const [divergedStrategy, setDivergedStrategy] = useState<DivergedSessionStrategy>(() => readDivergedStrategy());
  const [health, setHealth] = useState<HealthCheck | null>(null);
  const [status, setStatus] = useState("Ready");
  const [error, setError] = useState("");
  const [modal, setModal] = useState<Modal>(null);
  const [accountReq, setAccountReq] = useState<CreateAccountRequest>(emptyAccountReq);
  const [relayReq, setRelayReq] = useState<CreateRelayRequest>({
    runtimeProfileId: "",
    sourceProfileId: "",
    name: null,
    providerPolicy: "inherit_runtime",
    overwriteWrapper: false,
  });
  const [plan, setPlan] = useState<OperationPlan | SyncPlan | null>(null);
  const [syncReq, setSyncReq] = useState<SyncRequest | null>(null);
  const [syncResult, setSyncResult] = useState<SyncResult | null>(null);
  const [resume, setResume] = useState<ResumeCommand | null>(null);
  const [query, setQuery] = useState("");
  const [providers, setProviders] = useState<ProviderProfile[]>([]);
  const [quotas, setQuotas] = useState<UsageQuotaSnapshot[]>([]);
  const [providerReq, setProviderReq] = useState<CreateProviderRequest>(emptyProviderReq);
  const [attachReq, setAttachReq] = useState<AttachProviderRequest>({
    profileId: "",
    providerId: "",
    model: null,
  });
  const [refreshingQuotaIds, setRefreshingQuotaIds] = useState<string[]>([]);
  const [appReady, setAppReady] = useState(false);
  const quotaTimerRef = useRef<number | null>(null);
  const quotaRefreshInFlightRef = useRef(false);

  const selectedAccount = accounts.find((account) => account.id === selectedAccountId) ?? accounts[0];
  const selectedSession = sessions.find((session) => session.id === selectedSessionId) ?? sessions[0];
  const activeSessionAccount = activeSession
    ? accounts.find((account) => account.id === activeSession.accountId)
    : undefined;
  const selectedSessionAccount = selectedSession
    ? accounts.find((account) => account.id === selectedSession.accountId)
    : undefined;

  const filteredSessions = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return sessions;
    return sessions.filter((session) =>
      [session.id, session.cwd, session.summary, session.path, session.model]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
        .includes(needle),
    );
  }, [query, sessions]);

  const resolvedTheme = useMemo(() => {
    if (themeMode === "system") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }
    return themeMode;
  }, [themeMode]);

  async function refresh() {
    setError("");
    try {
      const [healthData, accountData, providerData] = await Promise.all([
        healthCheck(),
        listAccounts(),
        listProviders(),
      ]);
      setHealth(healthData);
      setAccounts(accountData);
      setProviders(providerData);
      const nextAccount = selectedAccountId || accountData[0]?.id || "";
      setSelectedAccountId(nextAccount);
      setAppReady(true);

      if (nextAccount) {
        void listSessions(nextAccount)
          .then((items) => {
            setSessions(items);
            setSelectedSessionId(items[0]?.id ?? "");
          })
          .catch((err) => setError(formatError(err)));
      } else {
        setSessions([]);
      }

      if (accountData.length) {
        void refreshActiveSession(accountData);
        void loadCachedQuotas(accountData.map((account) => account.id));
        scheduleQuotaRefresh(accountData.map((account) => account.id), QUOTA_INITIAL_DELAY_MS);
      } else {
        setQuotas([]);
      }
      setStatus(`Loaded ${accountData.length} accounts`);
    } catch (err) {
      setAppReady(true);
      setError(formatError(err));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  useEffect(() => {
    localStorage.setItem("lam-theme", themeMode);
  }, [themeMode]);

  useEffect(() => {
    localStorage.setItem("lam-diverged-session-strategy", divergedStrategy);
  }, [divergedStrategy]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      const next = themeMode === "system" ? (media.matches ? "dark" : "light") : themeMode;
      document.documentElement.dataset.theme = next;
    };
    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [themeMode]);

  useEffect(() => {
    if (!selectedAccountId) return;
    listSessions(selectedAccountId)
      .then((items) => {
        setSessions(items);
        setSelectedSessionId(items[0]?.id ?? "");
      })
      .catch((err) => setError(formatError(err)));
  }, [selectedAccountId]);

  useEffect(() => {
    if (!accounts.length) return;
    const timer = window.setInterval(() => {
      scheduleQuotaRefresh(accounts.map((account) => account.id), 0);
    }, QUOTA_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [accounts]);

  useEffect(() => {
    if (!accounts.length) return;
    void refreshActiveSession(accounts);
  }, [accounts]);

  useEffect(() => {
    return () => {
      if (quotaTimerRef.current !== null) {
        window.clearTimeout(quotaTimerRef.current);
      }
    };
  }, []);

  async function previewAccount() {
    setPlan(await planCreateAccount(accountReq));
  }

  async function createAccount() {
    await executeCreateAccount(accountReq);
    setModal(null);
    setPlan(null);
    await refresh();
  }

  async function openRelayModal() {
    const runtime = selectedAccount?.id ?? accounts[0]?.id ?? "";
    const source = accounts.find((account) => account.id !== runtime)?.id ?? accounts[0]?.id ?? "";
    setRelayReq({
      runtimeProfileId: runtime,
      sourceProfileId: source,
      name: runtime && source ? `${runtime}-relay-${source}` : null,
      providerPolicy: "inherit_runtime",
      overwriteWrapper: false,
    });
    setPlan(null);
    setModal("relay");
  }

  async function previewRelay() {
    setPlan(await planCreateRelay(relayReq));
  }

  async function createRelay() {
    await executeCreateRelay(relayReq);
    setModal(null);
    setPlan(null);
    await refresh();
  }

  async function openSyncModal(from = selectedAccount?.id) {
    const target = accounts.find((account) => account.isRelay)?.id ?? accounts.find((account) => account.id !== from)?.id ?? "";
    const req = {
      fromProfileId: from ?? "",
      toProfileId: target,
      syncSessions: true,
      backupTargetSessions: true,
      sidecarBackupHistory: false,
    };
    setSyncReq(req);
    setPlan(null);
    setSyncResult(null);
    setModal("sync");
  }

  async function previewSync() {
    if (!syncReq) return;
    setPlan(await buildSyncPlan(syncReq));
  }

  async function runSync() {
    if (!syncReq || !plan) return;
    setSyncResult(await executeSync(syncReq));
    await refresh();
  }

  async function previewResume(session = selectedSession) {
    if (!session) return;
    const command = await buildResumeCommand({
      profileId: session.accountId,
      sessionId: session.id,
      cwd: session.cwd,
    });
    setResume(command);
  }

  async function openSessionDetails(session = selectedSession) {
    if (!session) return;
    setSelectedSessionId(session.id);
    await previewResume(session);
    setModal("sessionDetail");
  }

  async function copyResume(session = selectedSession) {
    if (!session) return;
    const command = await buildResumeCommand({
      profileId: session.accountId,
      sessionId: session.id,
      cwd: session.cwd,
    });
    await navigator.clipboard.writeText(command.command);
    setResume(command);
    setStatus("Resume command copied");
  }

  async function openResume(session = selectedSession) {
    if (!session) return;
    try {
      await openTerminalWithResume({
        profileId: session.accountId,
        sessionId: session.id,
        cwd: session.cwd,
      });
      setStatus("Terminal resume opened");
    } catch (err) {
      setError(`${formatError(err)}. Copy command fallback is available.`);
      await previewResume(session);
    }
  }

  async function refreshActiveSession(accountData = accounts) {
    if (!accountData.length) {
      setActiveSession(undefined);
      return;
    }
    const results = await Promise.allSettled(accountData.map((account) => listSessions(account.id)));
    const allSessions = results.flatMap((result) => (result.status === "fulfilled" ? result.value : []));
    const latest = allSessions.sort((a, b) => b.modifiedAt - a.modifiedAt)[0];
    setActiveSession(latest);
  }

  async function relayResumeTo(account: CodexAccount) {
    const sourceSession = activeSession;
    if (!sourceSession) {
      setError("No active source session found for Resume Here.");
      return;
    }
    try {
      if (account.id === sourceSession.accountId) {
        await openResume(sourceSession);
        return;
      }
      const result = await relayResumeSession({
        fromProfileId: sourceSession.accountId,
        toProfileId: account.id,
        sessionId: sourceSession.id,
        cwd: sourceSession.cwd,
        divergedStrategy,
      });
      setSelectedAccountId(account.id);
      setSelectedSessionId(sourceSession.id);
      setResume(result.resume);
      await openTerminalWithCommand(result.resume.command);
      const actionLabel = result.action === "already_current" ? "already current" : result.action;
      setStatus(`Resume Here ${actionLabel}: ${sourceSession.id} on ${account.id}`);
      void refreshActiveSession(accounts);
      if (result.warnings.length) {
        setError(result.warnings.join(" "));
      }
    } catch (err) {
      setError(`${formatError(err)}. Existing session was not overwritten.`);
    }
  }

  async function login(account = selectedAccount) {
    if (!account) return;
    try {
      await openTerminalForLogin(account.id);
    } catch (err) {
      const command = await buildLoginCommand(account.id);
      setResume(command);
      setError(`${formatError(err)}. Copy login command fallback is available.`);
    }
  }

  async function refreshQuotas(profileIds?: string[]) {
    const targets = profileIds?.length ? profileIds : accounts.map((account) => account.id);
    if (!targets.length) return;
    if (quotaRefreshInFlightRef.current) return;
    quotaRefreshInFlightRef.current = true;
    setRefreshingQuotaIds((current) => Array.from(new Set([...current, ...targets])));
    try {
      const results = await Promise.allSettled(targets.map((profileId) => getProfileQuota(profileId, true)));
      const snapshots: UsageQuotaSnapshot[] = [];
      const warnings: string[] = [];
      results.forEach((result, index) => {
        if (result.status === "fulfilled") {
          snapshots.push(result.value);
        } else {
          warnings.push(`${targets[index]}: ${formatError(result.reason)}`);
        }
      });
      setQuotas((current) => {
        const next = new Map(current.map((item) => [item.profileId, item]));
        for (const snapshot of snapshots) next.set(snapshot.profileId, snapshot);
        return Array.from(next.values());
      });
      setStatus(warnings.length ? `Refreshed ${snapshots.length} quota snapshots; ${warnings.length} unavailable` : `Refreshed ${snapshots.length} quota snapshots`);
      void syncTrayQuota();
    } finally {
      quotaRefreshInFlightRef.current = false;
      setRefreshingQuotaIds((current) => current.filter((id) => !targets.includes(id)));
    }
  }

  function scheduleQuotaRefresh(profileIds: string[], delayMs = QUOTA_INITIAL_DELAY_MS) {
    if (!profileIds.length) return;
    if (quotaTimerRef.current !== null) {
      window.clearTimeout(quotaTimerRef.current);
    }
    quotaTimerRef.current = window.setTimeout(() => {
      quotaTimerRef.current = null;
      void refreshQuotas(profileIds);
    }, delayMs);
  }

  async function loadCachedQuotas(profileIds?: string[]) {
    try {
      const cached = await listCachedQuotas(profileIds);
      if (!cached.length) return;
      setQuotas((current) => {
        const next = new Map(current.map((item) => [item.profileId, item]));
        for (const snapshot of cached) next.set(snapshot.profileId, snapshot);
        return Array.from(next.values());
      });
      setStatus(`Loaded ${cached.length} cached quota snapshots`);
    } catch (err) {
      setError(formatError(err));
    }
  }

  function refreshAccountQuota(profileId: string) {
    void refreshQuotas([profileId]);
  }

  async function createProviderFromModal() {
    const envKey = providerReq.envKey?.trim() || null;
    await createProvider({
      ...providerReq,
      envKey,
      secret: envKey ? { kind: "env", envKey } : { kind: "none" },
    });
    setProviders(await listProviders());
    setModal(null);
    setStatus("Provider created");
  }

  async function runProviderTest(providerId: string) {
    const updated = await testProvider(providerId);
    setProviders((items) => items.map((item) => (item.id === providerId ? updated : item)));
    setStatus(`Provider ${providerId} health: ${updated.health}`);
  }

  async function removeProvider(providerId: string) {
    const confirmed = window.confirm(`Delete provider "${providerId}"? Profiles using it will no longer resolve this provider.`);
    if (!confirmed) return;
    try {
      await deleteProvider(providerId);
      setProviders(await listProviders());
      await refresh();
      setStatus(`Provider ${providerId} deleted`);
    } catch (err) {
      setError(formatError(err));
    }
  }

  async function openAttachProviderModal(providerId: string) {
    setAttachReq({
      profileId: selectedAccount?.id ?? accounts[0]?.id ?? "",
      providerId,
      model: providers.find((provider) => provider.id === providerId)?.defaultModel ?? null,
    });
    setPlan(null);
    setModal("attachProvider");
  }

  async function previewAttachProvider() {
    setPlan(await planAttachProviderToProfile(attachReq));
  }

  async function attachProvider() {
    await attachProviderToProfile(attachReq);
    setModal(null);
    setPlan(null);
    await refresh();
    setStatus(`Attached ${attachReq.providerId} to ${attachReq.profileId}`);
  }

  const footerStatus = useMemo(() => {
    if (!status) return inTauri() ? "Ready" : "Preview";
    if (status.startsWith("Refreshed")) {
      const stamp = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date());
      return `${status} • Today at ${stamp}`;
    }
    return status;
  }, [status]);

  return (
    <main className="app-shell">
      <header className="titlebar">
        <div className="titlebarLead">
          <span className="titlebarLogo" aria-hidden>
            <IconLogo size={22} />
          </span>
          <div>
            <p className="titlebarBrand">LAM</p>
            <h2>{routeTitleFromModule(route)}</h2>
          </div>
        </div>
        <div className="titlebarActions">
          <div className="toolbar">
            <UIButton size="sm" className="toolbarBtn" onClick={refresh}>
              <IconRefresh size={14} />
              Refresh
            </UIButton>
            <UIButton size="sm" onClick={() => setModal("account")}>+ New Account</UIButton>
            <UIButton size="sm" onClick={() => setModal("provider")}>+ New Provider</UIButton>
            <UIButton variant="primary" size="sm" onClick={openRelayModal}>+ New Relay</UIButton>
          </div>
          <ThemeToggle value={themeMode} onChange={setThemeMode} />
        </div>
      </header>

      <section className="content">
        {error ? <div className="notice danger">{error}</div> : null}
        {!appReady ? (
          <div className="bootState" role="status" aria-live="polite">
            <span className="bootSpinner" aria-hidden />
            <span>Loading accounts…</span>
          </div>
        ) : null}
        {appReady && route === "overview" ? (
          <Views.Overview
            accounts={accounts}
            quotas={quotas}
            providers={providers}
            select={setSelectedAccountId}
            openSync={openSyncModal}
            login={login}
            relayResume={relayResumeTo}
            currentSession={activeSession}
            refreshAccountQuota={refreshAccountQuota}
            refreshingQuotaIds={refreshingQuotaIds}
          />
        ) : null}
        {appReady && route === "sessions" ? (
          <Views.Sessions
            sessions={filteredSessions}
            accounts={accounts}
            selectedAccountId={selectedAccountId}
            setSelectedAccountId={setSelectedAccountId}
            query={query}
            setQuery={setQuery}
            copy={copyResume}
            open={openResume}
            details={openSessionDetails}
          />
        ) : null}
        {appReady && route === "relay" ? <Views.Relay accounts={accounts} openRelay={openRelayModal} openSync={openSyncModal} /> : null}
        {appReady && route === "providers" ? <Views.Providers accounts={accounts} providers={providers} create={() => setModal("provider")} test={runProviderTest} remove={removeProvider} attach={openAttachProviderModal} /> : null}
        {appReady && route === "sync" ? <Views.SyncHome accounts={accounts} openSync={openSyncModal} /> : null}
        {appReady && route === "settings" ? (
          <Views.Settings
            health={health}
            themeMode={themeMode}
            resolvedTheme={resolvedTheme}
            divergedStrategy={divergedStrategy}
            setDivergedStrategy={setDivergedStrategy}
          />
        ) : null}
      </section>

      <footer className="bottomDock">
        <div className="bottomDockIsland">
          <div className="statusbar">
            <IconClock size={14} className="statusbarClock" />
            <span className="statusbarMain">{footerStatus}</span>
            {selectedSession?.providerMismatch ? <span className="statusHint">Provider mismatch</span> : null}
            <span className="bottomDockMeta">
              <span className={`statusDot ${health?.ok ? "ok" : ""}`} aria-hidden />
              {health?.ok ? "Operational" : "Checking…"}
              <span className="bottomDockVersion">{health?.version ? `v${health.version}` : "v1.0.0"}</span>
            </span>
          </div>
          <Shell.BottomNav route={route} setRoute={setRoute} />
        </div>
      </footer>

      {modal === "account" ? (
        <Shell.Modal title="Add Managed Account" close={() => setModal(null)}>
          <div className="formGrid">
            <label>Account name<input value={accountReq.name} onChange={(e) => setAccountReq({ ...accountReq, name: e.target.value })} /></label>
            <label>Copy config from<select value={accountReq.copyConfigFrom ?? ""} onChange={(e) => setAccountReq({ ...accountReq, copyConfigFrom: e.target.value || null })}>
              <option value="">None</option>
              {accounts.filter((account) => account.hasConfig).map((account) => <option key={account.id} value={account.id}>{account.id}</option>)}
            </select></label>
          </div>
          <div className="previewBox">
            <div className="previewLine"><span>CODEX_HOME</span><strong>~/.codex-{accountReq.name || "name"}</strong></div>
            <div className="previewLine"><span>Wrapper</span><strong>~/bin/codex-{accountReq.name || "name"}</strong></div>
          </div>
          <label className="syncOption"><input type="checkbox" checked={accountReq.overwriteWrapper} onChange={(e) => setAccountReq({ ...accountReq, overwriteWrapper: e.target.checked })} /><span><strong>Overwrite wrapper if it exists</strong><span>Keeps CODEX_HOME untouched; only wrapper script is replaced.</span></span></label>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={() => setModal(null)}>Cancel</UIButton>
            <div className="modalFootPrimary"><UIButton type="button" onClick={previewAccount}>Dry Run</UIButton><UIButton type="button" variant="primary" disabled={!plan} onClick={createAccount}>Create</UIButton></div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "relay" ? (
        <Shell.Modal title="Create Relay Workspace" close={() => setModal(null)}>
          <div className="formGrid">
            <label>Runtime account<select value={relayReq.runtimeProfileId} onChange={(e) => setRelayReq({ ...relayReq, runtimeProfileId: e.target.value })}>{accounts.map((account) => <option key={account.id} value={account.id}>{account.id}</option>)}</select></label>
            <label>Source account<select value={relayReq.sourceProfileId} onChange={(e) => setRelayReq({ ...relayReq, sourceProfileId: e.target.value })}>{accounts.map((account) => <option key={account.id} value={account.id}>{account.id}</option>)}</select></label>
            <label>Relay name<input value={relayReq.name ?? ""} onChange={(e) => setRelayReq({ ...relayReq, name: e.target.value || null })} /></label>
            <label>Provider policy<select value={relayReq.providerPolicy} onChange={(e) => setRelayReq({ ...relayReq, providerPolicy: e.target.value })}><option value="inherit_runtime">Use runtime account provider</option><option value="inherit_source">Use source provider</option></select></label>
          </div>
          <div className="previewBox">
            <div className="previewLine"><span>CODEX_HOME</span><strong>~/.codex-{relayReq.name || `${relayReq.runtimeProfileId || "runtime"}-relay-${relayReq.sourceProfileId || "source"}`}</strong></div>
            <div className="previewLine"><span>Provider</span><strong>{relayReq.providerPolicy === "inherit_source" ? "source account" : "runtime account"}</strong></div>
          </div>
          <div className="notice">Relay creation never copies auth.json and does not modify the runtime profile.</div>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={() => setModal(null)}>Cancel</UIButton>
            <div className="modalFootPrimary"><UIButton type="button" onClick={previewRelay}>Dry Run</UIButton><UIButton type="button" variant="primary" disabled={!plan} onClick={createRelay}>Create Relay</UIButton></div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "sync" && syncReq ? (
        <Shell.Modal title="Sync Sessions Safely" wide close={() => setModal(null)}>
          <SyncModal
            accounts={accounts}
            syncReq={syncReq}
            setSyncReq={(req) => {
              setSyncReq(req);
              setPlan(null);
              setSyncResult(null);
            }}
            plan={plan}
            syncResult={syncResult}
            onDryRun={previewSync}
            onExecute={runSync}
            onClose={() => setModal(null)}
          />
        </Shell.Modal>
      ) : null}

      {modal === "provider" ? (
        <Shell.Modal title="Add External Provider" close={() => setModal(null)}>
          <div className="formGrid">
            <label>Provider id<input value={providerReq.id} onChange={(e) => setProviderReq({ ...providerReq, id: e.target.value })} /></label>
            <label>Name<input value={providerReq.name} onChange={(e) => setProviderReq({ ...providerReq, name: e.target.value })} /></label>
            <label>Base URL<input value={providerReq.baseUrl} onChange={(e) => setProviderReq({ ...providerReq, baseUrl: e.target.value })} /></label>
            <label>Wire API<input value={providerReq.wireApi} onChange={(e) => setProviderReq({ ...providerReq, wireApi: e.target.value })} /></label>
            <label>Default model<input value={providerReq.defaultModel} onChange={(e) => setProviderReq({ ...providerReq, defaultModel: e.target.value })} /></label>
            <label>Env key<input value={providerReq.envKey ?? ""} onChange={(e) => setProviderReq({ ...providerReq, envKey: e.target.value || null })} /></label>
          </div>
          <div className="previewBox">
            <div className="previewLine"><span>Provider ID</span><strong>{providerReq.id || "provider-id"}</strong></div>
            <div className="previewLine"><span>Secret storage</span><strong>{providerReq.envKey ? `ENV (${providerReq.envKey})` : "None"}</strong></div>
          </div>
          <div className="notice">Secrets are not shown or stored in config.toml. Use an environment variable or Keychain-backed secret storage.</div>
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={() => setModal(null)}>Cancel</UIButton>
            <div className="modalFootPrimary"><UIButton type="button" variant="primary" onClick={createProviderFromModal}>Create Provider</UIButton></div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "attachProvider" ? (
        <Shell.Modal title="Attach Provider to Account" close={() => setModal(null)}>
          <div className="formGrid">
            <label>Account<select value={attachReq.profileId} onChange={(e) => setAttachReq({ ...attachReq, profileId: e.target.value })}>{accounts.map((account) => <option key={account.id} value={account.id}>{account.id}</option>)}</select></label>
            <label>Provider<select value={attachReq.providerId} onChange={(e) => setAttachReq({ ...attachReq, providerId: e.target.value })}>{providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.id}</option>)}</select></label>
            <label>Model<input value={attachReq.model ?? ""} onChange={(e) => setAttachReq({ ...attachReq, model: e.target.value || null })} /></label>
          </div>
          <div className="previewBox">
            <div className="previewLine"><span>Target profile</span><strong>{attachReq.profileId || "profile"}</strong></div>
            <div className="previewLine"><span>Provider</span><strong>{attachReq.providerId || "provider"}</strong></div>
          </div>
          <div className="notice">Attach backs up config.toml and writes provider references only. API keys are never written.</div>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={() => setModal(null)}>Cancel</UIButton>
            <div className="modalFootPrimary"><UIButton type="button" onClick={previewAttachProvider}>Dry Run</UIButton><UIButton type="button" variant="primary" disabled={!plan} onClick={attachProvider}>Attach</UIButton></div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "sessionDetail" && selectedSession ? (
        <Shell.Modal title="Session Details" wide close={() => setModal(null)}>
          <div className="detailTitleRow">
            <h3 className="detailTitle">{selectedSession.id}</h3>
            {selectedSession.providerMismatch ? <span className="badge warn">mismatch</span> : null}
          </div>
          {selectedSession.providerMismatch ? (
            <p className="notice warn">Resume can continue, but runtime behavior, cost, and tool compatibility may differ.</p>
          ) : null}

          <div className="detailGrid">
            <div className="detailItem">
              <span className="detailLabel">Account</span>
              <strong>{selectedSessionAccount?.codexHome ?? selectedSession.accountId}</strong>
            </div>
            <div className="detailItem">
              <span className="detailLabel">cwd</span>
              <strong>{selectedSession.cwd ?? "unknown"}</strong>
            </div>
            <div className="detailItem">
              <span className="detailLabel">Session file</span>
              <strong className="mono">{selectedSession.path}</strong>
            </div>
            {selectedSession.summary ? (
              <div className="detailItem detailItem--full">
                <span className="detailLabel">Summary</span>
                <div className="detailSummary">{selectedSession.summary}</div>
              </div>
            ) : null}
            <div className="detailItem">
              <span className="detailLabel">Original provider</span>
              <strong>
                {selectedSession.originalProviderId ?? "unknown"} · {selectedSession.originalModel ?? "unknown"}
              </strong>
            </div>
            <div className="detailItem">
              <span className="detailLabel">Runtime provider</span>
              <strong>
                {selectedSession.currentProviderId ?? "unknown"} · {selectedSession.currentModel ?? "unknown"}
              </strong>
            </div>
          </div>

          {resume ? (
            <div className="notice safe">
              <strong>Resume command preview</strong>
              <div className="sideEffectsSubtle">Only executed via Copy/Open actions.</div>
              <div className="code">{resume.command}</div>
            </div>
          ) : (
            <div className="notice">Select a session to preview resume command.</div>
          )}

          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={() => setModal(null)}>
              Close
            </UIButton>
            <div className="modalFootPrimary">
              <UIButton type="button" onClick={() => copyResume(selectedSession)}>
                Copy
              </UIButton>
              <UIButton type="button" variant="primary" onClick={() => openResume(selectedSession)}>
                Open Terminal
              </UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}
    </main>
  );
}

function formatError(err: unknown) {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) return String((err as { message: unknown }).message);
  return String(err);
}
