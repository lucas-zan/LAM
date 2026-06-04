import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  hideQuotaPopover,
  listAccounts,
  listCachedQuotas,
  listSessions,
  openTerminalWithCommand,
  openTerminalWithResume,
  relayResumeSession,
  refreshAllQuotas,
  inTauri,
  setQuotaPopoverOpacity,
} from "../lib/api";
import { quotaColorState, quotaRemainingPercent } from "../lib/quota";
import { formatResetCountdown } from "../lib/reset";
import type { ThemeMode } from "../lib/theme";
import { TRAY_POPOVER_OPACITY_PERCENT } from "../lib/tray-popover-prefs";
import type { CodexAccount, CodexSession, DivergedSessionStrategy, UsageQuotaSnapshot } from "../lib/types";
import { IconClock, IconClose, IconExternalLink, IconLogo, IconRefresh } from "./icons";
import { UIButton } from "./ui-button";

function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

function resolveThemeMode(): ThemeMode {
  const saved = localStorage.getItem("lam-theme");
  return saved === "light" || saved === "dark" || saved === "system" ? saved : "system";
}

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

function TrayQuotaMeter(props: { label: string; used?: number | null; resetAt?: string | null }) {
  const remaining = quotaRemainingPercent(props.used);
  const state = quotaColorState(props.used);
  const width = remaining === null || state === "empty" ? 0 : Math.max(3, remaining);
  return (
    <div className={`trayQuotaMeter trayQuotaMeter--${state}`}>
      <div className="trayQuotaMeterHead">
        <span>{props.label}</span>
        <strong>{remaining === null ? "N/A" : `${remaining}%`}</strong>
      </div>
      <div className="trayQuotaTrack">
        <i style={{ width: `${width}%` }} />
      </div>
      <span className="trayResetLine">{formatResetCountdown(props.resetAt)}</span>
    </div>
  );
}

function accountRemaining(quota?: UsageQuotaSnapshot): number | null {
  if (!quota) return null;
  const values = [quotaRemainingPercent(quota.primaryUsedPercent), quotaRemainingPercent(quota.secondaryUsedPercent)].filter(
    (value): value is number => value !== null,
  );
  if (!values.length) return null;
  return Math.min(...values);
}

function quotaStateFromRemaining(remaining: number | null) {
  return remaining === null ? "na" : quotaColorState(100 - remaining);
}

function averageRemaining(quotas: UsageQuotaSnapshot[]): number | null {
  const values = quotas
    .flatMap((quota) => [
      quotaRemainingPercent(quota.primaryUsedPercent),
      quotaRemainingPercent(quota.secondaryUsedPercent),
    ])
    .filter((value): value is number => value !== null);
  if (!values.length) return null;
  return Math.round(values.reduce((sum, value) => sum + value, 0) / values.length);
}

function sortByLatestActivity(accounts: CodexAccount[]) {
  return [...accounts].sort((a, b) => {
    const aLatest = a.latestSessionModifiedAt ?? 0;
    const bLatest = b.latestSessionModifiedAt ?? 0;
    const latestDiff = bLatest - aLatest;
    if (latestDiff !== 0) return latestDiff;
    const sessionDiff = b.sessionCount - a.sessionCount;
    if (sessionDiff !== 0) return sessionDiff;
    return a.displayName.localeCompare(b.displayName);
  });
}

function TrayAccountRing(props: { remaining: number | null; title: string }) {
  const state = quotaStateFromRemaining(props.remaining);
  const value = props.remaining === null ? 0 : props.remaining;
  return (
    <span
      className={`trayAccountRing trayAccountRing--${state}`}
      aria-label={`${props.title} remaining ${props.remaining === null ? "N/A" : `${props.remaining}%`}`}
      style={{ "--ring-value": `${value}%` } as CSSProperties}
    />
  );
}

function readResolvedTheme(): "light" | "dark" {
  const mode = resolveThemeMode();
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return mode;
}

