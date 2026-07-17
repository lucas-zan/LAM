# Tauri Command API 草案

> **基础契约草案 / 部分有效：** 本文保留 Phase 1 Codex MVP 的基础 command 形状。Provider、UsageQuota、dry-run plan、AgentProfile 等扩展以当前源码为准。整帐号 Sessions Sync 已下线；接力只通过 `relay_resume_session` 处理单条 session。

> **v0.2.1 authoritative extension:** external API accounts use
> `plan_api_account_v2` → `execute_api_account_v2`, model changes use
> `plan_api_account_model_switch_v2` → `execute_api_account_model_switch_v2`, and deletion uses
> `delete_api_account_v2`. The plan never contains a secret; `apiKey` exists
> only on execute. Native Responses account connection metadata is read with
> `get_api_account_connection_v2` and URL/write-only key changes use
> `update_api_account_connection_v2`.
> External model discovery uses the separate write-only `discover_provider_models_v2` command; its result is
> advisory and cannot mutate a Provider or API Account.
> `relay_resume_session` accepts `confirmCompatibilityLoss` and returns a sanitized compatibility report
> plus `compatibilityFingerprint`. A blocked report causes zero target writes.

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

export async function buildResumeCommand(req: ResumeCommandRequest) {
  return invoke("build_resume_command", { req });
}

export async function openTerminalWithResume(req: ResumeCommandRequest) {
  return invoke("open_terminal_with_resume", { req });
}
```

---

### 2.1 External Provider 模型发现（v0.2.1）

模型发现与创建事务分离。它只接受标准 OpenAI `/models` 响应并返回规范化候选项；用户选择或手工添加
模型后，才由 API Account plan/execute 持久化 allowlist。

```ts
export type DiscoverProviderModelsRequestV2 = {
  baseUrl: string;
  apiKey: string;
};

export type DiscoverProviderModelsViewV2 = {
  models: Array<{ id: string; label: string }>;
};

export async function discoverProviderModelsV2(
  req: DiscoverProviderModelsRequestV2,
): Promise<DiscoverProviderModelsViewV2> {
  return invoke("discover_provider_models_v2", { req });
}
```

```rust
#[tauri::command]
pub async fn discover_provider_models_v2(
    req: DiscoverProviderModelsRequestV2,
) -> Result<DiscoverProviderModelsViewV2, StructuredErrorView>;
```

`apiKey` 是 write-only secret：request 的自定义 `Debug` 必须输出 `[REDACTED]`，command 不得将它
写入 DTO、Provider、plan、journal 或日志。服务关闭系统 proxy 和 redirect，使用 bearer auth，并强制
15 秒总超时、1 MiB body、2048 个模型以及单 id 256 bytes 上限。只接受 `{ data: [{ id }] }`；空、
duplicate、含空白或控制字符的 id、缺失 `data` 和非成功 HTTP 都返回结构化错误且不暴露上游 body。

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
