import { invoke } from '@tauri-apps/api/core';
import type {
  CodexAccount,
  CodexSession,
  AccountNoteUpdate,
  AttachProviderRequest,
  AttachProviderResult,
  CreateAccountRequest,
  CreateProviderRequest,
  CreateRelayRequest,
  CreateResult,
  DeleteAccountRequest,
  DeleteAccountResult,
  HealthCheck,
  OperationPlan,
  ProviderProfile,
  QuotaRefreshResult,
  ResetQuotaResult,
  RelayResumeRequest,
  RelayResumeResult,
  RenameAccountRequest,
  RenameAccountResult,
  ResumeCommand,
  ResumeCommandRequest,
  SyncPlan,
  SyncRequest,
  SyncResult,
  TerminalTarget,
  UpdateProviderRequest,
  UsageQuotaSnapshot,
  AntigravityQuotaResponse,
  UploadedCredentials,
  AuthMetadata,
  TokenExpirationStatus,
  AddPatAccountRequest,
  AddPatAccountResult,
  AddSessionProfileAccountRequest,
  CpaExport,
  UsageRefreshResult,
  UsageActivityBucket,
  UsageCallRow,
  UsageDashboard,
  UsageDashboardResponse,
  UsageDashboardRequest,
  UsageDiagnostics,
  UsageInsights,
  UsagePagedResponse,
  UsageRateCardEntry,
  UsageScopesResponse,
  UsageSummary,
  UsageSummaryRequest,
  UsageThreadSummary,
  CreateProviderRequestV2,
  CreateProviderWithKeychainRequestV2,
  UpdateProviderRequestV2,
  ProviderProfileViewV2,
  PlanAttachRequestV2,
  ExecuteAttachRequestV2,
  ExecuteDetachRequestV2,
  ProfileAttachPlanViewV2,
  ProfileDetachPlanViewV2,
  ProfileProviderBindingViewV2,
  AttachExecutionViewV2,
  RotateProviderCredentialRequestV2,
  CredentialRotationViewV2,
  ProviderUpstreamTestViewV2,
  LegacyCreateProviderCompatRequestV2,
  LegacyCreateProviderResultV2,
  ApproveAuthCommandRequestV2,
  AuthCommandApprovalListV2,
  AuthCommandApprovalViewV2,
  GatewayPortChangePlanV2,
  GatewayPortMigrationOutcomeV2,
  PlanApiAccountRequestV2,
  ApiAccountPlanViewV2,
  ExecuteApiAccountRequestV2,
  ApiAccountExecutionViewV2,
  DiscoverProviderModelsRequestV2,
  DiscoverProviderModelsViewV2,
} from './types';

type TauriInternals = {
  invoke?: unknown;
};

export const inTauri = () => {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriInternals })
    .__TAURI_INTERNALS__;
  return Boolean(
    internals && typeof internals === 'object' && typeof internals.invoke === 'function',
  );
};

export async function healthCheck(): Promise<HealthCheck> {
  if (!inTauri()) {
    return { ok: false, version: 'browser-preview', homeRoot: 'not connected' };
  }
  return invoke<HealthCheck>('health_check');
}

export async function listAccounts(): Promise<CodexAccount[]> {
  if (!inTauri()) return [];
  return invoke<CodexAccount[]>('list_accounts');
}

export async function listCachedAccounts(): Promise<CodexAccount[]> {
  if (!inTauri()) return [];
  return invoke<CodexAccount[]>('list_cached_accounts');
}

export async function listSessions(accountId: string): Promise<CodexSession[]> {
  if (!inTauri()) return [];
  return invoke<CodexSession[]>('list_sessions', { accountId });
}

export async function planCreateAccount(req: CreateAccountRequest): Promise<OperationPlan> {
  return invoke<OperationPlan>('plan_create_account', { req });
}

export async function executeCreateAccount(req: CreateAccountRequest): Promise<CreateResult> {
  return invoke<CreateResult>('execute_create_account', { req });
}

export async function planRenameAccount(req: RenameAccountRequest): Promise<OperationPlan> {
  return invoke<OperationPlan>('plan_rename_account', { req });
}

export async function executeRenameAccount(
  req: RenameAccountRequest,
): Promise<RenameAccountResult> {
  return invoke<RenameAccountResult>('execute_rename_account', { req });
}

export async function deleteAccount(req: DeleteAccountRequest): Promise<DeleteAccountResult> {
  return invoke<DeleteAccountResult>('delete_account', { req });
}

export async function updateAccountNote(req: AccountNoteUpdate): Promise<CodexAccount> {
  return invoke<CodexAccount>('update_account_note', { req });
}

