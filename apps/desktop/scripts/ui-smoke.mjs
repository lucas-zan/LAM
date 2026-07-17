import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const srcRoot = fileURLToPath(new URL('../src', import.meta.url));
function readTree(dir) {
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .map((entry) => {
      const current = path.join(dir, entry.name);
      return entry.isDirectory() ? readTree(current) : fs.readFileSync(current, 'utf8');
    })
    .join('\n');
}

const app = readTree(srcRoot);
const api = fs.readFileSync(new URL('../src/lib/api.ts', import.meta.url), 'utf8');
const quotaLib = fs.readFileSync(new URL('../src/lib/quota.ts', import.meta.url), 'utf8');
const views = fs.readFileSync(new URL('../src/routes/views.tsx', import.meta.url), 'utf8');
const trayPanel = fs.readFileSync(
  new URL('../src/components/tray-quota-panel.tsx', import.meta.url),
  'utf8',
);
const traySize = fs.readFileSync(
  new URL('../src/lib/tray-popover-size.ts', import.meta.url),
  'utf8',
);
const tauriConfig = fs.readFileSync(
  new URL('../src-tauri/tauri.conf.json', import.meta.url),
  'utf8',
);
const tauriTray = fs.readFileSync(new URL('../src-tauri/src/tray.rs', import.meta.url), 'utf8');
const tauriCommands = fs.readFileSync(
  new URL('../src-tauri/src/commands/mod.rs', import.meta.url),
  'utf8',
);
const servicesDir = fileURLToPath(new URL('../src-tauri/src/services', import.meta.url));
const tauriCore = fs
  .readdirSync(servicesDir)
  .filter((f) => f.endsWith('.rs'))
  .map((f) => fs.readFileSync(path.join(servicesDir, f), 'utf8'))
  .join('\n');
const iconSvg = fs.readFileSync(new URL('../src-tauri/icons/icon.svg', import.meta.url), 'utf8');
const orbitIconExists = fs.existsSync(new URL('../src/assets/lam-orbit-icon.svg', import.meta.url));

