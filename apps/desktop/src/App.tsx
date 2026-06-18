import { useCallback, useEffect, useMemo, useState } from 'react';
import { useAppStore } from './stores/app';
import { useAccountStore } from './stores/accounts';
import { useSessionStore } from './stores/sessions';
import { useQuotaStore } from './stores/quota';
import { useProviderStore } from './stores/providers';
import * as api from './lib/api';
import * as Shell from './components/shell';
import { IconClock, IconLogo, IconRefresh, IconPlus } from './components/icons';
import { SyncModal } from './components/sync-modal';
import { ThemeToggle } from './components/theme-toggle';
import { UIButton } from './components/ui-button';
import { routeTitle as routeTitleFromModule } from './routes/types';
import type {
  AttachProviderRequest,
  CreateAccountRequest,
  CreateProviderRequest,
  CodexSession,
  OperationPlan,
  RenameAccountRequest,
  SyncPlan,
  SyncRequest,
  SyncResult,
  AntigravityQuotaResponse,
} from './lib/types';
import * as Views from './routes/views';

const emptyAccountReq: CreateAccountRequest = {
  name: 'luna',
  copyConfigFrom: null,
  overwriteWrapper: false,
};
const emptyRenameReq: RenameAccountRequest = {
  fromProfileId: '',
  toName: '',
  overwriteWrapper: false,
};
const emptyProviderReq: CreateProviderRequest = {
  id: 'company-proxy',
  name: 'Company Proxy',
  baseUrl: 'https://proxy.example.test/v1',
  wireApi: 'openai',
  defaultModel: 'gpt-5-codex',
  envKey: 'COMPANY_PROXY_API_KEY',
  secret: { kind: 'env', envKey: 'COMPANY_PROXY_API_KEY' },
};

