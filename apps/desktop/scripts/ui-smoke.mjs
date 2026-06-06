import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const srcRoot = fileURLToPath(new URL("../src", import.meta.url));
function readTree(dir) {
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .map((entry) => {
      const current = path.join(dir, entry.name);
      return entry.isDirectory() ? readTree(current) : fs.readFileSync(current, "utf8");
    })
    .join("\n");
}

const app = readTree(srcRoot);
const api = fs.readFileSync(new URL("../src/lib/api.ts", import.meta.url), "utf8");
const tauriCommands = fs.readFileSync(new URL("../src-tauri/src/commands/mod.rs", import.meta.url), "utf8");
const servicesDir = fileURLToPath(new URL("../src-tauri/src/services", import.meta.url));
const tauriCore = fs
  .readdirSync(servicesDir)
  .filter((f) => f.endsWith(".rs"))
  .map((f) => fs.readFileSync(path.join(servicesDir, f), "utf8"))
  .join("\n");
const iconSvg = fs.readFileSync(new URL("../src-tauri/icons/icon.svg", import.meta.url), "utf8");
const orbitIconExists = fs.existsSync(new URL("../src/assets/lam-orbit-icon.svg", import.meta.url));

const checks = [
  ["empty state", app.includes("No Codex profiles found.") && app.includes("No sessions.")],
  ["bottom navigation dock", app.includes("bottomNav") && app.includes("bottomNavIcon")],
  [
    "account cards dense and sorted",
    app.includes("latestSessionModifiedAt") &&
      app.includes("accountCardGrid") &&
      app.includes("badge--auth") && app.includes("Logged in"),
  ],
  [
    "sessions actions stay inside table",
    app.includes("rowActions") && app.includes("Terminal") && app.includes("IconCopy"),
  ],
  [
    "sync requires dry-run",
    (app.includes("Dry-run first") || app.includes("Run dry-run")) &&
      (app.includes("disabled={!canExecute}") || app.includes("disabled={!plan}")),
  ],
  ["sync route layout", app.includes("syncRoute") && app.includes("routeBox")],
  ["grouped sync plan", app.includes("planGrouped") && app.includes("Will copy")],
  [
    "quota status states",
    app.includes("Quota usable") &&
      app.includes("\"N/A\"") &&
      app.includes("% left") &&
      app.includes("quotaColorState") &&
      app.includes("quotaBar--safe") &&
      app.includes("quotaBar--warn") &&
      app.includes("quotaBar--danger") &&
      app.includes("quotaBar--empty") &&
      !app.includes("% used") &&
      !app.includes("Activity estimate; no reset countdown") &&
      !app.includes("est tokens"),
  ],
  [
    "tray quota reference style",
    app.includes("trayBrandMark") &&
      app.includes("trayStats") &&
      app.includes("trayProviderTabs") &&
      app.includes("trayProviderPanel") &&
      app.includes("showProviderTabs") &&
      app.includes("trayAccountRow") &&
      app.includes("trayAccountRowTop") &&
      app.includes("accountActiveBadge") &&
      !app.includes("trayAccountRow--active") &&
      app.includes("trayAccountRing") &&
      app.includes("trayRelayButton") &&
      app.includes("trayRefreshButton") &&
      app.includes("trayResetSub") &&
      app.includes("trayQuotaMeter") &&
      app.includes("trayQuotaMeter--safe") &&
      app.includes("trayQuotaMeter--warn") &&
      app.includes("trayQuotaMeter--danger") &&
      app.includes("formatResetCountdown") &&
      app.includes('label="5h"') &&
      app.includes('label="weekly"') &&
      app.includes("grid-template-columns: minmax(0, 1fr) minmax(0, 1fr)") &&
      app.includes("Refreshing…") &&
      app.includes("latestSessionModifiedAt") &&
      app.includes("LAM quota") &&
      app.includes("countAccountsWithAvailableQuota") &&
      app.includes("accountsWithQuotaData") &&
      app.includes("availableQuotaAccounts") &&
      app.includes("height: 20px") &&
      app.includes("trayPopoverActions") &&
      app.includes("inset: 3px") &&
      app.includes("height: 4px") &&
      app.includes("rgba(32, 33, 32, 0.98)") &&
      !app.includes("trayProviderGroup") &&
      !app.includes("trayAccountCard") &&
      !app.includes("trayRefreshBtn") &&
      !app.includes("trayOpacityMini"),
  ],
  [
    "brand icon unified",
    app.includes("IconLogo") &&
      app.includes("LAMOrbitLogo") &&
      app.includes("lam-orbit-icon.svg") &&
      orbitIconExists &&
      iconSvg.includes("LAMLogo") &&
      iconSvg.includes("LamOrbit") &&
      iconSvg.includes("scale(0.8)") &&
      !iconSvg.includes("#000000"),
  ],
  [
    "quota cards do not overflow",
    app.includes(".accountQuota") &&
      app.includes("min-width: 0") &&
      app.includes("overflow-wrap: anywhere") &&
      app.includes("width: 100%"),
  ],
  [
    "startup accounts cache then scan",
    app.includes("listCachedAccounts") &&
      app.includes("applyAccountsList") &&
      app.includes("fromCache") &&
      app.includes("scanning…") &&
      api.includes("list_cached_accounts") &&
      tauriCommands.includes("pub async fn list_cached_accounts") &&
      tauriCore.includes("fn list_cached_accounts") &&
      tauriCore.includes("accounts-cache.json"),
  ],
  [
    "startup quota is nonblocking",
    app.includes("loadCachedQuotas") &&
      app.includes("scheduleQuotaRefresh") &&
      app.includes("getProfileQuota(profileId, true)") &&
      app.includes("mergeQuotaSnapshots") &&
      !app.includes("quotaRefreshInFlightRef") &&
      !app.includes("refreshAllQuotas(ids)") &&
      !app.includes("await refreshAllQuotas(accountData.map((account) => account.id))"),
  ],
  [
    "cached real quota startup path",
    app.includes("listCachedQuotas") &&
      app.includes("listCachedAccounts") &&
      api.includes("list_cached_quotas") &&
      api.includes("invoke<UsageQuotaSnapshot[]>(\"list_cached_quotas\""),
  ],
  [
    "real quota is decoupled from session estimates",
    tauriCore.includes("fn quota_account") &&
      tauriCore.includes("usage_unavailable") &&
      !tauriCore.includes("Activity estimate only; not real quota") &&
      !tauriCore.includes("Consider switching profile or creating a relay workspace."),
  ],
  [
    "heavy tauri commands are nonblocking",
    tauriCommands.includes("async fn run_blocking") &&
      tauriCommands.includes("pub async fn list_accounts") &&
      tauriCommands.includes("pub async fn list_sessions") &&
      tauriCommands.includes("pub async fn get_profile_quota") &&
      tauriCommands.includes("pub async fn list_providers") &&
      tauriCommands.includes("spawn_blocking"),
  ],
  ["provider delete safety", app.includes("variant=\"danger\"") && app.includes("Delete provider") && app.includes("window.confirm")],
  [
    "provider center",
    app.includes("Providers") &&
      app.includes("infoBanner") &&
      app.includes("Attach Provider to Account") &&
      app.includes("API keys are never returned to the UI"),
  ],
  [
    "provider mismatch",
    app.includes("provider mismatch") &&
      app.includes("runtime behavior, cost, and tool compatibility may differ"),
  ],
  [
    "tauri invokes",
    api.includes("invoke<HealthCheck>(\"health_check\")") &&
      api.includes("execute_sync") &&
      api.includes("relay_resume_session") &&
      api.includes("open_terminal_with_command") &&
      api.includes("create_provider") &&
      api.includes("refresh_all_quotas"),
  ],
  [
    "relay resume entry",
    app.includes("Resume Here") &&
      app.includes("relayResumeSession") &&
      app.includes("openTerminalWithCommand") &&
      app.includes("activeSession") &&
      app.includes("refreshActiveSession") &&
      app.includes("summarize_fork_with_target_account") &&
      app.includes("Existing session was not overwritten.") &&
      tauriCommands.includes("pub fn relay_resume_session"),
  ],
  [
    "latest active relay controls",
    app.includes("Active source") &&
      app.includes("activeSessionBanner") &&
      app.includes("trayActiveSource") &&
      app.includes("trayRelayButton") &&
      app.includes("2 * 60_000"),
  ],
  [
    "diverged strategy settings",
    app.includes("Diverged session strategy") &&
      app.includes("setDivergedStrategy") &&
      app.includes("timeline_merge_to_fork") &&
      app.includes("prefer_source") &&
      app.includes("prefer_target"),
  ],
];

const failed = checks.filter(([, ok]) => !ok);
if (failed.length) {
  console.error("UI smoke failed:");
  for (const [name] of failed) console.error(`- ${name}`);
  process.exit(1);
}

console.log("UI smoke passed");