export async function planCreateRelay(req: CreateRelayRequest): Promise<OperationPlan> {
  return invoke<OperationPlan>('plan_create_relay', { req });
}

export async function executeCreateRelay(req: CreateRelayRequest): Promise<CreateResult> {
  return invoke<CreateResult>('execute_create_relay', { req });
}

export async function buildSyncPlan(req: SyncRequest): Promise<SyncPlan> {
  return invoke<SyncPlan>('build_sync_plan', { req });
}

export async function executeSync(req: SyncRequest): Promise<SyncResult> {
  return invoke<SyncResult>('execute_sync', { req });
}

export async function buildResumeCommand(req: ResumeCommandRequest): Promise<ResumeCommand> {
  return invoke<ResumeCommand>('build_resume_command', { req });
}

export async function openTerminalWithResume(req: ResumeCommandRequest): Promise<void> {
  return invoke<void>('open_terminal_with_resume', { req });
}

export async function openTerminalWithCommand(command: string): Promise<void> {
  return invoke<void>('open_terminal_with_command', { command });
}

export async function relayResumeSession(req: RelayResumeRequest): Promise<RelayResumeResult> {
  return invoke<RelayResumeResult>('relay_resume_session', { req });
}

export async function buildLoginCommand(profileId: string): Promise<ResumeCommand> {
  return invoke<ResumeCommand>('build_login_command', { profileId });
}

export async function openTerminalForLogin(profileId: string): Promise<void> {
  return invoke<void>('open_terminal_for_login', { profileId });
}

export async function listTerminalTargets(): Promise<TerminalTarget[]> {
  if (!inTauri())
    return [{ id: 'terminal', displayName: 'Terminal.app', kind: 'terminal', installed: true }];
  return invoke<TerminalTarget[]>('list_terminal_targets');
}

export async function getSelectedTerminalTarget(): Promise<string> {
  if (!inTauri()) return 'terminal';
  return invoke<string>('get_selected_terminal_target');
}

export async function setSelectedTerminalTarget(targetId: string): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('set_selected_terminal_target', { targetId });
}

export async function getProfileQuota(
  profileId: string,
  forceRefresh = false,
): Promise<UsageQuotaSnapshot> {
  return invoke<UsageQuotaSnapshot>('get_profile_quota', { profileId, forceRefresh });
}

export async function refreshAllQuotas(profileIds?: string[]): Promise<QuotaRefreshResult> {
  return invoke<QuotaRefreshResult>('refresh_all_quotas', { profileIds });
}

export async function resetProfileQuota(profileId: string): Promise<ResetQuotaResult> {
  return invoke<ResetQuotaResult>('reset_profile_quota', { profileId });
}

export async function listCachedQuotas(profileIds?: string[]): Promise<UsageQuotaSnapshot[]> {
  if (!inTauri()) return [];
  return invoke<UsageQuotaSnapshot[]>('list_cached_quotas', { profileIds });
}

const emptyUsageSummary = (): UsageSummary => ({
  refreshedAt: null,
  scannedFiles: 0,
  parsedEvents: 0,
  skippedEvents: 0,
  totalCalls: 0,
  totalTokens: 0,
  inputTokens: 0,
  cachedInputTokens: 0,
  uncachedInputTokens: 0,
  outputTokens: 0,
  reasoningOutputTokens: 0,
  estimatedCostUsd: 0,
  pricingCoverage: {
    pricedTokens: 0,
    unpricedTokens: 0,
    pricedTokenRatio: 0,
    unknownModels: [],
  },
  diagnostics: {
    parserDiagnostics: {},
    skippedEvents: 0,
    unknownModels: [],
    lowCacheThreads: [],
    highContextCalls: [],
    lastRefreshError: null,
  },
  headlineStats: {
    lifetimeTokens: null,
    peakDailyTokens: null,
    longestRunningTurnSec: null,
    currentStreakDays: null,
    longestStreakDays: null,
    source: 'local_sqlite',
    localTotalTokens: 0,
    codexTotalTokens: null,
    tokenDelta: null,
    tokenDeltaPercent: null,
  },
  activityBuckets: [],
  topThreads: [],
  recentCalls: [],
  insights: null,
  callsPage: null,
  threadsPage: null,
});

const emptyUsageDashboard = (): UsageDashboard => ({
  ...emptyUsageSummary(),
  scope: null,
  modelOptions: [],
  effortOptions: [],
  pricingConfidenceOptions: [],
  statusChips: [],
  investigationPresets: [],
});

