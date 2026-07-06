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
  isActiveAuth?: boolean;
  hasPersonalAccessToken?: boolean;
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

export type DeleteAccountRequest = {
  profileId: string;
};

export type DeleteAccountResult = {
  profileId: string;
  removedHomePath: string;
  removedWrapperPath?: string | null;
};

export type AccountNoteUpdate = {
  profileId: string;
  renewalDate?: string | null;
  note?: string | null;
};

export type CpaExport = {
  fileName: string;
  content: Record<string, unknown>;
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

export type TerminalTarget = {
  id: string;
  displayName: string;
  kind: 'terminal' | 'app' | string;
  installed: boolean;
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
  resetCreditCount?: number | null;
  resetCreditExpiresAt?: string | null;
  resetCreditExpirySource?: 'api' | 'manual_config' | 'unknown' | null;
  resetCreditDetails?: ResetCreditDetail[];
  resetCreditDetailStatus?: 'available' | 'unsupported' | 'unavailable' | 'disabled' | null;
  resetCreditDetailError?: string | null;
  alerts: string[];
  suggestedActions: string[];
};

export type ResetCreditDetail = {
  id?: string | null;
  status?: string | null;
  expiresAt?: string | null;
  source: 'api' | 'manual_config';
};

export type QuotaRefreshResult = {
  snapshots: UsageQuotaSnapshot[];
  warnings: string[];
};

export type ResetQuotaResult = {
  snapshot: UsageQuotaSnapshot;
  outcome: string;
  operationId: string;
};

export type UsageRefreshResult = {
  scannedFiles: number;
  parsedFiles: number;
  parsedEvents: number;
  insertedOrUpdatedEvents: number;
  skippedEvents: number;
  dbPath: string;
  parserDiagnostics: Record<string, number>;
};

export type UsageWindowPreset = 'all' | 'today' | 'this-week' | 'last-7-days' | 'this-month' | 'custom';

export type UsageWindow = {
  preset: UsageWindowPreset;
  from?: string | null;
  to?: string | null;
};

export type UsageSummaryRequest = {
  window: UsageWindow;
  includeArchived: boolean;
  scopeId?: string | null;
  accountId?: string | null;
};

export type UsageDashboardRequest = UsageSummaryRequest & {
  search?: string | null;
  model?: string | null;
  effort?: string | null;
  pricingConfidence?: string | null;
  sortKey?: string | null;
  sortDirection?: 'asc' | 'desc' | null;
  limit?: number | null;
  offset?: number | null;
};

export type UsageSection =
  | 'scopes'
  | 'overview'
  | 'activity'
  | 'insights'
  | 'calls'
  | 'threads'
  | 'diagnostics';

export type UsageSectionLoading = Record<UsageSection, boolean>;

export type UsagePricingCoverage = {
  pricedTokens: number;
  unpricedTokens: number;
  pricedTokenRatio: number;
  unknownModels: string[];
};

export type UsageDiagnostics = {
  parserDiagnostics: Record<string, number>;
  skippedEvents: number;
  unknownModels: string[];
  lowCacheThreads: UsageThreadSummary[];
  highContextCalls: UsageCallRow[];
  lastRefreshError?: string | null;
};

export type UsageHeadlineStats = {
  lifetimeTokens?: number | null;
  peakDailyTokens?: number | null;
  longestRunningTurnSec?: number | null;
  currentStreakDays?: number | null;
  longestStreakDays?: number | null;
  source: string;
  localTotalTokens: number;
  codexTotalTokens?: number | null;
  tokenDelta?: number | null;
  tokenDeltaPercent?: number | null;
};

export type UsageActivityBucket = {
  date: string;
  calls: number;
  tokens: number;
  cumulativeCalls: number;
  cumulativeTokens: number;
};

export type UsageInsights = {
  fastModePercent?: number | null;
  mostUsedReasoning?: string | null;
  mostUsedReasoningPercent?: number | null;
  skillsExplored: number;
  totalSkillsUsed: number;
  totalThreads: number;
};

export type UsagePagedResponse<T> = {
  rows: T[];
  total: number;
  limit: number;
  offset: number;
  nextOffset?: number | null;
};

export type UsageRateCardEntry = {
  model: string;
  pricingModel: string;
  contextWindow: string;
  estimated: boolean;
  inputPerMillion: number;
  cachedInputPerMillion: number;
  outputPerMillion: number;
  notes?: string | null;
};

export type UsageSummary = {
  refreshedAt?: string | null;
  scannedFiles: number;
  parsedEvents: number;
  skippedEvents: number;
  totalCalls: number;
  totalTokens: number;
  inputTokens: number;
  cachedInputTokens: number;
  uncachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
  estimatedCostUsd: number;
  pricingCoverage: UsagePricingCoverage;
  diagnostics: UsageDiagnostics;
  headlineStats?: UsageHeadlineStats;
  activityBuckets?: UsageActivityBucket[];
  topThreads: UsageThreadSummary[];
  recentCalls: UsageCallRow[];
  insights?: UsageInsights | null;
  callsPage?: UsagePagedResponse<UsageCallRow> | null;
  threadsPage?: UsagePagedResponse<UsageThreadSummary> | null;
};

export type UsageDashboard = UsageSummary & {
  scope?: UsageScope | null;
  modelOptions: string[];
  effortOptions: string[];
  pricingConfidenceOptions: string[];
  statusChips: Array<{ label: string; value: string }>;
  investigationPresets: Array<{ id: string; label: string; description: string }>;
};

export type UsageScopeKind = 'total' | 'workspace' | 'account';

export type UsageScope = {
  id: string;
  label: string;
  kind: UsageScopeKind;
  accountId?: string | null;
  isDefault: boolean;
};

export type UsageDashboardResponse = {
  scopes: UsageScope[];
  activeScopeId: string;
  dashboard: UsageDashboard;
};

export type UsageScopesResponse = {
  scopes: UsageScope[];
  activeScopeId: string;
};

export type UsageThreadSummary = {
  threadKey: string;
  isArchivedScope?: boolean;
  threadLabel: string;
  firstEventTimestamp?: string | null;
  callCount: number;
  sessionCount?: number;
  totalTokens: number;
  inputTokens: number;
  cachedInputTokens: number;
  uncachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens?: number;
  latestEventTimestamp?: string | null;
  avgCacheRatio?: number;
  maxContextWindowPercent?: number | null;
  maxRecommendationScore?: number;
  primaryRecommendation?: string | null;
  callInitiatorSummary?: string | null;
  archivedCallCount?: number;
  updatedAt?: string | null;
  usageCredits?: number;
  cacheRatio: number;
  estimatedCostUsd?: number;
  isArchived?: boolean;
};

export type UsageCallRow = {
  recordId: string;
  sessionId: string;
  threadName?: string | null;
  sessionUpdatedAt?: string | null;
  eventTimestamp: string;
  sourceFile: string;
  workspaceId?: string | null;
  workspaceLabel?: string | null;
  workspaceHome?: string | null;
  attributedAccountId?: string | null;
  attributedAccountLabel?: string | null;
  attributionSource?: string | null;
  lineNumber: number;
  turnId?: string | null;
  turnTimestamp?: string | null;
  cwd?: string | null;
  model?: string | null;
  effort?: string | null;
  currentDate?: string | null;
  timezone?: string | null;
  callInitiator?: string | null;
  callInitiatorReason?: string | null;
  callInitiatorConfidence?: number | null;
  inputTokens: number;
  cachedInputTokens: number;
  uncachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
  totalTokens: number;
  cumulativeTotalTokens: number;
  cacheRatio: number;
  isArchived?: boolean;
  threadKey?: string | null;
  threadCallIndex?: number | null;
  previousRecordId?: string | null;
  nextRecordId?: string | null;
  threadSource?: string | null;
  subagentType?: string | null;
  agentRole?: string | null;
  agentNickname?: string | null;
  parentSessionId?: string | null;
  parentThreadName?: string | null;
  parentSessionUpdatedAt?: string | null;
  modelContextWindow?: number | null;
  contextWindowPercent?: number | null;
  rateLimitPlanType?: string | null;
  rateLimitLimitId?: string | null;
  rateLimitPrimaryUsedPercent?: number | null;
  rateLimitPrimaryWindowMinutes?: number | null;
  rateLimitPrimaryResetsAt?: string | null;
  rateLimitSecondaryUsedPercent?: number | null;
  rateLimitSecondaryWindowMinutes?: number | null;
  rateLimitSecondaryResetsAt?: string | null;
  reasoningOutputRatio?: number;
  usageCredits?: number;
  estimatedCostUsd?: number;
  pricingModel?: string | null;
  pricingEstimated?: boolean;
  pricingConfidence?: string;
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

export type AntigravityQuotaBucket = {
  bucketId?: string | null;
  displayName: string;
  description?: string | null;
  window?: string | null;
  remainingFraction?: number | null;
  resetTime?: string | null;
  disabled?: boolean | null;
};

export type AntigravityQuotaGroup = {
  displayName: string;
  description?: string | null;
  buckets: AntigravityQuotaBucket[];
};

export type AntigravityQuotaResponse = {
  ok: boolean;
  models: AntigravityModelQuota[];
  description?: string | null;
  groups?: AntigravityQuotaGroup[];
  error?: string | null;
};

export type UploadedCredentials = {
  accessToken: string;
  accountId: string;
  disabled: boolean;
  email: string;
  expired: string; // ISO 8601
  headers?: Record<string, unknown> | null;
  idToken?: string | null;
  lastRefresh: string; // ISO 8601
  refreshToken?: string | null;
  type: string;
  websockets: boolean;
  rawAuthJson?: Record<string, unknown> | null;
};

export type AuthMetadata = {
  profileId: string;
  authType: string; // "personal_token" | "oauth" | "api_key" | "uploaded"
  tokenExpiration?: string | null; // ISO 8601
  lastChecked: string; // ISO 8601
};

export type TokenExpirationStatus = {
  profileId: string;
  isExpired: boolean;
  daysUntilExpiration?: number | null;
  expirationDate?: string | null;
  warningLevel: string; // "ok" | "warning" | "critical" | "expired"
};

export type AddPatAccountRequest = {
  accountId: string;
  authJson: Record<string, unknown>;
  personalAccessToken?: string | null;
  tokenExpiration?: string | null;
};

export type AddSessionProfileAccountRequest = {
  accountId: string;
  sessionJson: Record<string, unknown>;
  overwriteWrapper: boolean;
};

export type AddPatAccountResult = {
  accountId: string;
  email: string;
  expired: string;
};