export function TrayQuotaPanel() {
  const [accounts, setAccounts] = useState<CodexAccount[]>([]);
  const [quotas, setQuotas] = useState<UsageQuotaSnapshot[]>([]);
  const [activeSession, setActiveSession] = useState<CodexSession | undefined>(undefined);
  const [status, setStatus] = useState("Loading…");
  const [refreshing, setRefreshing] = useState(false);
  const [relayingAccountId, setRelayingAccountId] = useState<string>("");
  const [resolvedTheme, setResolvedTheme] = useState<"light" | "dark">(() => readResolvedTheme());

  const applyTheme = useCallback(() => {
    const resolved = readResolvedTheme();
    setResolvedTheme(resolved);
    document.documentElement.dataset.theme = resolved;
  }, []);

  useEffect(() => {
    document.documentElement.dataset.trayPopover = "1";
    applyTheme();
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyTheme();
    media.addEventListener("change", onChange);
    const onStorage = (event: StorageEvent) => {
      if (event.key === "lam-theme") applyTheme();
    };
    window.addEventListener("storage", onStorage);
    return () => {
      delete document.documentElement.dataset.trayPopover;
      media.removeEventListener("change", onChange);
      window.removeEventListener("storage", onStorage);
    };
  }, [applyTheme]);

  useEffect(() => {
    if (inTauri()) {
      void setQuotaPopoverOpacity(TRAY_POPOVER_OPACITY_PERCENT);
    }
  }, []);

  const loadActiveSession = useCallback(async (accountData: CodexAccount[]) => {
    const results = await Promise.allSettled(accountData.map((account) => listSessions(account.id)));
    const allSessions = results.flatMap((result) => (result.status === "fulfilled" ? result.value : []));
    setActiveSession(allSessions.sort((a, b) => b.modifiedAt - a.modifiedAt)[0]);
  }, []);

  const load = useCallback(async (forceRefresh = false) => {
    if (!inTauri()) {
      setStatus("Tray panel requires the desktop app.");
      return;
    }
    if (forceRefresh) {
      setRefreshing(true);
      setStatus("Refreshing…");
    }
    try {
      const accountData = await listAccounts();
      setAccounts(accountData);
      void loadActiveSession(accountData);
      const ids = accountData.map((a) => a.id);
      if (forceRefresh && ids.length) {
        const result = await refreshAllQuotas(ids);
        setQuotas(result.snapshots);
        setStatus(`Updated ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`);
      } else {
        const cached = await listCachedQuotas(ids.length ? ids : undefined);
        setQuotas(cached);
        setStatus(`Cached · ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`);
      }
    } catch (err) {
      setStatus(formatError(err));
    } finally {
      setRefreshing(false);
    }
  }, [loadActiveSession]);

  useEffect(() => {
    void load(false);
    const timer = window.setInterval(() => {
      void load(true);
    }, 2 * 60_000);
    const unlisten = listen("quota-popover-refresh", () => {
      void load(true);
    });
    return () => {
      window.clearInterval(timer);
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  async function closePopover() {
    if (inTauri()) {
      await hideQuotaPopover();
      return;
    }
    await getCurrentWebviewWindow().hide();
  }

  async function openMain() {
    await invoke("show_main_window");
    await closePopover();
  }

  async function relayTo(account: CodexAccount) {
    if (!activeSession) {
      setStatus("No active session found.");
      return;
    }
    setRelayingAccountId(account.id);
    try {
      if (account.id !== activeSession.accountId) {
        const result = await relayResumeSession({
          fromProfileId: activeSession.accountId,
          toProfileId: account.id,
          sessionId: activeSession.id,
          cwd: activeSession.cwd,
          divergedStrategy: readDivergedStrategy(),
        });
        await openTerminalWithCommand(result.resume.command);
      } else {
        await openTerminalWithResume({
          profileId: account.id,
          sessionId: activeSession.id,
          cwd: activeSession.cwd,
        });
      }
      setStatus(`Resume ${activeSession.id} on ${account.id}`);
      await loadActiveSession(accounts);
    } catch (err) {
      setStatus(formatError(err));
    } finally {
      setRelayingAccountId("");
    }
  }

  const liveQuotaCount = quotas.filter(
    (quota) => quota.primaryUsedPercent !== null && quota.primaryUsedPercent !== undefined,
  ).length;
  const avgRemaining = averageRemaining(quotas);
  const activeAccount = activeSession ? accounts.find((account) => account.id === activeSession.accountId) : undefined;
  const orderedAccounts = sortByLatestActivity(accounts);
  const accountGroups = accounts.length
    ? [
        {
          id: "codex",
          title: "Codex",
          meta: "CLI",
          accounts: orderedAccounts,
        },
      ]
    : [];

  return (
    <div className="trayPopoverPanel" data-theme={resolvedTheme}>
      <header className="trayPopoverHead">
        <div className="trayBrand">
          <span className="trayBrandMark" aria-hidden>
            <IconLogo size={30} />
          </span>
          <div>
            <h2>LAM quota</h2>
            <p><IconClock size={12} /> {status}</p>
          </div>
        </div>
        <button
          type="button"
          className={`trayPopoverClose trayRefreshButton ${refreshing ? "isRefreshing" : ""}`}
          aria-label={refreshing ? "Refreshing quotas" : "Refresh quotas"}
          aria-busy={refreshing}
          disabled={refreshing}
          onClick={() => void load(true)}
        >
          <IconRefresh size={16} />
        </button>
      </header>

      <section className="trayStats" aria-label="Quota summary">
        <div>
          <span>Accounts</span>
          <strong>{accounts.length}</strong>
        </div>
        <div>
          <span>Average</span>
          <strong>{avgRemaining === null ? "N/A" : `${avgRemaining}%`}</strong>
        </div>
        <div>
          <span>Live</span>
          <strong>{liveQuotaCount}</strong>
        </div>
      </section>

      <section className="trayActiveSource" aria-label="Active source session">
        <span>Active</span>
        <strong>{activeAccount?.displayName ?? activeSession?.accountId ?? "No session"}</strong>
        <em className="mono">{activeSession?.id ?? "No active session"}</em>
      </section>

      <div className="trayPopoverList">
        {!accounts.length ? (
          <p className="trayPopoverEmpty">No Codex profiles found.</p>
        ) : (
          accountGroups.map((group) => (
            <section className="trayProviderGroup" key={group.id} aria-label={`${group.title} accounts`}>
              <div className="trayProviderGroupHead">
                <div>
                  <span className="trayProviderIcon" aria-hidden>
                    <IconLogo size={16} />
                  </span>
                  <strong>{group.title}</strong>
                  <em>{group.accounts.length}</em>
                </div>
                <span>{group.meta}</span>
              </div>
              <div className="trayProviderRows">
                {group.accounts.map((account) => {
                  const quota = quotas.find((q) => q.profileId === account.id);
                  const title = account.displayName.trim() || account.id;
                  const remaining = accountRemaining(quota);
                  const state = quotaStateFromRemaining(remaining);
                  return (
                    <div className="trayAccountRow" key={account.id}>
                      <div className="trayAccountMain">
                        <TrayAccountRing remaining={remaining} title={title} />
                        <strong>{title}</strong>
                      </div>
                      <div className="trayAccountMeters">
                        <TrayQuotaMeter label="5h" used={quota?.primaryUsedPercent} resetAt={quota?.resetAt} />
                        <TrayQuotaMeter label="7d" used={quota?.secondaryUsedPercent} resetAt={quota?.secondaryResetAt} />
                      </div>
                      <strong className={`trayAccountRemaining trayAccountRemaining--${state}`}>
                        {remaining === null ? "N/A" : `${remaining}%`}
                      </strong>
                      <button
                        type="button"
                        className="trayRelayButton"
                        disabled={!activeSession || relayingAccountId === account.id}
                        onClick={() => void relayTo(account)}
                      >
                        {activeSession?.accountId === account.id ? "Resume" : "Relay"}
                      </button>
                    </div>
                  );
                })}
              </div>
            </section>
          ))
        )}
      </div>

      <footer className="trayPopoverFoot">
        <div className="trayPopoverActions">
          <UIButton size="sm" variant="primary" onClick={() => void openMain()}>
            <IconExternalLink size={13} />
            Open
          </UIButton>
          <span />
          <UIButton size="sm" variant="ghost" onClick={() => void closePopover()}>
            <IconClose size={13} />
            Close
          </UIButton>
        </div>
      </footer>
    </div>
  );
}