const emptyUsageDashboardResponse = (): UsageDashboardResponse => ({
  scopes: [{ id: 'total', label: 'Total', kind: 'total', accountId: null, isDefault: true }],
  activeScopeId: 'total',
  dashboard: emptyUsageDashboard(),
});

export async function refreshUsageIndex(includeArchived = false): Promise<UsageRefreshResult> {
  if (!inTauri()) {
    return {
      scannedFiles: 0,
      parsedFiles: 0,
      parsedEvents: 0,
      insertedOrUpdatedEvents: 0,
      skippedEvents: 0,
      dbPath: '',
      parserDiagnostics: {},
    };
  }
  return invoke<UsageRefreshResult>('refresh_usage_index', { includeArchived });
}

export async function tryRefreshUsageIndex(
  includeArchived = false,
): Promise<UsageRefreshResult | null> {
  if (!inTauri()) return null;
  return invoke<UsageRefreshResult | null>('try_refresh_usage_index', { includeArchived });
}

export async function refreshAccountUsageSnapshot(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('refresh_account_usage_snapshot');
}

export async function getUsageSummary(req: UsageSummaryRequest): Promise<UsageSummary> {
  if (!inTauri()) return emptyUsageSummary();
  return invoke<UsageSummary>('get_usage_summary', { req });
}

export async function getUsageDashboard(req: UsageDashboardRequest): Promise<UsageDashboard> {
  if (!inTauri()) return emptyUsageDashboard();
  return invoke<UsageDashboard>('get_usage_dashboard', { req });
}

export async function getUsageDashboardResponse(
  req: UsageDashboardRequest,
): Promise<UsageDashboardResponse> {
  if (!inTauri()) return emptyUsageDashboardResponse();
  return invoke<UsageDashboardResponse>('get_usage_dashboard_response', { req });
}

export async function getUsageScopes(req: UsageDashboardRequest): Promise<UsageScopesResponse> {
  if (!inTauri()) {
    const response = emptyUsageDashboardResponse();
    return { scopes: response.scopes, activeScopeId: response.activeScopeId };
  }
  return invoke<UsageScopesResponse>('get_usage_scopes', { req });
}

export async function getUsageOverview(req: UsageDashboardRequest): Promise<UsageDashboard> {
  if (!inTauri()) return emptyUsageDashboard();
  return invoke<UsageDashboard>('get_usage_overview', { req });
}

export async function getUsageActivity(req: UsageDashboardRequest): Promise<UsageActivityBucket[]> {
  if (!inTauri()) return [];
  return invoke<UsageActivityBucket[]>('get_usage_activity', { req });
}

export async function getUsageInsights(req: UsageDashboardRequest): Promise<UsageInsights> {
  if (!inTauri()) {
    return {
      fastModePercent: null,
      mostUsedReasoning: null,
      mostUsedReasoningPercent: null,
      skillsExplored: 0,
      totalSkillsUsed: 0,
      totalThreads: 0,
    };
  }
  return invoke<UsageInsights>('get_usage_insights', { req });
}

export async function getUsageCalls(
  req: UsageDashboardRequest,
): Promise<UsagePagedResponse<UsageCallRow>> {
  if (!inTauri()) return { rows: [], total: 0, limit: req.limit ?? 0, offset: req.offset ?? 0 };
  return invoke<UsagePagedResponse<UsageCallRow>>('get_usage_calls', { req });
}

export async function getUsageThreads(
  req: UsageDashboardRequest,
): Promise<UsagePagedResponse<UsageThreadSummary>> {
  if (!inTauri()) return { rows: [], total: 0, limit: req.limit ?? 0, offset: req.offset ?? 0 };
  return invoke<UsagePagedResponse<UsageThreadSummary>>('get_usage_threads', { req });
}

export async function getUsageRateCard(): Promise<UsageRateCardEntry[]> {
  if (!inTauri()) return [];
  return invoke<UsageRateCardEntry[]>('get_usage_rate_card');
}

export async function getUsageDiagnostics(req: UsageDashboardRequest): Promise<UsageDiagnostics> {
  if (!inTauri()) return emptyUsageSummary().diagnostics;
  return invoke<UsageDiagnostics>('get_usage_diagnostics', { req });
}

export async function resetUsageIndex(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('reset_usage_index');
}

export async function compactUsageDb(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('compact_usage_db');
}

export async function syncTrayQuota(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('sync_tray_quota');
}

export async function showUsageStats(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('show_usage_stats');
}

export interface CallRawContents {
  request: string;
  assistant: string;
  toolOutput: string;
}

