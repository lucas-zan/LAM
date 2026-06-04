# Tauri Command API 草案

> **基础契约草案 / 部分有效：** 本文保留 Phase 1 Codex MVP 的基础 command 形状。Provider、UsageQuota、Sync manifest、dry-run plan、AgentProfile 等扩展以 `docs/FINAL-DESIGN.md` §6.3 和 `docs/IMPLEMENTATION-ISSUES.md` 为准。

版本：0.1 draft

---

## 1. TypeScript 类型

```ts
export type CodexAccount = {
  id: string;
  displayName: string;
  codexHome: string;
  wrapperPath?: string;
  hasAuth: boolean;
  hasConfig: boolean;
  hasHistory: boolean;
  sessionCount: number;
  latestSessionModifiedAt?: string;
  managed: boolean;
  isRelay: boolean;
  relaySource?: string;
  relayIdentity?: string;
};

export type CodexSession = {
  id: string;
  accountId: string;
  path: string;
  modifiedAt: string;
  sizeBytes: number;
  cwd?: string;
  summary?: string;
  firstUserMessage?: string;
};

export type CreateAccountRequest = {
  name: string;
  copyConfigFromAccountId?: string;
  createWrapper: boolean;
  openLoginAfterCreate: boolean;
};

export type CreateRelayAccountRequest = {
  relayIdentityAccountId: string;
  sourceAccountId: string;
  name?: string;
  createWrapper: boolean;
  openLoginAfterCreate: boolean;
};

export type SyncOptions = {
  syncSessions: boolean;
  backupTargetSessions: boolean;
  sidecarBackupHistory: boolean;
  mergeHistory: boolean;
  dryRun: boolean;
};

export type SyncRequest = {
  fromAccountId: string;
  toAccountId: string;
  options: SyncOptions;
};

export type ResumeCommandRequest = {
  accountId: string;
  sessionId: string;
  cwd?: string;
  mode: "specific" | "last" | "allPicker";
};
```

---

## 2. Frontend API wrapper

```ts
import { invoke } from "@tauri-apps/api/core";

export async function listAccounts(): Promise<CodexAccount[]> {
  return invoke("list_accounts");
}

export async function listSessions(accountId: string): Promise<CodexSession[]> {
  return invoke("list_sessions", { accountId });
}

export async function createAccount(req: CreateAccountRequest) {
  return invoke("create_account", { req });
}

export async function createRelayAccount(req: CreateRelayAccountRequest) {
  return invoke("create_relay_account", { req });
}

export async function buildSyncPlan(req: SyncRequest) {
  return invoke("build_sync_plan", { req });
}

export async function executeSync(req: SyncRequest) {
  return invoke("execute_sync", { req });
}

export async function buildResumeCommand(req: ResumeCommandRequest) {
  return invoke("build_resume_command", { req });
}

export async function openTerminalWithResume(req: ResumeCommandRequest) {
  return invoke("open_terminal_with_resume", { req });
}
```

---

## 3. Rust command signatures

```rust
#[tauri::command]
pub async fn list_accounts() -> Result<Vec<CodexAccount>, AppError>;

#[tauri::command]
pub async fn list_sessions(account_id: String) -> Result<Vec<CodexSession>, AppError>;

#[tauri::command]
pub async fn create_account(req: CreateAccountRequest) -> Result<CreateAccountResult, AppError>;

#[tauri::command]
pub async fn create_relay_account(req: CreateRelayAccountRequest) -> Result<CreateAccountResult, AppError>;

#[tauri::command]
pub async fn build_sync_plan(req: SyncRequest) -> Result<SyncPlan, AppError>;

#[tauri::command]
pub async fn execute_sync(req: SyncRequest) -> Result<SyncResult, AppError>;

#[tauri::command]
pub async fn build_resume_command(req: ResumeCommandRequest) -> Result<ResumeCommandResult, AppError>;

#[tauri::command]
pub async fn open_terminal_with_resume(req: ResumeCommandRequest) -> Result<(), AppError>;
```

---

## 4. 错误模型

```rust
#[derive(Debug, serde::Serialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    pub details: Option<serde_json::Value>,
}
```

常见错误码：

```text
INVALID_ACCOUNT_NAME
ACCOUNT_ALREADY_EXISTS
ACCOUNT_NOT_FOUND
CODEX_HOME_UNSAFE
WRAPPER_DIR_NOT_IN_PATH
CODEX_BINARY_NOT_FOUND
SESSION_NOT_FOUND
SYNC_BLOCKED_SENSITIVE_FILE
TERMINAL_PERMISSION_DENIED
IO_ERROR
PARSE_ERROR
```

---

## 5. Dry-run result 示例

```json
{
  "fromAccountId": "a",
  "toAccountId": "b-relay-a",
  "operations": [
    {
      "type": "backup_dir",
      "from": "/Users/me/.codex-b-relay-a/sessions",
      "to": "/Users/me/.codex-b-relay-a/sessions.backup.20260526-143000"
    },
    {
      "type": "copy_dir_merge",
      "from": "/Users/me/.codex-a/sessions",
      "to": "/Users/me/.codex-b-relay-a/sessions",
      "fileCount": 42
    }
  ],
  "blockedFiles": [
    "auth.json",
    "history.jsonl",
    "logs_2.sqlite"
  ],
  "warnings": [
    "history.jsonl will not be merged by default."
  ]
}
```