const checks = [
  ['empty state', app.includes('No Codex profiles found.') && app.includes('No sessions.')],
  ['bottom navigation dock', app.includes('bottomNav') && app.includes('bottomNavIcon')],
  [
    'account cards dense and sorted',
    app.includes('latestSessionModifiedAt') &&
      app.includes('accountCardGrid') &&
      app.includes('badge--auth') &&
      app.includes('Logged in'),
  ],
  [
    'sessions actions stay inside table',
    app.includes('rowActions') && app.includes('Terminal') && app.includes('IconCopy'),
  ],
  [
    'quota status states',
    app.includes('Quota usable') &&
      app.includes("'N/A'") &&
      app.includes('% left') &&
      app.includes('quotaColorState') &&
      app.includes('quotaBar--safe') &&
      app.includes('quotaBar--warn') &&
      app.includes('quotaBar--danger') &&
      app.includes('quotaBar--empty') &&
      !app.includes('% used') &&
      !app.includes('Activity estimate; no reset countdown') &&
      !app.includes('est tokens'),
  ],
  [
    'tray quota reference style',
    app.includes('trayBrandMark') &&
      app.includes('trayStats') &&
      app.includes('trayProviderTabs') &&
      app.includes('trayProviderPanel') &&
      app.includes('showProviderTabs') &&
      app.includes('trayAccountRow') &&
      app.includes('trayAccountRowTop') &&
      app.includes('accountActiveBadge') &&
      !app.includes('trayAccountRow--active') &&
      app.includes('CircularProgressRing') &&
      app.includes('trayRelayButton') &&
      app.includes('trayRefreshButton') &&
      app.includes('trayResetSub') &&
      app.includes('trayQuotaTrack') &&
      app.includes('quotaDisplayWindows') &&
      app.includes('formatResetCountdown') &&
      trayPanel.includes('<PlanTypeBadge planType={quota?.planType} />') &&
      app.includes('.planTypeBadge') &&
      app.includes('font-size: 9px') &&
      app.includes('padding: 2px 5.5px') &&
      app.includes('.trayAccountRow--monthlyOnly .trayAccountRowContentLeft') &&
      app.includes('gap: 28px') &&
      app.includes('.trayAccountRow--monthlyOnly .trayAccountRowContentLeftText') &&
      app.includes('width: 160px') &&
      quotaLib.includes('primaryWindowDurationMins') &&
      quotaLib.includes('secondaryWindowDurationMins') &&
      quotaLib.includes("'Monthly'") &&
      !trayPanel.includes('<strong>5h</strong>') &&
      !trayPanel.includes('<span>weekly</span>') &&
      app.includes('Refreshing…') &&
      app.includes('latestSessionModifiedAt') &&
      trayPanel.includes('<h2>LAM</h2>') &&
      app.includes('countAccountsWithAvailableQuota') &&
      app.includes('accountsWithQuotaData') &&
      app.includes('availableQuotaAccounts') &&
      app.includes('height: 20px') &&
      app.includes('trayPopoverActions') &&
      app.includes('height: 4px') &&
      app.includes('background: linear-gradient(180deg, #15181f 0%, #0d0f12 100%), #0d0f12') &&
      !app.includes('trayProviderGroup') &&
      !app.includes('trayAccountCard') &&
      !app.includes('trayRefreshBtn') &&
      !app.includes('trayOpacityMini'),
  ],
  [
    'tray popover remains scrollable with footer visible',
    app.includes("html[data-tray-popover='1'] body") &&
      app.includes('height: auto') &&
      app.includes('.trayPopoverPanel') &&
      app.includes('max-height: 100vh') &&
      app.includes('.trayAccountList') &&
      app.includes('flex: 1 1 auto') &&
      app.includes('max-height: 700px') &&
      app.includes('overflow-y: auto') &&
      app.includes('overscroll-behavior: contain') &&
      app.includes('.trayPopoverFoot') &&
      app.includes('flex: 0 0 auto') &&
      trayPanel.includes('<footer className="trayPopoverFoot">') &&
      trayPanel.includes('Open') &&
      trayPanel.includes('Close'),
  ],
  [
    'desktop main window opens only through explicit open command',
    tauriConfig.includes('"label": "main"') &&
      tauriConfig.includes('"visible": false') &&
      tauriConfig.includes('"height": 520') &&
      tauriTray.includes('pub fn show_main_window') &&
      tauriTray.includes('window.unminimize()') &&
      !tauriTray.includes('prepare_main_window_for_tray_only(app);'),
  ],
  [
    'tray close does not open main window',
    trayPanel.includes('onClose') &&
      trayPanel.includes('onOpen') &&
      trayPanel.includes('e.preventDefault();') &&
      trayPanel.includes('e.stopPropagation();') &&
      trayPanel.includes('onOpen();') &&
      !trayPanel.includes('onPointerDown={onClosePointerDown}'),
  ],
  [
    'tray popover height grows up to four accounts',
    traySize.includes('TRAY_POPOVER_MAX_HEIGHT = 900') &&
      traySize.includes('measureTrayPopoverHeight') &&
      traySize.includes('list.scrollHeight') &&
      traySize.includes('Math.min(list.scrollHeight, 700)') &&
      !traySize.includes('panel.getBoundingClientRect().height'),
  ],
  [
    'tray quota reset text sits below progress bar',
    trayPanel.indexOf('className="trayQuotaTrack"') <
      trayPanel.indexOf('className="trayResetSub"') &&
      !trayPanel.includes(
        '<div className="trayQuotaLabel">\\n          <span>{props.label}</span>\\n          <span className="trayResetSub">',
      ),
  ],
  [
    'brand icon unified',
    app.includes('IconLogo') &&
      app.includes('LAMOrbitLogo') &&
      app.includes('lam-orbit-icon.svg') &&
      orbitIconExists &&
      iconSvg.includes('LAMLogo') &&
      iconSvg.includes('LamOrbit') &&
      iconSvg.includes('scale(0.8)') &&
      !iconSvg.includes('#000000'),
  ],
  [
    'quota cards do not overflow',
    app.includes('.accountQuota') &&
      app.includes('min-width: 0') &&
      app.includes('overflow-wrap: anywhere') &&
      app.includes('width: 100%'),
  ],
  [
    'startup accounts cache then scan',
    app.includes('listCachedAccounts') &&
      app.includes('applyAccountsList') &&
      app.includes('fromCache') &&
      app.includes('scanning…') &&
      api.includes('list_cached_accounts') &&
      tauriCommands.includes('pub async fn list_cached_accounts') &&
      tauriCore.includes('fn list_cached_accounts') &&
      tauriCore.includes('accounts-cache.json'),
  ],
  [
    'startup quota is nonblocking',
    app.includes('loadCachedQuotas') &&
      app.includes('scheduleQuotaRefresh') &&
      app.includes('getProfileQuota(profileId, true)') &&
      app.includes('mergeQuotaSnapshots') &&
      !app.includes('quotaRefreshInFlightRef') &&
      !app.includes('refreshAllQuotas(ids)') &&
      !app.includes('await refreshAllQuotas(accountData.map((account) => account.id))'),
  ],
  [
    'stale quota snapshots filtered after account rename',
    quotaLib.includes('filterQuotaSnapshotsForAccounts') &&
      quotaLib.includes('quotaRefreshProfileIds') &&
      app.includes('filterToProfileIds(quotaProfileIds)') &&
      views.includes('countAccountsWithQuotaData(accounts, quotas)') &&
      trayPanel.includes('countAccountsWithQuotaData(accounts, quotas)'),
  ],
  [
    'cached real quota startup path',
    app.includes('listCachedQuotas') &&
      app.includes('listCachedAccounts') &&
      api.includes('list_cached_quotas') &&
      api.includes("invoke<UsageQuotaSnapshot[]>('list_cached_quotas'"),
  ],
  [
    'real quota is decoupled from session estimates',
    tauriCore.includes('fn quota_account') &&
      tauriCore.includes('usage_unavailable') &&
      !tauriCore.includes('Activity estimate only; not real quota') &&
      !tauriCore.includes('Consider switching profile or creating a relay workspace.'),
  ],
  [
    'heavy tauri commands are nonblocking',
    tauriCommands.includes('async fn run_blocking') &&
      tauriCommands.includes('pub async fn list_accounts') &&
      tauriCommands.includes('pub async fn list_sessions') &&
      tauriCommands.includes('pub async fn get_profile_quota') &&
      tauriCommands.includes('pub async fn list_providers') &&
      tauriCommands.includes('spawn_blocking'),
  ],
  [
    'provider delete safety',
    app.includes('variant="danger"') &&
      app.includes('Detach Provider') &&
      app.includes('onDetach(binding)'),
  ],
  [
    'provider center',
    app.includes('Providers') &&
      app.includes('infoBanner') &&
      app.includes('Attach Provider') &&
      app.includes('Provider views contain credential references only'),
  ],
  [
    'account-first external API lifecycle',
    app.includes('Create API Account') &&
      app.includes('Advanced Provider Connections') &&
      app.includes('Switch Model') &&
      api.includes("invoke<ApiAccountPlanViewV2>('plan_api_account_v2'") &&
      api.includes("invoke<ApiAccountExecutionViewV2>('execute_api_account_v2'") &&
      api.includes("invoke<DeleteAccountResult>('delete_api_account_v2'"),
  ],
  [
    'provider center creation uses the External API account flow',
    app.includes('onAddExternalApi={openExternalApiModal}') &&
      app.includes('function openExternalApiModal()') &&
      !app.includes("setDialog({ kind: 'editor', provider: null })"),
  ],
  [
    'global header opens the External API account flow directly',
    app.includes('className="toolbarBtn" onClick={openExternalApiModal}') &&
      app.includes('<IconPlus size={14} /> External API') &&
      !app.includes('<IconPlus size={14} /> New Provider'),
  ],
  [
    'External API has a dedicated modal shell',
    app.includes("openModal('externalApi')") &&
      app.includes("modal === 'externalApi'") &&
      app.includes('<Shell.Modal title="Add External API"') &&
      app.includes('<Shell.Modal title="Add Account"') &&
      !app.includes("setCreateMode('api')") &&
      !app.includes("createMode === 'api'"),
  ],
  [
    'provider mismatch',
    app.includes('provider mismatch') &&
      app.includes('runtime behavior, cost, and tool compatibility may differ'),
  ],
  [
    'tauri invokes',
    api.includes("invoke<HealthCheck>('health_check')") &&
      !api.includes('execute_sync') &&
      api.includes('relay_resume_session') &&
      api.includes('open_terminal_with_command') &&
      api.includes('create_provider') &&
      api.includes('refresh_all_quotas'),
  ],
  [
    'relay resume entry',
    app.includes('Resume Here') &&
      app.includes('relayResumeSession') &&
      app.includes('openTerminalWithCommand') &&
      app.includes('activeSession') &&
      app.includes('refreshActiveSession') &&
      app.includes('summarize_fork_with_target_account') &&
      app.includes('Existing session was not overwritten.') &&
      tauriCommands.includes('pub fn relay_resume_session'),
  ],
  [
    'latest active relay controls',
    app.includes('Active source') &&
      app.includes('activeSessionBanner') &&
      app.includes('trayActiveSource') &&
      app.includes('trayRelayButton') &&
      app.includes('2 * 60_000'),
  ],
  [
    'diverged strategy settings',
    app.includes('Diverged session strategy') &&
      app.includes('setDivergedStrategy') &&
      app.includes('timeline_merge_to_fork') &&
      app.includes('prefer_source') &&
      app.includes('prefer_target'),
  ],
];

const failed = checks.filter(([, ok]) => !ok);
if (failed.length) {
  console.error('UI smoke failed:');
  for (const [name] of failed) console.error(`- ${name}`);
  process.exit(1);
}

console.log('UI smoke passed');
