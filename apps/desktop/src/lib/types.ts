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
  deletable?: boolean;
  deletionProtectionReason?: string | null;
};

export type SessionPageCursor = {
  modifiedAt: number;
  sizeBytes?: number | null;
  path: string;
};

export type SessionSort = 'newest' | 'largest' | 'smallest';
export type SessionAgeFilter = 'all' | 'last7Days' | 'last30Days' | 'olderThan30Days';
export type CodexLaunchPermissionPreset = 'askForApproval' | 'approveForMe' | 'fullAccess';

export type SessionPageRequest = {
  limit?: number | null;
  cursor?: SessionPageCursor | null;
};

export type SessionQueryRequest = SessionPageRequest & {
  sort?: SessionSort | null;
  age?: SessionAgeFilter | null;
};

export type SessionSelectionRequest = {
  age?: SessionAgeFilter | null;
  query?: string | null;
};

export type SessionPage = {
  items: CodexSession[];
  nextCursor?: SessionPageCursor | null;
};

export type SessionStorageSummary = {
  evaluatedAt: number;
  activeCount: number;
  activeBytes: number;
  eligibleCount: number;
  eligibleBytes: number;
  retainedRecentCount: number;
  minimumAgeDays: number;
};

export type DeleteSessionsRequest = {
  profileId: string;
  paths: string[];
};