export async function getCallRawContents(
  sourceFile: string,
  lineNumber: number,
): Promise<CallRawContents> {
  if (!inTauri()) return { request: '', assistant: '', toolOutput: '' };
  return invoke<CallRawContents>('get_call_raw_contents', { sourceFile, lineNumber });
}

export async function takePendingRoute(): Promise<string | null> {
  if (!inTauri()) return null;
  return invoke<string | null>('take_pending_route');
}

export async function setQuotaPopoverOpacity(percent: number): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('set_quota_popover_opacity', { percent });
}

export async function hideQuotaPopover(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('hide_quota_popover');
}

export async function listProviders(): Promise<ProviderProfile[]> {
  if (!inTauri()) return [];
  return invoke<ProviderProfile[]>('list_providers');
}

export async function createProvider(req: CreateProviderRequest): Promise<ProviderProfile> {
  return invoke<ProviderProfile>('create_provider', { req });
}

export async function updateProvider(req: UpdateProviderRequest): Promise<ProviderProfile> {
  return invoke<ProviderProfile>('update_provider', { req });
}

export async function deleteProvider(providerId: string): Promise<boolean> {
  return invoke<boolean>('delete_provider', { providerId });
}

export async function testProvider(providerId: string): Promise<ProviderProfile> {
  return invoke<ProviderProfile>('test_provider', { providerId });
}

export async function planAttachProviderToProfile(
  req: AttachProviderRequest,
): Promise<OperationPlan> {
  return invoke<OperationPlan>('plan_attach_provider_to_profile', { req });
}

export async function attachProviderToProfile(
  req: AttachProviderRequest,
): Promise<AttachProviderResult> {
  return invoke<AttachProviderResult>('attach_provider_to_profile', { req });
}

export async function listProvidersV2(): Promise<ProviderProfileViewV2[]> {
  return invoke<ProviderProfileViewV2[]>('list_providers_v2');
}

export async function createProviderV2(
  req: CreateProviderRequestV2,
): Promise<ProviderProfileViewV2> {
  return invoke<ProviderProfileViewV2>('create_provider_v2', { req });
}

export async function discoverProviderModelsV2(
  req: DiscoverProviderModelsRequestV2,
): Promise<DiscoverProviderModelsViewV2> {
  return invoke<DiscoverProviderModelsViewV2>('discover_provider_models_v2', { req });
}

export async function listProviderAuthCommandApprovalsV2(): Promise<AuthCommandApprovalListV2> {
  return invoke<AuthCommandApprovalListV2>('list_provider_auth_command_approvals_v2');
}

export async function approveProviderAuthCommandV2(
  req: ApproveAuthCommandRequestV2,
): Promise<AuthCommandApprovalViewV2> {
  return invoke<AuthCommandApprovalViewV2>('approve_provider_auth_command_v2', { req });
}

export async function createProviderWithKeychainV2(
  req: CreateProviderWithKeychainRequestV2,
): Promise<ProviderProfileViewV2> {
  return invoke<ProviderProfileViewV2>('create_provider_with_keychain_v2', { req });
}

export async function createProviderLegacyCompatV2(
  req: LegacyCreateProviderCompatRequestV2,
  expectedRevision: number,
): Promise<LegacyCreateProviderResultV2> {
  return invoke<LegacyCreateProviderResultV2>('create_provider_legacy_compat_v2', {
    req,
    expectedRevision,
  });
}

export async function updateProviderV2(
  req: UpdateProviderRequestV2,
): Promise<ProviderProfileViewV2> {
  return invoke<ProviderProfileViewV2>('update_provider_v2', { req });
}

export async function rotateProviderCredentialV2(
  req: RotateProviderCredentialRequestV2,
): Promise<CredentialRotationViewV2> {
  return invoke<CredentialRotationViewV2>('rotate_provider_credential_v2', { req });
}

export async function testProviderUpstreamV2(
  providerId: string,
): Promise<ProviderUpstreamTestViewV2> {
  return invoke<ProviderUpstreamTestViewV2>('test_provider_upstream_v2', { providerId });
}

export async function listProfileProviderBindingsV2(): Promise<ProfileProviderBindingViewV2[]> {
  return invoke<ProfileProviderBindingViewV2[]>('list_profile_provider_bindings_v2');
}

export async function planAttachProviderV2(
  req: PlanAttachRequestV2,
): Promise<ProfileAttachPlanViewV2> {
  return invoke<ProfileAttachPlanViewV2>('plan_attach_provider_v2', { req });
}

