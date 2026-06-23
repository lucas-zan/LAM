export type AppError = {
  code: string;
  message: string;
  recoverable: boolean;
  details?: unknown;
};

export type HealthCheck = {
  ok: boolean;
  version: string;
  homeRoot: string;
};

export type CodexAccount = {
  id: string;
  displayName: string;
  codexHome: string;
  wrapperPath?: string | null;
  hasAuth: boolean;
  hasConfig: boolean;
  hasHistory: boolean;
  sessionCount: number;
  latestSessionModifiedAt?: number | null;
  managed: boolean;
  isRelay: boolean;
  relaySource?: string | null;
  relayIdentity?: string | null;
  providerId?: string | null;
  model?: string | null;
  authMode?: string | null;
  renewalDate?: string | null;
  note?: string | null;
};

export type CodexSession = {
  id: string;
  accountId: string;
  path: string;
  modifiedAt: number;
  sizeBytes: number;
  cwd?: string | null;
  threadName?: string | null;
  summary?: string | null;
  firstUserMessage?: string | null;
  model?: string | null;
  originalProviderId?: string | null;
  originalModel?: string | null;
  currentProviderId?: string | null;
  currentModel?: string | null;
  providerMismatch: boolean;
};

export type OperationPlan = {
  operations: string[];
  warnings: string[];
  blocked: string[];
};

export type CreateAccountRequest = {
  name: string;
  copyConfigFrom?: string | null;
  overwriteWrapper: boolean;
};

export type RenameAccountRequest = {
  fromProfileId: string;
  toName: string;
  overwriteWrapper: boolean;
};

export type CreateRelayRequest = {
  runtimeProfileId: string;
  sourceProfileId: string;
  name?: string | null;
  providerPolicy: string;
  overwriteWrapper: boolean;
};

export type CreateResult = {
  profileId: string;
  homePath: string;
  wrapperPath: string;
  operations: string[];
  warnings: string[];
};

export type RenameAccountResult = {
  profileId: string;
  previousProfileId: string;
  homePath: string;
  previousHomePath: string;
  wrapperPath: string;
  previousWrapperPath: string;
  operations: string[];
  warnings: string[];
};

export type AccountNoteUpdate = {
  profileId: string;
  renewalDate?: string | null;
  note?: string | null;
};

export type SyncRequest = {
  fromProfileId: string;
  toProfileId: string;
  syncSessions: boolean;
  backupTargetSessions: boolean;
  sidecarBackupHistory: boolean;
};

export type SyncPlan = {
  fromProfileId: string;
  toProfileId: string;
  operations: Array<{
    kind: string;
    from?: string | null;
    to?: string | null;
    rel?: string | null;
  }>;
  warnings: string[];
  blockedFiles: string[];
  policyBlockedFiles: string[];
};

export type SyncResult = {
  copied: number;
  skipped: number;
  backupPath?: string | null;
  manifestPath: string;
  warnings: string[];
};

export type ResumeCommandRequest = {
  profileId: string;
  sessionId?: string | null;
  cwd?: string | null;
};

export type ResumeCommand = {
  command: string;
  sideEffects: string[];
};

export type RelayResumeRequest = {
  fromProfileId: string;
  toProfileId: string;
  sessionId: string;
  cwd?: string | null;
  divergedStrategy?: DivergedSessionStrategy | null;
};

export type RelayResumeResult = {
  action: string;
  fromProfileId: string;
  toProfileId: string;
  sessionId: string;
  sourcePath: string;
  targetPath: string;
  backupPath?: string | null;
  forkPath?: string | null;
  handoffPath?: string | null;
  resume: ResumeCommand;
  warnings: string[];
};

export type DivergedSessionStrategy =
  | 'stop_and_ask'
  | 'summarize_fork_with_target_account'
  | 'timeline_merge_to_fork'
  | 'prefer_source'
  | 'prefer_target';

export type UsageQuotaSnapshot = {
  profileId: string;
  source: string;
  fetchedAt: number;
  staleness: string;
  planType?: string | null;
  activityTokens?: number | null;
  primaryUsedPercent?: number | null;
  primaryWindowDurationMins?: number | null;
  secondaryUsedPercent?: number | null;
  secondaryWindowDurationMins?: number | null;
  remainingPercent?: number | null;
  resetAt?: string | null;
  secondaryResetAt?: string | null;
  alerts: string[];
  suggestedActions: string[];
};

export type QuotaRefreshResult = {
  snapshots: UsageQuotaSnapshot[];
  warnings: string[];
};

export type ProviderProfile = {
  id: string;
  name: string;
  baseUrl: string;
  wireApi: string;
  defaultModel: string;
  envKey?: string | null;
  secretStorage: string;
  health: string;
};

export type SecretInput =
  | { kind: 'env'; envKey: string }
  | { kind: 'keychain'; secret: string }
  | { kind: 'none' };

export type CreateProviderRequest = {
  id: string;
  name: string;
  baseUrl: string;
  wireApi: string;
  defaultModel: string;
  envKey?: string | null;
  secret?: SecretInput | null;
};

export type UpdateProviderRequest = CreateProviderRequest;

export type AttachProviderRequest = {
  profileId: string;
  providerId: string;
  model?: string | null;
};

export type AttachProviderResult = {
  profileId: string;
  providerId: string;
  configPath: string;
  backupPath: string;
  operations: string[];
  warnings: string[];
};

export type AntigravityModelQuota = {
  label: string;
  remainingFraction?: number | null;
  resetTime?: string | null;
};

export type AntigravityQuotaResponse = {
  ok: boolean;
  models: AntigravityModelQuota[];
  error?: string | null;
};
