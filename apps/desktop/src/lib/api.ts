import { invoke } from "@tauri-apps/api/core";
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
  HealthCheck,
  OperationPlan,
  ProviderProfile,
  QuotaRefreshResult,
  RelayResumeRequest,
  RelayResumeResult,
  RenameAccountRequest,
  RenameAccountResult,
  ResumeCommand,
  ResumeCommandRequest,
  SyncPlan,
  SyncRequest,
  SyncResult,
  UpdateProviderRequest,
  UsageQuotaSnapshot,
  AntigravityQuotaResponse,
  UploadedCredentials,
  AuthMetadata,
  TokenExpirationStatus,
  AddPatAccountRequest,
  AddPatAccountResult,
} from "./types";

export const inTauri = () => "__TAURI_INTERNALS__" in window;

export async function healthCheck(): Promise<HealthCheck> {
  if (!inTauri()) {
    return { ok: false, version: "browser-preview", homeRoot: "not connected" };
  }
  return invoke<HealthCheck>("health_check");
}

export async function listAccounts(): Promise<CodexAccount[]> {
  if (!inTauri()) return [];
  return invoke<CodexAccount[]>("list_accounts");
}

export async function listCachedAccounts(): Promise<CodexAccount[]> {
  if (!inTauri()) return [];
  return invoke<CodexAccount[]>("list_cached_accounts");
}

export async function listSessions(accountId: string): Promise<CodexSession[]> {
  if (!inTauri()) return [];
  return invoke<CodexSession[]>("list_sessions", { accountId });
}

export async function planCreateAccount(req: CreateAccountRequest): Promise<OperationPlan> {
  return invoke<OperationPlan>("plan_create_account", { req });
}

export async function executeCreateAccount(req: CreateAccountRequest): Promise<CreateResult> {
  return invoke<CreateResult>("execute_create_account", { req });
}

export async function planRenameAccount(req: RenameAccountRequest): Promise<OperationPlan> {
  return invoke<OperationPlan>("plan_rename_account", { req });
}

export async function executeRenameAccount(req: RenameAccountRequest): Promise<RenameAccountResult> {
  return invoke<RenameAccountResult>("execute_rename_account", { req });
}

export async function updateAccountNote(req: AccountNoteUpdate): Promise<CodexAccount> {
  return invoke<CodexAccount>("update_account_note", { req });
}

export async function planCreateRelay(req: CreateRelayRequest): Promise<OperationPlan> {
  return invoke<OperationPlan>("plan_create_relay", { req });
}

export async function executeCreateRelay(req: CreateRelayRequest): Promise<CreateResult> {
  return invoke<CreateResult>("execute_create_relay", { req });
}

export async function buildSyncPlan(req: SyncRequest): Promise<SyncPlan> {
  return invoke<SyncPlan>("build_sync_plan", { req });
}

export async function executeSync(req: SyncRequest): Promise<SyncResult> {
  return invoke<SyncResult>("execute_sync", { req });
}

export async function buildResumeCommand(req: ResumeCommandRequest): Promise<ResumeCommand> {
  return invoke<ResumeCommand>("build_resume_command", { req });
}

export async function openTerminalWithResume(req: ResumeCommandRequest): Promise<void> {
  return invoke<void>("open_terminal_with_resume", { req });
}

export async function openTerminalWithCommand(command: string): Promise<void> {
  return invoke<void>("open_terminal_with_command", { command });
}

export async function relayResumeSession(req: RelayResumeRequest): Promise<RelayResumeResult> {
  return invoke<RelayResumeResult>("relay_resume_session", { req });
}

export async function buildLoginCommand(profileId: string): Promise<ResumeCommand> {
  return invoke<ResumeCommand>("build_login_command", { profileId });
}

export async function openTerminalForLogin(profileId: string): Promise<void> {
  return invoke<void>("open_terminal_for_login", { profileId });
}

export async function getProfileQuota(profileId: string, forceRefresh = false): Promise<UsageQuotaSnapshot> {
  return invoke<UsageQuotaSnapshot>("get_profile_quota", { profileId, forceRefresh });
}

export async function refreshAllQuotas(profileIds?: string[]): Promise<QuotaRefreshResult> {
  return invoke<QuotaRefreshResult>("refresh_all_quotas", { profileIds });
}

export async function listCachedQuotas(profileIds?: string[]): Promise<UsageQuotaSnapshot[]> {
  if (!inTauri()) return [];
  return invoke<UsageQuotaSnapshot[]>("list_cached_quotas", { profileIds });
}

export async function syncTrayQuota(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("sync_tray_quota");
}

export async function setQuotaPopoverOpacity(percent: number): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("set_quota_popover_opacity", { percent });
}

export async function hideQuotaPopover(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("hide_quota_popover");
}

export async function listProviders(): Promise<ProviderProfile[]> {
  if (!inTauri()) return [];
  return invoke<ProviderProfile[]>("list_providers");
}

export async function createProvider(req: CreateProviderRequest): Promise<ProviderProfile> {
  return invoke<ProviderProfile>("create_provider", { req });
}

export async function updateProvider(req: UpdateProviderRequest): Promise<ProviderProfile> {
  return invoke<ProviderProfile>("update_provider", { req });
}

export async function deleteProvider(providerId: string): Promise<boolean> {
  return invoke<boolean>("delete_provider", { providerId });
}

export async function testProvider(providerId: string): Promise<ProviderProfile> {
  return invoke<ProviderProfile>("test_provider", { providerId });
}

export async function planAttachProviderToProfile(req: AttachProviderRequest): Promise<OperationPlan> {
  return invoke<OperationPlan>("plan_attach_provider_to_profile", { req });
}

export async function attachProviderToProfile(req: AttachProviderRequest): Promise<AttachProviderResult> {
  return invoke<AttachProviderResult>("attach_provider_to_profile", { req });
}

export async function getAntigravityQuota(): Promise<AntigravityQuotaResponse> {
  if (!inTauri()) {
    return { ok: false, models: [], error: "Not in Tauri environment" };
  }
  return invoke<AntigravityQuotaResponse>("get_antigravity_quota");
}

export async function uploadPatCredentials(
  profileId: string,
  uploaded: UploadedCredentials
): Promise<void> {
  return invoke<void>("upload_pat_credentials", { profileId, uploaded });
}

export async function getPatMetadata(profileId: string): Promise<AuthMetadata | null> {
  return invoke<AuthMetadata | null>("get_pat_metadata", { profileId });
}

export async function checkProfileTokenExpiration(
  profileId: string
): Promise<TokenExpirationStatus> {
  return invoke<TokenExpirationStatus>("check_profile_token_expiration", { profileId });
}

export async function addPatAccount(
  req: AddPatAccountRequest
): Promise<AddPatAccountResult> {
  return invoke<AddPatAccountResult>("add_pat_account", { req });
}

export async function switchToPatAccount(accountId: string): Promise<void> {
  return invoke<void>("switch_to_pat_account", { accountId });
}

export async function getAuthMode(): Promise<string> {
  if (!inTauri()) return "oauth";
  return invoke<string>("get_auth_mode");
}

export async function setAuthMode(mode: string): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("set_auth_mode", { mode });
}

export async function getHideDockIcon(): Promise<boolean> {
  if (!inTauri()) return false;
  return invoke<boolean>("get_hide_dock_icon");
}

export async function setHideDockIcon(hide: boolean): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("set_hide_dock_icon", { hide });
}