export type DeleteSessionsResult = {
  deletedCount: number;
  deletedBytes: number;
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
  confirmCompatibilityLoss?: boolean;
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
  compatibility?: {
    disposition: 'compatible' | 'compatible_with_loss' | 'blocked';
    confirmationRequired: boolean;
    issues: Array<{
      itemId: string;
      itemType: string;
      reason: string;
      recoveryAction: string;
    }>;
    transformations: Array<{
      itemId: string;
      itemType: string;
      action: string;
      warning: string;
    }>;
  } | null;
  compatibilityFingerprint?: string | null;
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

export type UsageWindowPreset =
  | 'all'
  | 'today'
  | 'this-week'
  | 'last-7-days'
  | 'this-month'
  | 'custom';

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

export type ProviderProtocolV2 = 'responses' | 'chat_completions';
export type RouteKindV2 = 'direct' | 'gateway';

export type CredentialReferenceV2 =
  | { kind: 'env'; envKey: string }
  | { kind: 'keychain'; service: 'lam.remote-provider'; account: string; version: number }
  | { kind: 'auth_command'; approvalId: string }
  | { kind: 'codex_profile'; profileId: string }
  | { kind: 'none' };

export type UpstreamAuthV2 =
  | { kind: 'bearer'; credential: CredentialReferenceV2 }
  | { kind: 'header'; name: string; credential: CredentialReferenceV2 }
  | { kind: 'none' };

export type AdapterV2 =
  | { kind: 'none' }
  | { kind: 'local'; adapterId: string; upstreamPath: string };

export type ProviderDefinitionV2 = {
  id: string;
  name: string;
  protocol: ProviderProtocolV2;
  baseUrl: string;
  defaultModel: string;
  models: Array<{ id: string; label: string }>;
  upstreamAuth: UpstreamAuthV2;
  adapter: AdapterV2;
  compatibilityProfile?: string;
  codex: {
    displayName?: string;
    streamIdleTimeoutMs?: number;
    directRequestMaxRetries?: number;
    directStreamMaxRetries?: number;
    routeViaGateway?: boolean;
    queryParams: Record<string, string>;
    envHttpHeaders: Record<string, string>;
  };
};

export type CreateProviderRequestV2 = {
  expectedRevision: number;
  provider: ProviderDefinitionV2;
};
export type UpdateProviderRequestV2 = CreateProviderRequestV2;

export type DiscoverProviderModelsRequestV2 = {
  baseUrl: string;
  apiKey: string;
};

export type DiscoverProviderModelsViewV2 = {
  models: Array<{ id: string; label: string }>;
};

export type ApproveAuthCommandRequestV2 = {
  expectedRevision: number;
  executable: string;
  args: string[];
  timeoutMs: number;
  maxStdoutBytes: number;
  refreshIntervalMs: number;
};

export type AuthCommandApprovalViewV2 = {
  revision: number;
  approvalId: string;
  executable: string;
};

export type AuthCommandApprovalListV2 = {
  revision: number;
  approvals: AuthCommandApprovalViewV2[];
};

export type ProviderProfileViewV2 = {
  id: string;
  name: string;
  protocol: ProviderProtocolV2;
  baseUrl: string;
  defaultModel: string;
  models: Array<{ id: string; label: string }>;
  upstreamAuth: UpstreamAuthV2;
  adapter: AdapterV2;
  compatibilityProfile?: string;
  codex: ProviderDefinitionV2['codex'];
  storeRevision: number;
  usedBy: string[];
  readinessBlockers: string[];
  readiness?: { ready: boolean; blockers: string[]; bindingCount: number };
  capabilities?: {
    streaming: {
      effective: 'supported' | 'partial' | 'unsupported' | 'unknown';
      provenance: string;
    };
    functionTools: {
      effective: 'supported' | 'partial' | 'unsupported' | 'unknown';
      provenance: string;
    };
    structuredOutputs: {
      effective: 'supported' | 'partial' | 'unsupported' | 'unknown';
      provenance: string;
    };
    reasoning: {
      effective: 'supported' | 'partial' | 'unsupported' | 'unknown';
      provenance: string;
    };
    evidenceCurrent: boolean;
    evidenceFingerprint?: string | null;
    evidenceFixtureIds: string[];
  };
  lastHealth?: {
    providerId: string;
    observedAt: string;
    ok: boolean;
    latencyMs?: number;
    errorCode?: string;
  };
};

export type ProviderListViewV2 = {
  revision: number;
  providers: ProviderProfileViewV2[];
};

export type CreateProviderWithKeychainRequestV2 = {
  expectedRevision: number;
  provider: ProviderDefinitionV2;
  secret: string;
};

export type PlanAttachRequestV2 = {
  profileId: string;
  providerId: string;
  selectedModel: string;
};
export type ExecuteAttachRequestV2 = { planId: string; fingerprint: string };
export type ExecuteDetachRequestV2 = ExecuteAttachRequestV2;

export type ApiAccountProviderSelectionV2 =
  | { kind: 'new'; provider: ProviderDefinitionV2 }
  | { kind: 'existing'; providerId: string };

export type PlanApiAccountRequestV2 = {
  accountName: string;
  selectedModel: string;
  overwriteWrapper: boolean;
  provider: ApiAccountProviderSelectionV2;
};

export type ApiAccountPlanViewV2 = {
  planId: string;
  fingerprint: string;
  expiresAtMs: number;
  accountName: string;
  providerId: string;
  selectedModel: string;
  routeKind: RouteKindV2;
  operations: string[];
  warnings: string[];
  blockers: string[];
};

export type ExecuteApiAccountRequestV2 = {
  planId: string;
  fingerprint: string;
  apiKey?: string | null;
};

export type ApiAccountExecutionViewV2 = {
  account: CreateResult;
  provider: ProviderProfileViewV2;
  binding: ProfileProviderBindingViewV2;
  attach: AttachExecutionViewV2;
};

export type ApiAccountConnectionViewV2 = {
  profileId: string;
  providerId: string;
  protocol: 'responses';
  baseUrl: string;
  selectedModel: string;
  providerStoreRevision: number;
  apiKeyConfigured: boolean;
};

export type UpdateApiAccountConnectionRequestV2 = {
  profileId: string;
  expectedProviderStoreRevision: number;
  baseUrl: string;
  apiKey?: string;
};

export type ProfileAttachPlanViewV2 = {
  planId: string;
  fingerprint: string;
  expiresAtMs: number;
  profileId: string;
  providerId: string;
  selectedModel: string;
  routeKind: RouteKindV2;
  blockers: string[];
  warnings: string[];
  operations: string[];
  redactedPreview: string;
  expectedProviderStoreRevision: number;
  expectedBindingStoreRevision: number;
  expectedBindingRevision?: number | null;
  sourceConfigHash: string;
};

export type ProfileDetachPlanViewV2 = {
  planId: string;
  fingerprint: string;
  expiresAtMs: number;
  profileId: string;
  expectedBindingStoreRevision: number;
  expectedBindingRevision: number;
  sourceConfigHash: string;
};

export type ProfileProviderBindingViewV2 = {
  profileId: string;
  providerId: string;
  selectedModel: string;
  routeKind: RouteKindV2;
  revision: number;
  providerRevision: number;
};

export type AttachExecutionViewV2 = {
  operationId?: string | null;
  state: string;
  idempotent: boolean;
};

export type GatewayPortChangePlanV2 = {
  expectedStateRevision: number;
  oldPort: number;
  newPort: number;
  profileIds: string[];
  fingerprint: string;
};

export type GatewayPortMigrationOutcomeV2 = {
  oldPort: number;
  newPort: number;
  migratedProfiles: string[];
  stateRevision: number;
  bindingStoreRevision: number;
};

export type RotateProviderCredentialRequestV2 = {
  expectedRevision: number;
  providerId: string;
  expectedCredential: CredentialReferenceV2;
  secret: string;
};

export type CredentialRotationViewV2 = {
  provider: ProviderProfileViewV2;
  cleanupPending: boolean;
};

export type ProviderUpstreamTestViewV2 = {
  providerId: string;
  ok: boolean;
  routeKind: RouteKindV2;
  modelsEndpoint: string;
  redactedSummary: string;
};

export type LegacyCreateProviderCompatRequestV2 = {
  id: string;
  name: string;
  baseUrl: string;
  wireApi: string;
  defaultModel: string;
  envKey?: string | null;
  secret?: { kind: 'env'; envKey: string } | { kind: 'none' } | null;
};

export type LegacyCreateProviderResultV2 = {
  provider: ProviderProfileViewV2;
  warnings: string[];
};

export type StructuredErrorViewV2 = {
  code: string;
  message: string;
  recoverable: boolean;
  recoveryActions: string[];
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
