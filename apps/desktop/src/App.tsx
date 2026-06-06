import { useEffect, useMemo, useState } from "react";
import { useAppStore } from "./stores/app";
import { useAccountStore } from "./stores/accounts";
import { useSessionStore } from "./stores/sessions";
import { useQuotaStore } from "./stores/quota";
import { useProviderStore } from "./stores/providers";
import * as api from "./lib/api";
import * as Shell from "./components/shell";
import { IconClock, IconLogo, IconRefresh } from "./components/icons";
import { SyncModal } from "./components/sync-modal";
import { ThemeToggle } from "./components/theme-toggle";
import { UIButton } from "./components/ui-button";
import { routeTitle as routeTitleFromModule } from "./routes/types";
import type {
  AttachProviderRequest,
  CreateAccountRequest,
  CreateProviderRequest,
  CreateRelayRequest,
  OperationPlan,
  SyncPlan,
  SyncRequest,
  SyncResult,
} from "./lib/types";
import * as Views from "./routes/views";

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

export function App() {
  const { route, setRoute, themeMode, setThemeMode, health, status, error, appReady, modal, openModal, closeModal } =
    useAppStore();
  const { accounts, selectedAccountId, setSelectedAccountId, activeSession, divergedStrategy, setDivergedStrategy, refresh, relayResumeTo, login } =
    useAccountStore();
  const { query, setQuery, resume, copyResume, openResume, openSessionDetails, filteredSessions } =
    useSessionStore();
  const { quotas, refreshingQuotaIds, refreshAccountQuota, startAutoRefresh, stopAutoRefresh } =
    useQuotaStore();
  const { providers, testProvider: runProviderTest, removeProvider, createFromModal, attachToProfile } =
    useProviderStore();

  const selectedAccount = useAccountStore((s) => s.selectedAccount());
  const selectedSession = useSessionStore((s) => s.selectedSession());
  const filtered = filteredSessions();

  const selectedSessionAccount = selectedSession ? accounts.find((a) => a.id === selectedSession.accountId) : undefined;

  // Modal form state (local — only needed while modal is open)
  const [accountReq, setAccountReq] = useState(emptyAccountReq);
  const [relayReq, setRelayReq] = useState<CreateRelayRequest>({
    runtimeProfileId: "", sourceProfileId: "", name: null, providerPolicy: "inherit_runtime", overwriteWrapper: false,
  });
  const [plan, setPlan] = useState<OperationPlan | SyncPlan | null>(null);
  const [syncReq, setSyncReq] = useState<SyncRequest | null>(null);
  const [syncResult, setSyncResult] = useState<SyncResult | null>(null);
  const [providerReq, setProviderReq] = useState(emptyProviderReq);
  const [attachReq, setAttachReq] = useState<AttachProviderRequest>({ profileId: "", providerId: "", model: null });

  const resolvedTheme = useMemo(() => {
    if (themeMode === "system") return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    return themeMode;
  }, [themeMode]);

  // Boot — run once on mount, refresh is a stable store action
  useEffect(() => { refresh(); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Theme sync
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = themeMode === "system" ? (media.matches ? "dark" : "light") : themeMode;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [themeMode]);

  // Reload sessions on account switch
  useEffect(() => {
    if (selectedAccountId) useSessionStore.getState().loadSessions(selectedAccountId);
  }, [selectedAccountId]);

  // Auto-refresh quotas — start/stop are stable store actions
  useEffect(() => {
    if (accounts.length) startAutoRefresh(accounts.map((a) => a.id));
    return stopAutoRefresh;
  }, [accounts]); // eslint-disable-line react-hooks/exhaustive-deps

  const footerStatus = useMemo(() => {
    if (!status) return api.inTauri() ? "Ready" : "Preview";
    if (status.startsWith("Refreshed")) {
      const stamp = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date());
      return `${status} • Today at ${stamp}`;
    }
    return status;
  }, [status]);

  // Modal helpers
  async function openRelayModal() {
    const runtime = selectedAccount?.id ?? accounts[0]?.id ?? "";
    const source = accounts.find((a) => a.id !== runtime)?.id ?? accounts[0]?.id ?? "";
    setRelayReq({
      runtimeProfileId: runtime, sourceProfileId: source,
      name: runtime && source ? `${runtime}-relay-${source}` : null,
      providerPolicy: "inherit_runtime", overwriteWrapper: false,
    });
    setPlan(null);
    openModal("relay");
  }
  async function openSyncModal(from = selectedAccount?.id) {
    const target = accounts.find((a) => a.isRelay)?.id ?? accounts.find((a) => a.id !== from)?.id ?? "";
    setSyncReq({ fromProfileId: from ?? "", toProfileId: target, syncSessions: true, backupTargetSessions: true, sidecarBackupHistory: false });
    setPlan(null);
    setSyncResult(null);
    openModal("sync");
  }
  function openAttachProviderModal(providerId: string) {
    setAttachReq({
      profileId: selectedAccount?.id ?? accounts[0]?.id ?? "", providerId,
      model: providers.find((p) => p.id === providerId)?.defaultModel ?? null,
    });
    setPlan(null);
    openModal("attachProvider");
  }

  return (
    <main className="app-shell">
      <header className="titlebar">
        <div className="titlebarLead">
          <span className="titlebarLogo" aria-hidden><IconLogo size={22} /></span>
          <div>
            <p className="titlebarBrand">LAM</p>
            <h2>{routeTitleFromModule(route)}</h2>
          </div>
        </div>
        <div className="titlebarActions">
          <div className="toolbar">
            <UIButton size="sm" className="toolbarBtn" onClick={refresh}><IconRefresh size={14} /> Refresh</UIButton>
            <UIButton size="sm" onClick={() => openModal("account")}>+ New Account</UIButton>
            <UIButton size="sm" onClick={() => openModal("provider")}>+ New Provider</UIButton>
            <UIButton variant="primary" size="sm" onClick={openRelayModal}>+ New Relay</UIButton>
          </div>
          <ThemeToggle value={themeMode} onChange={setThemeMode} />
        </div>
      </header>

      <section className="content">
        {error ? <div className="notice danger">{error}</div> : null}
        {!appReady ? (
          <div className="bootState" role="status" aria-live="polite">
            <span className="bootSpinner" aria-hidden /><span>Loading accounts…</span>
          </div>
        ) : null}
        {appReady && route === "overview" ? (
          <Views.Overview accounts={accounts} quotas={quotas} providers={providers} select={setSelectedAccountId}
            openSync={openSyncModal} login={login} relayResume={relayResumeTo} currentSession={activeSession}
            refreshAccountQuota={refreshAccountQuota} refreshingQuotaIds={refreshingQuotaIds} />
        ) : null}
        {appReady && route === "sessions" ? (
          <Views.Sessions sessions={filtered} accounts={accounts} selectedAccountId={selectedAccountId}
            setSelectedAccountId={setSelectedAccountId} query={query} setQuery={setQuery}
            copy={copyResume} open={openResume} details={openSessionDetails} />
        ) : null}
        {appReady && route === "relay" ? <Views.Relay accounts={accounts} openRelay={openRelayModal} openSync={openSyncModal} /> : null}
        {appReady && route === "providers" ? <Views.Providers accounts={accounts} providers={providers} create={() => openModal("provider")} test={runProviderTest} remove={removeProvider} attach={openAttachProviderModal} /> : null}
        {appReady && route === "sync" ? <Views.SyncHome accounts={accounts} openSync={openSyncModal} /> : null}
        {appReady && route === "settings" ? (
          <Views.Settings health={health} themeMode={themeMode} resolvedTheme={resolvedTheme}
            divergedStrategy={divergedStrategy} setDivergedStrategy={setDivergedStrategy} />
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
        <Shell.Modal title="Add Managed Account" close={closeModal}>
          <div className="formGrid">
            <label>Account name<input value={accountReq.name} onChange={(e) => setAccountReq({ ...accountReq, name: e.target.value })} /></label>
            <label>Copy config from<select value={accountReq.copyConfigFrom ?? ""} onChange={(e) => setAccountReq({ ...accountReq, copyConfigFrom: e.target.value || null })}>
              <option value="">None</option>
              {accounts.filter((a) => a.hasConfig).map((a) => <option key={a.id} value={a.id}>{a.id}</option>)}
            </select></label>
          </div>
          <div className="previewBox">
            <div className="previewLine"><span>CODEX_HOME</span><strong>~/.codex-{accountReq.name || "name"}</strong></div>
            <div className="previewLine"><span>Wrapper</span><strong>~/bin/codex-{accountReq.name || "name"}</strong></div>
          </div>
          <label className="syncOption"><input type="checkbox" checked={accountReq.overwriteWrapper} onChange={(e) => setAccountReq({ ...accountReq, overwriteWrapper: e.target.checked })} /><span><strong>Overwrite wrapper if it exists</strong><span>Keeps CODEX_HOME untouched; only wrapper script is replaced.</span></span></label>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>Cancel</UIButton>
            <div className="modalFootPrimary">
              <UIButton type="button" onClick={async () => setPlan(await api.planCreateAccount(accountReq))}>Dry Run</UIButton>
              <UIButton type="button" variant="primary" disabled={!plan} onClick={async () => { await api.executeCreateAccount(accountReq); closeModal(); setPlan(null); await refresh(); }}>Create</UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "relay" ? (
        <Shell.Modal title="Create Relay Workspace" close={closeModal}>
          <div className="formGrid">
            <label>Runtime account<select value={relayReq.runtimeProfileId} onChange={(e) => setRelayReq({ ...relayReq, runtimeProfileId: e.target.value })}>{accounts.map((a) => <option key={a.id} value={a.id}>{a.id}</option>)}</select></label>
            <label>Source account<select value={relayReq.sourceProfileId} onChange={(e) => setRelayReq({ ...relayReq, sourceProfileId: e.target.value })}>{accounts.map((a) => <option key={a.id} value={a.id}>{a.id}</option>)}</select></label>
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
            <UIButton type="button" variant="ghost" onClick={closeModal}>Cancel</UIButton>
            <div className="modalFootPrimary">
              <UIButton type="button" onClick={async () => setPlan(await api.planCreateRelay(relayReq))}>Dry Run</UIButton>
              <UIButton type="button" variant="primary" disabled={!plan} onClick={async () => { await api.executeCreateRelay(relayReq); closeModal(); setPlan(null); await refresh(); }}>Create Relay</UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "sync" && syncReq ? (
        <Shell.Modal title="Sync Sessions Safely" wide close={closeModal}>
          <SyncModal accounts={accounts} syncReq={syncReq}
            setSyncReq={(req) => { setSyncReq(req); setPlan(null); setSyncResult(null); }}
            plan={plan} syncResult={syncResult}
            onDryRun={async () => { if (syncReq) setPlan(await api.buildSyncPlan(syncReq)); }}
            onExecute={async () => { if (syncReq && plan) { setSyncResult(await api.executeSync(syncReq)); await refresh(); } }}
            onClose={closeModal} />
        </Shell.Modal>
      ) : null}

      {modal === "provider" ? (
        <Shell.Modal title="Add External Provider" close={closeModal}>
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
            <UIButton type="button" variant="ghost" onClick={closeModal}>Cancel</UIButton>
            <div className="modalFootPrimary"><UIButton type="button" variant="primary" onClick={() => createFromModal(providerReq)}>Create Provider</UIButton></div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "attachProvider" ? (
        <Shell.Modal title="Attach Provider to Account" close={closeModal}>
          <div className="formGrid">
            <label>Account<select value={attachReq.profileId} onChange={(e) => setAttachReq({ ...attachReq, profileId: e.target.value })}>{accounts.map((a) => <option key={a.id} value={a.id}>{a.id}</option>)}</select></label>
            <label>Provider<select value={attachReq.providerId} onChange={(e) => setAttachReq({ ...attachReq, providerId: e.target.value })}>{providers.map((p) => <option key={p.id} value={p.id}>{p.id}</option>)}</select></label>
            <label>Model<input value={attachReq.model ?? ""} onChange={(e) => setAttachReq({ ...attachReq, model: e.target.value || null })} /></label>
          </div>
          <div className="previewBox">
            <div className="previewLine"><span>Target profile</span><strong>{attachReq.profileId || "profile"}</strong></div>
            <div className="previewLine"><span>Provider</span><strong>{attachReq.providerId || "provider"}</strong></div>
          </div>
          <div className="notice">Attach backs up config.toml and writes provider references only. API keys are never written.</div>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>Cancel</UIButton>
            <div className="modalFootPrimary">
              <UIButton type="button" onClick={async () => setPlan(await api.planAttachProviderToProfile(attachReq))}>Dry Run</UIButton>
              <UIButton type="button" variant="primary" disabled={!plan} onClick={() => attachToProfile(attachReq)}>Attach</UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === "sessionDetail" && selectedSession ? (
        <Shell.Modal title="Session Details" wide close={closeModal}>
          <div className="detailTitleRow">
            <h3 className="detailTitle">{selectedSession.id}</h3>
            {selectedSession.providerMismatch ? <span className="badge warn">mismatch</span> : null}
          </div>
          {selectedSession.providerMismatch ? (
            <p className="notice warn">Resume can continue, but runtime behavior, cost, and tool compatibility may differ.</p>
          ) : null}
          <div className="detailGrid">
            <div className="detailItem"><span className="detailLabel">Account</span><strong>{selectedSessionAccount?.codexHome ?? selectedSession.accountId}</strong></div>
            <div className="detailItem"><span className="detailLabel">cwd</span><strong>{selectedSession.cwd ?? "unknown"}</strong></div>
            <div className="detailItem"><span className="detailLabel">Session file</span><strong className="mono">{selectedSession.path}</strong></div>
            {selectedSession.summary ? (
              <div className="detailItem detailItem--full"><span className="detailLabel">Summary</span><div className="detailSummary">{selectedSession.summary}</div></div>
            ) : null}
            <div className="detailItem"><span className="detailLabel">Original provider</span><strong>{selectedSession.originalProviderId ?? "unknown"} · {selectedSession.originalModel ?? "unknown"}</strong></div>
            <div className="detailItem"><span className="detailLabel">Runtime provider</span><strong>{selectedSession.currentProviderId ?? "unknown"} · {selectedSession.currentModel ?? "unknown"}</strong></div>
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
            <UIButton type="button" variant="ghost" onClick={closeModal}>Close</UIButton>
            <div className="modalFootPrimary">
              <UIButton type="button" onClick={() => copyResume(selectedSession)}>Copy</UIButton>
              <UIButton type="button" variant="primary" onClick={() => openResume(selectedSession)}>Open Terminal</UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}
    </main>
  );
}