export function App() {
  const {
    route,
    setRoute,
    themeMode,
    setThemeMode,
    health,
    status,
    error,
    appReady,
    modal,
    openModal,
    closeModal,
  } = useAppStore();
  const {
    accounts,
    selectedAccountId,
    setSelectedAccountId,
    activeSession,
    divergedStrategy,
    setDivergedStrategy,
    refreshing: refreshingAccounts,
    refresh,
    relayResumeTo,
    relaySessionTo,
    login,
  } = useAccountStore();
  const { query, setQuery, resume, copyResume, openResume, openSessionDetails, filteredSessions } =
    useSessionStore();
  const { quotas, refreshingQuotaIds, refreshAccountQuota, startAutoRefresh, stopAutoRefresh } =
    useQuotaStore();
  const {
    providers,
    testProvider: runProviderTest,
    removeProvider,
    createFromModal,
    attachToProfile,
  } = useProviderStore();

  const selectedAccount = useAccountStore((s) => s.selectedAccount());
  const selectedSession = useSessionStore((s) => s.selectedSession());
  const filtered = filteredSessions();

  const selectedSessionAccount = selectedSession
    ? accounts.find((a) => a.id === selectedSession.accountId)
    : undefined;

  // Modal form state (local — only needed while modal is open)
  const [accountReq, setAccountReq] = useState(emptyAccountReq);
  const [renameReq, setRenameReq] = useState(emptyRenameReq);
  const [plan, setPlan] = useState<OperationPlan | SyncPlan | null>(null);
  const [syncReq, setSyncReq] = useState<SyncRequest | null>(null);
  const [syncResult, setSyncResult] = useState<SyncResult | null>(null);
  const [providerReq, setProviderReq] = useState(emptyProviderReq);
  const [attachReq, setAttachReq] = useState<AttachProviderRequest>({
    profileId: '',
    providerId: '',
    model: null,
  });
  const [handoffSourceId, setHandoffSourceId] = useState('');
  const [handoffTargetId, setHandoffTargetId] = useState('');
  const [handoffSessionId, setHandoffSessionId] = useState('');
  const [handoffSessions, setHandoffSessions] = useState<CodexSession[]>([]);
  const [handoffLoading, setHandoffLoading] = useState(false);
  const [antigravityQuota, setAntigravityQuota] = useState<AntigravityQuotaResponse | null>(null);
  const [refreshingAntigravity, setRefreshingAntigravity] = useState(false);

  const resolvedTheme = useMemo(() => {
    if (themeMode === 'system')
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    return themeMode;
  }, [themeMode]);

  // Boot — run once on mount, refresh is a stable store action
  useEffect(() => {
    refresh();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Theme sync
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () => {
      document.documentElement.dataset.theme =
        themeMode === 'system' ? (media.matches ? 'dark' : 'light') : themeMode;
    };
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
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

  const loadAntigravity = useCallback(async (forceRefresh = false) => {
    if (!api.inTauri()) return;
    if (forceRefresh) {
      setRefreshingAntigravity(true);
    }
    try {
      const res = await api.getAntigravityQuota();
      setAntigravityQuota(res);
    } catch (err) {
      console.error('Failed to load Antigravity quota:', err);
    } finally {
      setRefreshingAntigravity(false);
    }
  }, []);

  const handleRefreshAll = useCallback(async () => {
    await Promise.all([
      refresh({ refreshQuotasNow: true }),
      loadAntigravity(true)
    ]);
  }, [refresh, loadAntigravity]);

  // Antigravity auto-refresh
  /* eslint-disable react-hooks/set-state-in-effect -- async data fetch on mount + interval */
  useEffect(() => {
    void loadAntigravity(false);
    const timer = window.setInterval(() => {
      void loadAntigravity(true);
    }, 2 * 60_000);
    return () => window.clearInterval(timer);
  }, [loadAntigravity]);
  /* eslint-enable react-hooks/set-state-in-effect */

  const footerStatus = useMemo(() => {
    if (!status) return api.inTauri() ? 'Ready' : 'Preview';
    if (status.startsWith('Refreshed')) {
      const stamp = new Intl.DateTimeFormat(undefined, {
        hour: 'numeric',
        minute: '2-digit',
      }).format(new Date());
      return `${status} • Today at ${stamp}`;
    }
    return status;
  }, [status]);

  useEffect(() => {
    if (modal !== 'handoff' || !handoffSourceId) return;
    let active = true;
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setHandoffLoading(true);
    api
      .listSessions(handoffSourceId)
      .then((items) => {
        if (!active) return;
        setHandoffSessions(items);
        setHandoffSessionId((current) =>
          items.some((item) => item.id === current) ? current : (items[0]?.id ?? ''),
        );
      })
      .catch((err) => {
        if (!active) return;
        useAppStore.getState().setError(String(err));
      })
      .finally(() => {
        if (!active) return;
        setHandoffLoading(false);
      });

    return () => {
      active = false;
    };
  }, [modal, handoffSourceId]);

  function changeHandoffSource(newSourceId: string) {
    setHandoffSourceId(newSourceId);
    setHandoffSessionId('');
    setHandoffSessions([]);
    setHandoffLoading(true);
    if (newSourceId === handoffTargetId) {
      const other = accounts.find((a) => a.id !== newSourceId)?.id ?? '';
      setHandoffTargetId(other);
    }
  }

  // Modal helpers
  function openHandoffModal(opts?: { session?: CodexSession; targetAccountId?: string }) {
    const requestedTarget = opts?.targetAccountId;
    let source = opts?.session?.accountId ?? selectedAccount?.id ?? accounts[0]?.id ?? '';
    if (requestedTarget && requestedTarget === source) {
      source = accounts.find((a) => a.id !== requestedTarget)?.id ?? source;
    }
    const target =
      requestedTarget ?? accounts.find((a) => a.id !== source)?.id ?? accounts[0]?.id ?? '';
    setHandoffSourceId(source);
    setHandoffTargetId(target);
    setHandoffSessionId(opts?.session?.id ?? '');
    setHandoffSessions(opts?.session ? [opts.session] : []);
    setHandoffLoading(true);
    openModal('handoff');
  }

  async function openSyncModal(from = selectedAccount?.id) {
    const target = accounts.find((a) => a.id !== from)?.id ?? '';
    setSyncReq({
      fromProfileId: from ?? '',
      toProfileId: target,
      syncSessions: true,
      backupTargetSessions: true,
      sidecarBackupHistory: false,
    });
    setPlan(null);
    setSyncResult(null);
    openModal('sync');
  }
  function openAttachProviderModal(providerId: string) {
    setAttachReq({
      profileId: selectedAccount?.id ?? accounts[0]?.id ?? '',
      providerId,
      model: providers.find((p) => p.id === providerId)?.defaultModel ?? null,
    });
    setPlan(null);
    openModal('attachProvider');
  }

  function openRenameAccountModal(account: (typeof accounts)[number]) {
    setRenameReq({
      fromProfileId: account.id,
      toName: account.id,
      overwriteWrapper: false,
    });
    setPlan(null);
    openModal('renameAccount');
  }

  async function renameAccount() {
    if (!plan) return;
    const result = await api.executeRenameAccount(renameReq);
    closeModal();
    setPlan(null);
    useAppStore.getState().setStatus(`Renamed ${result.previousProfileId} to ${result.profileId}`);
    await refresh();
    setSelectedAccountId(result.profileId);
    await useSessionStore.getState().loadSessions(result.profileId);
  }

  if (!appReady) {
    return (
      <div className="splash-container">
        <svg xmlns="http://www.w3.org/2000/svg" className="splash-logo" viewBox="0 0 1024 1024">
          <defs>
            <linearGradient id="surface" x1="150" y1="80" x2="870" y2="930" gradientUnits="userSpaceOnUse">
              <stop offset="0" stopColor="#ffffff"/>
              <stop offset="1" stopColor="#f6f8ff"/>
            </linearGradient>
            <radialGradient id="core" cx="50%" cy="47%" r="34%">
              <stop offset="0" stopColor="#ffffff"/>
              <stop offset="0.42" stopColor="#efe7ff"/>
              <stop offset="0.72" stopColor="#dff7ff"/>
              <stop offset="1" stopColor="#ffffff"/>
            </radialGradient>
          </defs>
          <g transform="translate(102.4 102.4) scale(0.8)">
            <rect x="0" y="0" width="1024" height="1024" rx="176" fill="url(#surface)" stroke="#dfe5f2" strokeWidth="10"/>
            <g fill="none" strokeLinecap="round">
              <ellipse cx="512" cy="512" rx="360" ry="238" stroke="#8ec8ff" strokeWidth="5" opacity="0.56"/>
              <ellipse cx="512" cy="512" rx="324" ry="286" stroke="#c9a8ff" strokeWidth="5" opacity="0.46" transform="rotate(-24 512 512)"/>
              <ellipse cx="512" cy="512" rx="298" ry="210" stroke="#8ee9e0" strokeWidth="4" opacity="0.45" transform="rotate(58 512 512)"/>
            </g>
            <circle cx="512" cy="512" r="118" fill="url(#core)" stroke="#e8e6ff" strokeWidth="10"/>
            <text x="512" y="608" textAnchor="middle" fontFamily="Georgia, 'Times New Roman', serif" fontSize="360" fontWeight="700" fill="#111c55">Lam</text>
            <g fill="#ffffff" stroke="#e6eaff" strokeWidth="4">
              <circle cx="512" cy="174" r="60"/>
              <circle cx="204" cy="386" r="58"/>
              <circle cx="826" cy="386" r="58"/>
              <circle cx="300" cy="752" r="56"/>
              <circle cx="730" cy="752" r="56"/>
            </g>
            <g fill="#26358c">
              <path d="M482 138 512 121l30 17v34l-30 18-30-18v-34Zm17 11v22l13 8 13-8v-22l-13-8-13 8Z"/>
              <path d="M174 369h55v18h-55v-18Zm24-23 38 38-38 38-13-13 25-25-25-25 13-13Z"/>
            </g>
            <text x="826" y="406" textAnchor="middle" fontFamily="-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif" fontSize="72" fontWeight="700" fill="#ef4d65">AI</text>
            <circle cx="300" cy="752" r="42" fill="#111827"/>
            <path d="M300 717 334 737 300 757 266 737 300 717Zm-34 45 34 20 34-20v24l-34 20-34-20v-24Z" fill="#dce4ef"/>
            <path d="M696 788c34-108 86-108 120 0h-34c-22-58-38-58-60 0h-26Z" fill="#2f73ff"/>
            <circle cx="326" cy="228" r="14" fill="#90caff" stroke="#d8f0ff" strokeWidth="6"/>
            <circle cx="706" cy="228" r="14" fill="#d690ff" stroke="#f3ddff" strokeWidth="6"/>
            <circle cx="576" cy="724" r="14" fill="#74ece0" stroke="#dffffb" strokeWidth="6"/>
          </g>
        </svg>
        <div className="splash-text">Loading LAM...</div>
      </div>
    );
  }

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
            <UIButton
              size="sm"
              className={`toolbarBtn refreshToolbarBtn ${refreshingAccounts || refreshingAntigravity ? 'isRefreshing' : ''}`}
              onClick={() => void handleRefreshAll()}
              disabled={refreshingAccounts || refreshingAntigravity}
              aria-label={refreshingAccounts || refreshingAntigravity ? 'Refreshing app data' : 'Refresh app data'}
            >
              <IconRefresh size={14} /> Refresh
            </UIButton>
            <UIButton size="sm" className="toolbarBtn" onClick={() => openModal('account')}>
              <IconPlus size={14} /> New Account
            </UIButton>
            <UIButton size="sm" className="toolbarBtn" onClick={() => openModal('provider')}>
              <IconPlus size={14} /> New Provider
            </UIButton>
          </div>
          <ThemeToggle value={themeMode} onChange={setThemeMode} />
        </div>
      </header>

      <section className="content">
        {error ? <div className="notice danger">{error}</div> : null}
        {appReady && route === 'overview' ? (
          <Views.Overview
            accounts={accounts}
            quotas={quotas}
            providers={providers}
            select={setSelectedAccountId}
            openSync={openSyncModal}
            rename={openRenameAccountModal}
            login={login}
            openHandoff={(account) => openHandoffModal({ targetAccountId: account.id })}
            relayLatest={relayResumeTo}
            currentSession={activeSession}
            refreshAccountQuota={refreshAccountQuota}
            refreshingQuotaIds={refreshingQuotaIds}
            antigravityQuota={antigravityQuota}
            refreshingAntigravity={refreshingAntigravity}
            onRefreshAntigravity={() => void loadAntigravity(true)}
          />
        ) : null}
        {appReady && route === 'sessions' ? (
          <Views.Sessions
            sessions={filtered}
            accounts={accounts}
            selectedAccountId={selectedAccountId}
            setSelectedAccountId={setSelectedAccountId}
            query={query}
            setQuery={setQuery}
            copy={copyResume}
            open={openResume}
            details={openSessionDetails}
            openHandoff={(session) => openHandoffModal({ session })}
          />
        ) : null}
        {appReady && route === 'providers' ? (
          <Views.Providers
            accounts={accounts}
            providers={providers}
            create={() => openModal('provider')}
            test={runProviderTest}
            remove={removeProvider}
            attach={openAttachProviderModal}
          />
        ) : null}
        {appReady && route === 'sync' ? (
          <Views.SyncHome accounts={accounts} openSync={openSyncModal} />
        ) : null}
        {appReady && route === 'settings' ? (
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
          <Shell.BottomNav route={route} setRoute={setRoute} />
        </div>
      </footer>

      <div className="statusbarOutside">
        <div className="statusbarOutsideLeft">
          <IconClock size={12} className="statusbarClock" />
          <span className="statusbarMain">{footerStatus}</span>
          {selectedSession?.providerMismatch ? (
            <span className="statusHint">Provider mismatch</span>
          ) : null}
        </div>
        <span className="bottomDockMeta">
          <span className={`statusDot ${health?.ok ? 'ok' : ''}`} aria-hidden />
          {health?.ok ? 'Operational' : 'Checking…'}
          <span className="bottomDockVersion">
            {health?.version ? `v${health.version}` : 'v1.0.0'}
          </span>
        </span>
      </div>

      {modal === 'account' ? (
        <Shell.Modal title="Add Managed Account" close={closeModal}>
          <div className="formGrid">
            <label>
              Account name
              <input
                value={accountReq.name}
                onChange={(e) => setAccountReq({ ...accountReq, name: e.target.value })}
              />
            </label>
            <label>
              Copy config from
              <select
                value={accountReq.copyConfigFrom ?? ''}
                onChange={(e) =>
                  setAccountReq({ ...accountReq, copyConfigFrom: e.target.value || null })
                }
              >
                <option value="">None</option>
                {accounts
                  .filter((a) => a.hasConfig)
                  .map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.id}
                    </option>
                  ))}
              </select>
            </label>
          </div>
          <div className="previewBox">
            <div className="previewLine">
              <span>CODEX_HOME</span>
              <strong>~/.codex-{accountReq.name || 'name'}</strong>
            </div>
            <div className="previewLine">
              <span>Wrapper</span>
              <strong>~/bin/codex-{accountReq.name || 'name'}</strong>
            </div>
          </div>
          <label className="syncOption">
            <input
              type="checkbox"
              checked={accountReq.overwriteWrapper}
              onChange={(e) => setAccountReq({ ...accountReq, overwriteWrapper: e.target.checked })}
            />
            <span>
              <strong>Overwrite wrapper if it exists</strong>
              <span>Keeps CODEX_HOME untouched; only wrapper script is replaced.</span>
            </span>
          </label>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>
              Cancel
            </UIButton>
            <div className="modalFootPrimary">
              <UIButton
                type="button"
                onClick={async () => setPlan(await api.planCreateAccount(accountReq))}
              >
                Dry Run
              </UIButton>
              <UIButton
                type="button"
                variant="primary"
                disabled={!plan}
                onClick={async () => {
                  await api.executeCreateAccount(accountReq);
                  closeModal();
                  setPlan(null);
                  await refresh();
                }}
              >
                Create
              </UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === 'renameAccount' ? (
        <Shell.Modal title="Rename Account" close={closeModal}>
          <div className="formGrid">
            <label>
              Current account
              <select
                value={renameReq.fromProfileId}
                onChange={(e) => {
                  setRenameReq({ ...renameReq, fromProfileId: e.target.value });
                  setPlan(null);
                }}
              >
                {accounts
                  .filter((a) => a.id !== 'main')
                  .map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.displayName}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              New account name
              <input
                value={renameReq.toName}
                onChange={(e) => {
                  setRenameReq({ ...renameReq, toName: e.target.value });
                  setPlan(null);
                }}
              />
            </label>
          </div>
          <div className="previewBox">
            <div className="previewLine">
              <span>From</span>
              <strong>~/.codex-{renameReq.fromProfileId || 'source'}</strong>
            </div>
            <div className="previewLine">
              <span>To</span>
              <strong>~/.codex-{renameReq.toName || 'name'}</strong>
            </div>
            <div className="previewLine">
              <span>Wrapper</span>
              <strong>~/bin/codex-{renameReq.toName || 'name'}</strong>
            </div>
          </div>
          <label className="syncOption">
            <input
              type="checkbox"
              checked={renameReq.overwriteWrapper}
              onChange={(e) => {
                setRenameReq({ ...renameReq, overwriteWrapper: e.target.checked });
                setPlan(null);
              }}
            />
            <span>
              <strong>Overwrite target wrapper if it exists</strong>
              <span>Profile directory conflicts are always blocked.</span>
            </span>
          </label>
          <div className="notice">
            Close any running Codex process that uses this profile before renaming. auth.json is
            kept inside the moved CODEX_HOME and is not copied.
          </div>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>
              Cancel
            </UIButton>
            <div className="modalFootPrimary">
              <UIButton
                type="button"
                onClick={async () => setPlan(await api.planRenameAccount(renameReq))}
              >
                Dry Run
              </UIButton>
              <UIButton type="button" variant="primary" disabled={!plan} onClick={renameAccount}>
                Rename
              </UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === 'handoff' ? (
        <Shell.Modal title="Handoff Session" close={closeModal}>
          <div className="formGrid">
            <label>
              Source account
              <select value={handoffSourceId} onChange={(e) => changeHandoffSource(e.target.value)}>
                {accounts.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.displayName}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Session
              <select
                value={handoffSessionId}
                onChange={(e) => setHandoffSessionId(e.target.value)}
                disabled={handoffLoading || !handoffSessions.length}
              >
                {handoffSessions.map((s) => (
                  <option key={`${s.accountId}-${s.id}-${s.path}`} value={s.id}>
                    {s.id} · {s.cwd ?? 'unknown cwd'}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Target account
              <select value={handoffTargetId} onChange={(e) => setHandoffTargetId(e.target.value)}>
                {accounts.map((a) => (
                  <option key={a.id} value={a.id} disabled={a.id === handoffSourceId}>
                    {a.displayName}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="previewBox">
            <div className="previewLine">
              <span>Source</span>
              <strong>
                {accounts.find((a) => a.id === handoffSourceId)?.displayName ??
                  (handoffSourceId || 'source')}
              </strong>
            </div>
            <div className="previewLine">
              <span>Target runtime</span>
              <strong>
                {accounts.find((a) => a.id === handoffTargetId)?.displayName ??
                  (handoffTargetId || 'target')}
              </strong>
            </div>
            <div className="previewLine">
              <span>Session</span>
              <strong>
                {handoffSessionId || (handoffLoading ? 'Loading...' : 'No session selected')}
              </strong>
            </div>
          </div>
          <div className="notice">
            Handoff copies the selected session transcript to the target account when needed, never
            auth.json or API secrets.
          </div>
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>
              Cancel
            </UIButton>
            <div className="modalFootPrimary">
              <UIButton
                type="button"
                variant="primary"
                disabled={
                  !handoffSessionId ||
                  !handoffSourceId ||
                  !handoffTargetId ||
                  handoffLoading ||
                  handoffSourceId === handoffTargetId
                }
                onClick={async () => {
                  const session = handoffSessions.find((s) => s.id === handoffSessionId);
                  const target = accounts.find((a) => a.id === handoffTargetId);
                  if (!session || !target) return;
                  await relaySessionTo(session, target);
                  closeModal();
                  await refresh();
                }}
              >
                Start Handoff
              </UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === 'sync' && syncReq ? (
        <Shell.Modal title="Sync Sessions Safely" wide close={closeModal}>
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
            onDryRun={async () => {
              if (syncReq) setPlan(await api.buildSyncPlan(syncReq));
            }}
            onExecute={async () => {
              if (syncReq && plan) {
                setSyncResult(await api.executeSync(syncReq));
                await refresh();
              }
            }}
            onClose={closeModal}
          />
        </Shell.Modal>
      ) : null}

      {modal === 'provider' ? (
        <Shell.Modal title="Add External Provider" close={closeModal}>
          <div className="formGrid">
            <label>
              Provider id
              <input
                value={providerReq.id}
                onChange={(e) => setProviderReq({ ...providerReq, id: e.target.value })}
              />
            </label>
            <label>
              Name
              <input
                value={providerReq.name}
                onChange={(e) => setProviderReq({ ...providerReq, name: e.target.value })}
              />
            </label>
            <label>
              Base URL
              <input
                value={providerReq.baseUrl}
                onChange={(e) => setProviderReq({ ...providerReq, baseUrl: e.target.value })}
              />
            </label>
            <label>
              Wire API
              <input
                value={providerReq.wireApi}
                onChange={(e) => setProviderReq({ ...providerReq, wireApi: e.target.value })}
              />
            </label>
            <label>
              Default model
              <input
                value={providerReq.defaultModel}
                onChange={(e) => setProviderReq({ ...providerReq, defaultModel: e.target.value })}
              />
            </label>
            <label>
              Env key
              <input
                value={providerReq.envKey ?? ''}
                onChange={(e) => setProviderReq({ ...providerReq, envKey: e.target.value || null })}
              />
            </label>
          </div>
          <div className="previewBox">
            <div className="previewLine">
              <span>Provider ID</span>
              <strong>{providerReq.id || 'provider-id'}</strong>
            </div>
            <div className="previewLine">
              <span>Secret storage</span>
              <strong>{providerReq.envKey ? `ENV (${providerReq.envKey})` : 'None'}</strong>
            </div>
          </div>
          <div className="notice">
            Secrets are not shown or stored in config.toml. Use an environment variable or
            Keychain-backed secret storage.
          </div>
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>
              Cancel
            </UIButton>
            <div className="modalFootPrimary">
              <UIButton
                type="button"
                variant="primary"
                onClick={() => createFromModal(providerReq)}
              >
                Create Provider
              </UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === 'attachProvider' ? (
        <Shell.Modal title="Attach Provider to Account" close={closeModal}>
          <div className="formGrid">
            <label>
              Account
              <select
                value={attachReq.profileId}
                onChange={(e) => setAttachReq({ ...attachReq, profileId: e.target.value })}
              >
                {accounts.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.id}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Provider
              <select
                value={attachReq.providerId}
                onChange={(e) => setAttachReq({ ...attachReq, providerId: e.target.value })}
              >
                {providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.id}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Model
              <input
                value={attachReq.model ?? ''}
                onChange={(e) => setAttachReq({ ...attachReq, model: e.target.value || null })}
              />
            </label>
          </div>
          <div className="previewBox">
            <div className="previewLine">
              <span>Target profile</span>
              <strong>{attachReq.profileId || 'profile'}</strong>
            </div>
            <div className="previewLine">
              <span>Provider</span>
              <strong>{attachReq.providerId || 'provider'}</strong>
            </div>
          </div>
          <div className="notice">
            Attach backs up config.toml and writes provider references only. API keys are never
            written.
          </div>
          <Views.PlanView plan={plan} />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>
              Cancel
            </UIButton>
            <div className="modalFootPrimary">
              <UIButton
                type="button"
                onClick={async () => setPlan(await api.planAttachProviderToProfile(attachReq))}
              >
                Dry Run
              </UIButton>
              <UIButton
                type="button"
                variant="primary"
                disabled={!plan}
                onClick={() => attachToProfile(attachReq)}
              >
                Attach
              </UIButton>
            </div>
          </div>
        </Shell.Modal>
      ) : null}

      {modal === 'sessionDetail' && selectedSession ? (
        <Shell.Modal title="Session Details" wide close={closeModal}>
          <div className="detailTitleRow">
            <h3 className="detailTitle">{selectedSession.id}</h3>
            {selectedSession.providerMismatch ? <span className="badge warn">mismatch</span> : null}
          </div>
          {selectedSession.providerMismatch ? (
            <p className="notice warn">
              Resume can continue, but runtime behavior, cost, and tool compatibility may differ.
            </p>
          ) : null}
          <div className="detailGrid">
            <div className="detailItem">
              <span className="detailLabel">Account</span>
              <strong>{selectedSessionAccount?.codexHome ?? selectedSession.accountId}</strong>
            </div>
            <div className="detailItem">
              <span className="detailLabel">cwd</span>
              <strong>{selectedSession.cwd ?? 'unknown'}</strong>
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
                {selectedSession.originalProviderId ?? 'unknown'} ·{' '}
                {selectedSession.originalModel ?? 'unknown'}
              </strong>
            </div>
            <div className="detailItem">
              <span className="detailLabel">Runtime provider</span>
              <strong>
                {selectedSession.currentProviderId ?? 'unknown'} ·{' '}
                {selectedSession.currentModel ?? 'unknown'}
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
            <UIButton type="button" variant="ghost" onClick={closeModal}>
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