export async function executeAttachProviderV2(
  req: ExecuteAttachRequestV2,
): Promise<AttachExecutionViewV2> {
  return invoke<AttachExecutionViewV2>('execute_attach_provider_v2', { req });
}

export async function planDetachProviderV2(profileId: string): Promise<ProfileDetachPlanViewV2> {
  return invoke<ProfileDetachPlanViewV2>('plan_detach_provider_v2', { profileId });
}

export async function executeDetachProviderV2(
  req: ExecuteDetachRequestV2,
): Promise<AttachExecutionViewV2> {
  return invoke<AttachExecutionViewV2>('execute_detach_provider_v2', { req });
}

export async function planApiAccountV2(
  req: PlanApiAccountRequestV2,
): Promise<ApiAccountPlanViewV2> {
  return invoke<ApiAccountPlanViewV2>('plan_api_account_v2', { req });
}

export async function executeApiAccountV2(
  req: ExecuteApiAccountRequestV2,
): Promise<ApiAccountExecutionViewV2> {
  return invoke<ApiAccountExecutionViewV2>('execute_api_account_v2', { req });
}

export async function planApiAccountModelSwitchV2(
  profileId: string,
  selectedModel: string,
): Promise<ProfileAttachPlanViewV2> {
  return invoke<ProfileAttachPlanViewV2>('plan_api_account_model_switch_v2', {
    profileId,
    selectedModel,
  });
}

export async function executeApiAccountModelSwitchV2(
  planId: string,
  fingerprint: string,
): Promise<AttachExecutionViewV2> {
  return invoke<AttachExecutionViewV2>('execute_api_account_model_switch_v2', {
    planId,
    fingerprint,
  });
}

export async function deleteApiAccountV2(profileId: string): Promise<DeleteAccountResult> {
  return invoke<DeleteAccountResult>('delete_api_account_v2', { profileId });
}

export async function planGatewayPortMigrationV2(
  newPort: number,
): Promise<GatewayPortChangePlanV2> {
  return invoke<GatewayPortChangePlanV2>('plan_gateway_port_migration_v2', { newPort });
}

export async function executeGatewayPortMigrationV2(
  plan: GatewayPortChangePlanV2,
): Promise<GatewayPortMigrationOutcomeV2> {
  return invoke<GatewayPortMigrationOutcomeV2>('execute_gateway_port_migration_v2', {
    plan,
    fingerprint: plan.fingerprint,
  });
}

export async function getAntigravityQuota(): Promise<AntigravityQuotaResponse> {
  if (!inTauri()) {
    return { ok: false, models: [], error: 'Not in Tauri environment' };
  }
  return invoke<AntigravityQuotaResponse>('get_antigravity_quota');
}

export async function uploadPatCredentials(
  profileId: string,
  uploaded: UploadedCredentials,
): Promise<void> {
  return invoke<void>('upload_pat_credentials', { profileId, uploaded });
}

export async function getPatMetadata(profileId: string): Promise<AuthMetadata | null> {
  return invoke<AuthMetadata | null>('get_pat_metadata', { profileId });
}

export async function checkProfileTokenExpiration(
  profileId: string,
): Promise<TokenExpirationStatus> {
  return invoke<TokenExpirationStatus>('check_profile_token_expiration', { profileId });
}

export async function addPatAccount(req: AddPatAccountRequest): Promise<AddPatAccountResult> {
  return invoke<AddPatAccountResult>('add_pat_account', { req });
}

export async function addSessionProfileAccount(
  req: AddSessionProfileAccountRequest,
): Promise<CreateResult> {
  return invoke<CreateResult>('add_session_profile_account', { req });
}

export async function switchToPatAccount(accountId: string): Promise<void> {
  return invoke<void>('switch_to_pat_account', { accountId });
}

export async function exportCpaCredentials(profileId: string): Promise<CpaExport> {
  return invoke<CpaExport>('export_cpa_credentials', { profileId });
}

export async function updatePatSessionAuth(
  profileId: string,
  authJson: Record<string, unknown>,
): Promise<void> {
  return invoke<void>('update_pat_session_auth', { profileId, authJson });
}

export async function getAuthMode(): Promise<string> {
  if (!inTauri()) return 'oauth';
  return invoke<string>('get_auth_mode');
}

export async function setAuthMode(mode: string): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('set_auth_mode', { mode });
}

export async function getHideDockIcon(): Promise<boolean> {
  if (!inTauri()) return false;
  return invoke<boolean>('get_hide_dock_icon');
}

export async function setHideDockIcon(hide: boolean): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('set_hide_dock_icon', { hide });
}

export async function restartChatgpt(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>('restart_chatgpt');
}
