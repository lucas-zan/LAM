# Tauri 2 + Rust：复刻 Codex `Manual reset expiry` 与 `Reset quota` 的实现说明

> **适用目标**：在 Tauri 2.0 + Rust 桌面端中，实现：
>
> 1. 显示 `Manual reset expiry (GMT+8)`（逐张 reset credit 的失效时间）；
> 2. 显示并执行 `Reset quota`（消费一张可用 reset credit，随后刷新额度）。
>
> **基线**：参考 `router-for-me/Cli-Proxy-API-Management-Center` 的 Codex quota 实现，并针对 Tauri/Rust 的安全性、幂等性和可维护性做必要修正。
>
> **源代码核对日期**：2026-06-29。

---

## TL;DR

- **常规 quota reset** 与 **Manual reset expiry** 是两套不同数据：
  - 常规额度窗口：来自 `usage` 的 `reset_at` / `reset_after_seconds`；
  - manual reset credit 到期：来自 reset-credit 列表里每个 credit 的 `expiresAt`。
- 参考项目的关键读取链路是：
  1. `GET https://chatgpt.com/backend-api/wham/usage`
  2. `GET https://chatgpt.com/backend-api/wham/rate-limit-reset-credits`
  3. 将 `credits[].expiresAt` 转成 `Asia/Shanghai` 显示。
- 参考项目的 Reset 操作是：
  - `POST https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume`
  - body：`{"redeem_request_id":"<UUID>"}`
  - 返回成功后重新读取 usage 和 credits；**不在客户端伪造/修改 quota**。
- **不要完全照抄参考项目的重试逻辑**：它每次调用生成一个新的 UUID；网络超时后若再次生成新 UUID，理论上可能重复消费 credit。你的实现必须在一次逻辑兑换生命周期中持久化并复用同一 UUID。
- 对正式产品，推荐采用 **Hybrid 设计**：
  - 常规 quota 与 reset：优先走官方 Codex App Server；
  - per-credit expiry：仅在用户明确启用 experimental private endpoint 时读取；若不可用，隐藏 expiry 列表，但仍可展示和执行官方支持的 reset。

**置信度：高**（参考仓库当前 `main` 的实现已逐项核对；私有 `wham` 接口属于非稳定 surface，未来兼容性为中低）。

---

## 1. 先纠正概念：三个不同时间

| UI 信息 | 典型字段 | 来源 | 含义 |
|---|---|---|---|
| 5h / Weekly reset | `reset_at` / `reset_after_seconds` | usage | 常规额度窗口何时恢复 |
| Manual reset expiry | `credits[].expiresAt` | reset credits detail | 已获得的一张手动重置 credit 在何时失效 |
| Subscription expiry | `chatgpt_subscription_active_until` 等 | 本地 auth 元数据或 account 信息 | ChatGPT 订阅到期时间 |

你要实现的 “Reset 1 / Reset 2 / Reset 3 + 日期” 属于第二行，不应由 5h 或 weekly window 推导。

### 1.1 为什么不能自己计算 `expiresAt`

`expiresAt` 是服务端对每张 credit 给出的绝对时间。不同 credit 的 grant time、有效期、状态可能不同；例如：

```json
{
  "credits": [
    {
      "id": "RateLimitResetCredit_xxx",
      "status": "available",
      "grantedAt": "2026-06-12T02:34:36Z",
      "expiresAt": "2026-07-12T02:34:36Z"
    }
  ]
}
```

不要把 “获赠日 + 30 天” 硬编码为业务规则。客户端只负责解析、校验和显示服务端返回的 `expiresAt`。

---

## 2. 参考项目的真实实现路径

### 2.1 从 UI 到 upstream 的调用流

```text
Tauri React/WebView
   │ invoke("codex_get_quota") / invoke("codex_reset_quota")
   ▼
Rust command layer（不向前端暴露 access token）
   ▼
CodexQuotaService
   ├─ GET  /backend-api/wham/usage
   ├─ GET  /backend-api/wham/rate-limit-reset-credits
   └─ POST /backend-api/wham/rate-limit-reset-credits/consume
   ▼
Codex / ChatGPT upstream
```

CLIProxyAPI Management Center 的 Web 前端并不直接持有或发送 OAuth token。它调用 CLIProxyAPI 的 Management API `POST /v0/management/api-call`；后端根据 `auth_index` 找到 credential，并将请求 header 内的 `$TOKEN$` 替换为该 credential 的 token 后再转发。

### 2.2 参考项目的请求定义

```text
Usage
GET https://chatgpt.com/backend-api/wham/usage

Reset-credit detail（私有/非稳定）
GET https://chatgpt.com/backend-api/wham/rate-limit-reset-credits

Consume reset credit（私有/非稳定）
POST https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume
Content-Type: application/json

{"redeem_request_id":"<UUID>"}
```

基础 headers：

```http
Authorization: Bearer <access_token>
Content-Type: application/json
User-Agent: codex_cli_rs/0.76.0 (Debian 13.0.0; x86_64) WindowsTerminal
Chatgpt-Account-Id: <optional account id>
```

参考项目在读取 credit detail 时还增加：

```http
Accept: application/json
OpenAI-Beta: codex-1
Originator: Codex Desktop
```

> `Chatgpt-Account-Id` 从 Codex OAuth 的 `id_token`（JWT payload）中提取 `chatgpt_account_id`。如果你的 credential 保存了明确 account ID，应优先读取已保存字段，而不是每次解析 JWT。

### 2.3 Reference 的 availability count 优先级

参考项目最终采用下列优先级来决定 `available_count`：

```text
credit-detail.availableCount
  > credit-detail.credits.length（当数组非空）
  > usage.rate_limit_reset_credits.available_count
```

这能应对 detail endpoint 只返回数组、或 usage endpoint 仅返回汇总 count 的不同 payload。

---

## 3. 设计选择：建议你用 Hybrid，不要只依赖 private endpoint

### 3.1 方案对比

| 方案 | 可读 5h/weekly | 可 reset | 可读逐张 expiry | OAuth token 管理 | 推荐度 |
|---|---:|---:|---:|---|---:|
| A. 直接调 `wham` | 是 | 是 | 是 | 你负责 | 中 |
| B. 官方 Codex App Server | 是 | 是 | 否 | Codex 负责 | 高 |
| C. Hybrid | 是 | 是 | 有条件 | 大部分由 Codex 负责 | **最高** |

### 3.2 官方 App Server 能力

官方 Codex App Server 已公开：

```json
{"method":"account/rateLimits/read","id":1}
```

可返回 `rateLimits`、`planType`、常规 window、可用 reset count。消费 reset：

```json
{
  "method": "account/rateLimitResetCredit/consume",
  "id": 2,
  "params": { "idempotencyKey": "<UUID>" }
}
```

消费结果可能包含：`reset`、`alreadyRedeemed`、`nothingToReset`、`noCredit`。官方建议：消费后再调用 `account/rateLimits/read`，不要依赖本地推算。

### 3.3 Hybrid 的产品策略

```text
quota windows / available count / reset action
  → official Codex App Server first

per-credit expiry list
  → private GET endpoint, only when:
     1) user explicitly enables “Show reset-credit expiry (experimental)”
     2) OAuth credential is available to Rust backend
     3) endpoint returns a recognized payload

private endpoint fails (401 / 404 / schema change)
  → retain quota + count + reset
  → hide expiry list and show “expiry details unavailable”
  → do not mark entire account quota unavailable
```

这是最合理的 degradation path。因为截至本文核对日期，官方 App Server 能读 count 和执行 reset，但没有逐张 credit 的 `expiresAt`；这一缺口也已在 Codex 公共 issue 中被明确提出。

---

## 4. Tauri/Rust 架构

### 4.1 模块划分

```text
src-tauri/src/
├── main.rs
├── state.rs                    # AppState / service registration
├── commands/
│   └── codex_quota.rs          # #[tauri::command]
├── codex/
│   ├── mod.rs
│   ├── model.rs                # serde DTO + frontend view model
│   ├── service.rs              # fetch + consume orchestration
│   ├── private_wham.rs         # experimental REST adapter
│   ├── app_server.rs           # supported JSON-RPC adapter (optional)
│   ├── credential_store.rs     # token/account-id only in Rust
│   ├── operation_store.rs      # idempotency key persistence
│   └── time.rs                 # UTC -> Asia/Shanghai formatting
└── error.rs
```

### 4.2 前后端边界

前端只能收到：

```ts
export type CodexQuotaSnapshot = {
  accountId: string;
  planType?: string;
  windows: Array<{
    id: "five-hour" | "weekly" | string;
    usedPercent?: number;
    remainingPercent?: number;
    resetAtUnix?: number;
    resetDisplay?: string;
  }>;
  resetCredits: {
    availableCount?: number;
    credits: Array<{
      id: string;
      status?: string;
      expiresAt?: string;
      expiresAtShanghai?: string;
    }>;
    detailStatus: "available" | "unavailable" | "unsupported";
    detailError?: string;
  };
};
```

前端**不应**收到：`access_token`、refresh token、完整 JWT、upstream raw request header、或未经脱敏的 upstream response body。

---

## 5. Cargo 依赖建议

```toml
# src-tauri/Cargo.toml
[dependencies]
tauri = "2"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "process"] }
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
chrono-tz = "0.10"
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "2"
url = "2"
async-trait = "0.1"
tracing = "0.1"

# 可选：持久化 reset operation / secure secret storage
# keyring = "3"
# tauri-plugin-stronghold = "2"
```

- `reqwest::ClientBuilder::timeout()` 是整个 request lifecycle 的总 deadline，必须设置；不要让 quota refresh 无限阻塞。
- 对 token 存储，优先使用系统 keychain 或 Tauri Stronghold；普通 `tauri-plugin-store` 不应存 OAuth token 明文。

---

## 6. Rust 数据模型

> 重点：对字段同时兼容 snake_case 与 camelCase；private endpoint 的 response shape 不是稳定 contract，所以 detail parser 必须容忍 wrapper 差异。

```rust
// src-tauri/src/codex/model.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct UsagePayload {
    #[serde(default, alias = "planType")]
    pub plan_type: Option<String>,

    #[serde(default, alias = "rateLimit")]
    pub rate_limit: Option<RateLimitInfo>,

    #[serde(default, alias = "rateLimitResetCredits")]
    pub rate_limit_reset_credits: Option<ResetCreditsCount>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitInfo {
    #[serde(default, alias = "primaryWindow")]
    pub primary_window: Option<UsageWindow>,

    #[serde(default, alias = "secondaryWindow")]
    pub secondary_window: Option<UsageWindow>,

    #[serde(default, alias = "limitReached")]
    pub limit_reached: Option<bool>,

    #[serde(default)]
    pub allowed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageWindow {
    #[serde(default, alias = "usedPercent")]
    pub used_percent: Option<f64>,

    #[serde(default, alias = "limitWindowSeconds")]
    pub limit_window_seconds: Option<i64>,

    #[serde(default, alias = "resetAfterSeconds")]
    pub reset_after_seconds: Option<i64>,

    #[serde(default, alias = "resetAt")]
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResetCreditsCount {
    #[serde(default, alias = "availableCount")]
    pub available_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredit {
    pub id: String,
    pub status: Option<String>,
    pub reset_type: Option<String>,
    pub granted_at: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ResetCreditDetails {
    pub available_count: Option<u32>,
    pub credits: Vec<ResetCredit>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindowView {
    pub id: String,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub reset_at_unix: Option<i64>,
    pub reset_display: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCreditsView {
    pub available_count: Option<u32>,
    pub credits: Vec<ResetCreditView>,
    pub detail_status: String, // available | unavailable | unsupported
    pub detail_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCreditView {
    pub id: String,
    pub status: Option<String>,
    pub expires_at: Option<String>,
    pub expires_at_shanghai: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexQuotaSnapshot {
    pub account_id: String,
    pub plan_type: Option<String>,
    pub windows: Vec<QuotaWindowView>,
    pub reset_credits: ResetCreditsView,
}
```

### 6.1 Detail payload 的容错解析

不要假设 reset-credit detail 一定是单一结构。建议将 response 先解析为 `serde_json::Value`，从多个可能 wrapper 提取数据：

```rust
// src-tauri/src/codex/private_wham.rs
use serde_json::Value;
use crate::codex::model::{ResetCredit, ResetCreditDetails};

fn as_u32(v: Option<&Value>) -> Option<u32> {
    v.and_then(|x| {
        x.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .or_else(|| x.as_str()?.trim().parse::<u32>().ok())
    })
}

fn parse_credit(v: &Value) -> Option<ResetCredit> {
    let id = v.get("id")?.as_str()?.trim().to_owned();
    if id.is_empty() {
        return None;
    }

    let read_string = |snake: &str, camel: &str| {
        v.get(snake)
            .or_else(|| v.get(camel))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    };

    Some(ResetCredit {
        id,
        status: read_string("status", "status"),
        reset_type: read_string("reset_type", "resetType"),
        granted_at: read_string("granted_at", "grantedAt"),
        expires_at: read_string("expires_at", "expiresAt"),
    })
}

pub fn normalize_reset_credit_details(body: Value) -> Result<ResetCreditDetails, String> {
    // 支持：{ credits: [...] }、{ rate_limit_reset_credits: { ... } }、
    // { rateLimitResetCredits: { ... } }；保持 strict enough，避免错误 UI。
    let root = body
        .get("rate_limit_reset_credits")
        .or_else(|| body.get("rateLimitResetCredits"))
        .unwrap_or(&body);

    let available_count = as_u32(
        root.get("available_count")
            .or_else(|| root.get("availableCount")),
    );

    let credits_value = root.get("credits").or_else(|| body.get("credits"));
    let credits = credits_value
        .and_then(Value::as_array)
        .ok_or_else(|| "reset-credit payload has no credits array".to_string())?
        .iter()
        .filter_map(parse_credit)
        .collect::<Vec<_>>();

    Ok(ResetCreditDetails {
        available_count,
        credits,
    })
}
```

如果实际 response 不符合上述结构：

1. 在本地仅记录脱敏 schema（字段名、类型、HTTP status）；
2. 将 `detail_status` 设为 `unavailable`；
3. 仍从 usage response 展示 count；
4. 不要把未知内容 dump 到前端或日志。

---

## 7. `expiresAt` 转 GMT+8 的正确方式

```rust
// src-tauri/src/codex/time.rs
use chrono::{DateTime, Utc};
use chrono_tz::Asia::Shanghai;

pub fn format_shanghai_rfc3339(value: &str) -> Option<String> {
    let utc: DateTime<Utc> = value.parse().ok()?;
    Some(
        utc.with_timezone(&Shanghai)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}

pub fn format_unix_seconds_shanghai(seconds: i64) -> Option<String> {
    let utc = DateTime::<Utc>::from_timestamp(seconds, 0)?;
    Some(
        utc.with_timezone(&Shanghai)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}
```

规则：

- `expiresAt` 必须按 RFC 3339/ISO-8601 的**带 offset**时间解析；解析失败时显示 `--` 或不显示该 credit 的时间，不要把无 offset 字符串猜成用户本地时间。
- UI 标题写 `Manual reset expiry (GMT+8)` 时，所有同一区域必须统一使用 `Asia/Shanghai`，不能混用浏览器 locale。
- 常规 quota reset：优先 `reset_at`（Unix seconds，绝对时间）；只有缺失时才用 `now + reset_after_seconds`。

```rust
pub fn resolve_reset_at(window: &UsageWindow, now_unix: i64) -> Option<i64> {
    if window.reset_at.is_some_and(|v| v > 0) {
        return window.reset_at;
    }
    window
        .reset_after_seconds
        .filter(|v| *v > 0)
        .map(|delta| now_unix + delta)
}
```

---

## 8. 私有 `wham` REST adapter（与参考项目兼容）

> 这部分专门用于复刻 Management Center 的 expiry 行为。请将它独立为 feature flag，例如 `experimental_reset_credit_expiry`。

```rust
// src-tauri/src/codex/private_wham.rs
use std::time::Duration;

use reqwest::{header, Client, StatusCode};
use serde_json::{json, Value};
use uuid::Uuid;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const RESET_CREDITS_URL: &str =
    "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";
const RESET_CREDITS_CONSUME_URL: &str =
    "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume";
const CODEX_USER_AGENT: &str =
    "codex_cli_rs/0.76.0 (Debian 13.0.0; x86_64) WindowsTerminal";

#[derive(Clone)]
pub struct CodexCredential {
    pub account_id: String,
    pub access_token: String,
    pub chatgpt_account_id: Option<String>,
}

#[derive(Clone)]
pub struct WhamClient {
    http: Client,
}

impl WhamClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            // 避免携带 Authorization 自动跟随到重定向目标。
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(CODEX_USER_AGENT)
            .build()?;
        Ok(Self { http })
    }

    fn request_headers(&self, credential: &CodexCredential) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", credential.access_token)
                .parse()
                .expect("token is a valid header value"),
        );
        headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
        if let Some(account_id) = credential.chatgpt_account_id.as_deref() {
            if let Ok(value) = account_id.parse() {
                headers.insert("Chatgpt-Account-Id", value);
            }
        }
        headers
    }

    pub async fn fetch_usage(&self, credential: &CodexCredential) -> Result<Value, WhamError> {
        self.send_json(
            self.http.get(USAGE_URL).headers(self.request_headers(credential)),
        )
        .await
    }

    pub async fn fetch_reset_credit_details(
        &self,
        credential: &CodexCredential,
    ) -> Result<Value, WhamError> {
        let mut headers = self.request_headers(credential);
        headers.insert(header::ACCEPT, header::HeaderValue::from_static("application/json"));
        headers.insert("OpenAI-Beta", header::HeaderValue::from_static("codex-1"));
        headers.insert("Originator", header::HeaderValue::from_static("Codex Desktop"));

        // Reference project uses 8 seconds for this non-critical detail request.
        self.send_json(
            self.http
                .get(RESET_CREDITS_URL)
                .headers(headers)
                .timeout(Duration::from_secs(8)),
        )
        .await
    }

    pub async fn consume_reset_credit(
        &self,
        credential: &CodexCredential,
        redeem_request_id: Uuid,
    ) -> Result<(), WhamError> {
        let response = self
            .http
            .post(RESET_CREDITS_CONSUME_URL)
            .headers(self.request_headers(credential))
            .json(&json!({ "redeem_request_id": redeem_request_id }))
            .send()
            .await
            .map_err(WhamError::Network)?;

        if !response.status().is_success() {
            return Err(WhamError::Upstream {
                status: response.status(),
                message: safe_error_message(response).await,
            });
        }
        Ok(())
    }

    async fn send_json(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<Value, WhamError> {
        let response = request.send().await.map_err(WhamError::Network)?;
        if !response.status().is_success() {
            return Err(WhamError::Upstream {
                status: response.status(),
                message: safe_error_message(response).await,
            });
        }
        response.json::<Value>().await.map_err(WhamError::Network)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WhamError {
    #[error("network error: {0}")]
    Network(reqwest::Error),
    #[error("upstream returned {status}: {message}")]
    Upstream { status: StatusCode, message: String },
}

async fn safe_error_message(response: reqwest::Response) -> String {
    // 限制长度，避免把 HTML、token 片段或大 response 直接写入日志/UI。
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let text = text.replace('\n', " ").chars().take(400).collect::<String>();
    if text.is_empty() { format!("HTTP {status}") } else { format!("HTTP {status}: {text}") }
}
```

### 8.1 不要直接用这段代码的三个坑

1. `expect("token is a valid header value")` 在生产版应替换为错误返回；credential 值必须事先校验。
2. 不能让 UI 传 URL、任意 header 或 token。CLIProxyAPI 的通用 `api-call` 需要严密保护；你的 Tauri 应用没有必要把它暴露给 WebView。
3. 不要对 `POST .../consume` 进行“超时后自动再试并生成新 UUID”的 retry。

---

## 9. 服务层：读取快照、恢复 count、构造 UI 数据

```rust
// src-tauri/src/codex/service.rs（核心伪实现）
use chrono::Utc;
use serde_json::Value;

use crate::codex::{
    model::{CodexQuotaSnapshot, ResetCreditsView, ResetCreditView, UsagePayload},
    private_wham::{normalize_reset_credit_details, CodexCredential, WhamClient},
    time::{format_shanghai_rfc3339, format_unix_seconds_shanghai},
};

pub async fn fetch_quota_snapshot(
    client: &WhamClient,
    credential: &CodexCredential,
    enable_expiry_details: bool,
) -> Result<CodexQuotaSnapshot, AppError> {
    // usage 是核心请求；detail 是 optional enhancement。
    let usage_value = client.fetch_usage(credential).await?;
    let usage: UsagePayload = serde_json::from_value(usage_value)
        .map_err(|_| AppError::InvalidPayload("usage schema is not recognized".into()))?;

    let usage_count = usage
        .rate_limit_reset_credits
        .as_ref()
        .and_then(|x| x.available_count);

    let (credits, detail_status, detail_error, detail_count) = if enable_expiry_details {
        match client.fetch_reset_credit_details(credential).await {
            Ok(value) => match normalize_reset_credit_details(value) {
                Ok(details) => {
                    let count = details.available_count.or_else(|| {
                        (!details.credits.is_empty()).then_some(details.credits.len() as u32)
                    });
                    let views = details.credits.into_iter().map(|credit| ResetCreditView {
                        id: credit.id,
                        status: credit.status,
                        expires_at_shanghai: credit
                            .expires_at
                            .as_deref()
                            .and_then(format_shanghai_rfc3339),
                        expires_at: credit.expires_at,
                    }).collect();
                    (views, "available".into(), None, count)
                }
                Err(err) => (vec![], "unavailable".into(), Some(err), None),
            },
            Err(err) => (vec![], "unavailable".into(), Some(redact_error(&err)), None),
        }
    } else {
        (vec![], "unsupported".into(), None, None)
    };

    // 与参考项目保持一致的 count 优先级。
    let available_count = detail_count.or(usage_count);

    let now = Utc::now().timestamp();
    let windows = build_window_views(usage.rate_limit.as_ref(), now);

    Ok(CodexQuotaSnapshot {
        account_id: credential.account_id.clone(),
        plan_type: usage.plan_type,
        windows,
        reset_credits: ResetCreditsView {
            available_count,
            credits,
            detail_status,
            detail_error,
        },
    })
}

fn build_window_views(
    limit: Option<&crate::codex::model::RateLimitInfo>,
    now: i64,
) -> Vec<crate::codex::model::QuotaWindowView> {
    const FIVE_HOURS: i64 = 18_000;
    const WEEK: i64 = 604_800;

    let mut result = Vec::new();
    for window in limit.into_iter().flat_map(|x| [x.primary_window.as_ref(), x.secondary_window.as_ref()]) {
        let Some(window) = window else { continue };
        let duration = window.limit_window_seconds;
        let id = match duration {
            Some(FIVE_HOURS) => "five-hour",
            Some(WEEK) => "weekly",
            _ => "other",
        }.to_string();

        let reset_at = window.reset_at.filter(|v| *v > 0).or_else(|| {
            window.reset_after_seconds.filter(|v| *v > 0).map(|v| now + v)
        });
        let used = window.used_percent.map(|v| v.clamp(0.0, 100.0));

        result.push(crate::codex::model::QuotaWindowView {
            id,
            used_percent: used,
            remaining_percent: used.map(|v| 100.0 - v),
            reset_display: reset_at.and_then(format_unix_seconds_shanghai),
            reset_at_unix: reset_at,
        });
    }
    result
}
```

> **推荐强化**：对于 `reset_after_seconds` fallback，使用 upstream `Date` response header 校正本机 clock drift。参考项目只对 Antigravity 做了 server-time offset；你可以把这一改进应用于 Codex。绝对字段 `reset_at` 不需要校正。

---

## 10. `Reset quota`：必须是幂等状态机

### 10.1 Reference 的流程

```text
用户点击 Reset
  → 生成 UUID
  → POST consume
  → 2xx
  → 重新 fetch quota + credits
```

### 10.2 参考实现不足：网络不确定状态

如果 POST 已被 upstream 接收并完成，但连接在客户端超时：

```text
你不知道 reset 到底有没有成功。
```

此时：

- **错误做法**：立即生成新的 UUID 再请求一次；
- **正确做法**：复用第一次的 request id，或先重新读取 quota；
- **最佳做法**：将 operation UUID 和状态持久化，使 app restart 后也不会遗失幂等 key。

### 10.3 建议 operation state

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetOperation {
    pub id: uuid::Uuid,
    pub account_id: String,
    pub created_at_unix: i64,
    pub status: ResetOperationStatus,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetOperationStatus {
    Pending,
    Succeeded,
    OutcomeUnknown,
    Rejected,
}
```

状态转移：

```text
new UUID → Pending → POST consume
                      ├─ 2xx                 → Succeeded → force refresh
                      ├─ definite 4xx         → Rejected
                      └─ timeout / connection → OutcomeUnknown

OutcomeUnknown → retry with SAME UUID OR read fresh quota first
```

### 10.4 Reset command 的关键实现

```rust
#[tauri::command]
pub async fn codex_reset_quota(
    account_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<CodexQuotaSnapshot, CommandError> {
    // 1) 对同一 account 加单独锁：避免用户双击、多个窗口并发 consume。
    let _guard = state.account_locks.lock_for(&account_id).await;

    // 2) 从安全存储读取；token 不出 Rust 进程。
    let credential = state.credentials.load_codex(&account_id).await?;

    // 3) 拿到未完成 operation；没有才创建 UUID，并持久化。
    let operation = state.operations.get_or_create_pending(&account_id).await?;

    // 4) REST private mode：使用同一个 redeem_request_id。
    match state.wham.consume_reset_credit(&credential, operation.id).await {
        Ok(()) => {
            state.operations.mark_succeeded(operation.id).await?;
            // 必须强制刷新，而不是本地 --availableCount。
            state.quota.fetch_fresh(&credential).await.map_err(Into::into)
        }
        Err(err) if is_transport_unknown(&err) => {
            state.operations.mark_outcome_unknown(operation.id).await?;
            Err(CommandError::RetrySameOperation {
                operation_id: operation.id.to_string(),
            })
        }
        Err(err) => {
            state.operations.mark_rejected(operation.id).await?;
            Err(err.into())
        }
    }
}
```

### 10.5 官方 App Server 适配时的差异

| Private `wham` REST | Official Codex App Server |
|---|---|
| `redeem_request_id` | `idempotencyKey` |
| 2xx 表示 REST 调用成功 | `outcome` 有语义：`reset` / `alreadyRedeemed` / `nothingToReset` / `noCredit` |
| per-credit expiry 可读（但不稳定） | count 和 reset 是支持功能；无逐张 expiry |
| OAuth token 由你管理 | Codex app-server 管理 auth/refresh |

对 app-server 的 `alreadyRedeemed`：按 idempotent success 处理，然后重新拉 `account/rateLimits/read`。

---

## 11. Tauri commands 与 WebView 调用

### 11.1 Rust command 注册

```rust
// src-tauri/src/main.rs
mod commands;
mod codex;
mod error;
mod state;

fn main() {
    tauri::Builder::default()
        .manage(state::AppState::new().expect("app state"))
        .invoke_handler(tauri::generate_handler![
            commands::codex_quota::codex_get_quota,
            commands::codex_quota::codex_reset_quota,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
```

### 11.2 TypeScript 调用

```ts
import { invoke } from "@tauri-apps/api/core";

export async function loadCodexQuota(accountId: string) {
  return invoke<CodexQuotaSnapshot>("codex_get_quota", { accountId });
}

export async function resetCodexQuota(accountId: string) {
  return invoke<CodexQuotaSnapshot>("codex_reset_quota", { accountId });
}
```

### 11.3 前端 UI 行为要求

```text
Refresh:
  - 读取 quota snapshot
  - 常规 quota 展示 5h / weekly
  - count > 0 才显示 Reset 按钮
  - detailStatus=available 且 credits 非空时，显示 Manual reset expiry (GMT+8)

Reset click:
  - 明确 confirmation dialog
  - disable 当前 account 的 Reset 按钮
  - success 后直接采用 command 返回的新 snapshot
  - failure 时不乐观减 1
  - OutcomeUnknown 时提示“状态不确定；正在以相同操作 ID 核验”，不能让用户反复创建新请求
```

用 React 伪代码：

```tsx
const canReset = (snapshot.resetCredits.availableCount ?? 0) > 0 && !resetting;

{snapshot.resetCredits.credits.length > 0 && (
  <section>
    <h4>Manual reset expiry (GMT+8)</h4>
    {snapshot.resetCredits.credits.map((credit, index) => (
      <div key={credit.id}>
        <span>Reset {index + 1}</span>
        <time>{credit.expiresAtShanghai ?? "--"}</time>
      </div>
    ))}
  </section>
)}

<button disabled={!canReset} onClick={onReset}>
  Reset quota
</button>
```

---

## 12. Credential 与 Account ID

### 12.1 可接受的数据结构

```rust
pub struct CodexCredentialRecord {
    pub account_id: String,             // app 内部稳定 ID
    pub provider: String,               // "codex"
    pub access_token_secret_ref: String,
    pub chatgpt_account_id: Option<String>,
    pub expires_at: Option<String>,
}
```

### 12.2 Account ID 提取（仅作为 fallback）

Codex reference 会从以下位置寻找：

```text
file.id_token
metadata.id_token
attributes.id_token
```

并从 JWT payload 读取：

```json
{ "chatgpt_account_id": "..." }
```

实现建议：

1. OAuth login/import 成功时，解析一次并保存 `chatgpt_account_id`；
2. 调用时优先读取保存值；
3. 缺失时才 decode JWT payload；
4. 不验证 JWT signature 来“提取非安全 UI routing 字段”是可行的，但不能据此做授权决策；真正授权始终由 upstream token 校验。

### 12.3 Secret storage

- 推荐：OS keychain / Stronghold；
- 绝对不要：把 `access_token` 放到 React Zustand/Redux、localStorage、日志、crash report、或 command response；
- 限制 command 参数为 `accountId`，而不是 `token`；
- Redaction：任何日志将 bearer token 替换为 `[REDACTED]`。

---

## 13. 错误处理和 UI 文案策略

| 情形 | 后端处理 | UI 行为 |
|---|---|---|
| 401 | 标记 credential 失效；引导重新登录 | 不显示 stale expiry |
| 403 | 记录无权限或 workspace/account mismatch | 保留上次 cache 但标明 stale |
| 404 detail endpoint | `detail_status=unsupported` | quota/count 仍展示；隐藏 expiry |
| usage 2xx，但 detail timeout | `detail_status=unavailable` | count fallback 到 usage；显示轻提示 |
| consume 4xx + no credit | 不本地减 count；强制 refresh | 提示已无可用 reset |
| consume timeout | `OutcomeUnknown`，保留 UUID | 不允许新 UUID 重试 |
| schema drift | 只记录字段 schema + status | 显示 “detail unavailable” |

建议 error code：

```text
CODEX_AUTH_REQUIRED
CODEX_USAGE_UNAVAILABLE
CODEX_RESET_CREDIT_DETAIL_UNAVAILABLE
CODEX_RESET_NO_CREDIT
CODEX_RESET_NOTHING_TO_RESET
CODEX_RESET_OUTCOME_UNKNOWN
CODEX_UPSTREAM_SCHEMA_CHANGED
```

不要直接把 upstream 原始 error body 原样展示给最终用户。

---

## 14. 安全边界：比参考项目更严格

### 14.1 不要实现通用 WebView HTTP proxy

CLIProxyAPI 的 `/v0/management/api-call` 设计为管理 API 的 credential-aware 通用 HTTP 转发器；参考项目用它来替换 `$TOKEN$`。Tauri 应用不需要向前端开放同等能力。

你的 Rust 后端应：

- 将 upstream URL 写死为 allowlist；
- method 写死为 GET / POST；
- header 由 Rust 组装；
- body schema 写死；
- 禁止 WebView 输入 URL、Host、Authorization、Cookie、proxy URL；
- 禁止跟随跨域 redirect；
- 不记录 token 或完整 response。

### 14.2 rate limiting 与并发控制

- 每个 account 同时最多一个 `consume`；
- quota refresh 至少做 30–60 秒 cache（用户手动刷新和 reset 后除外）；
- reset credit detail 失败不触发无限重试；
- 对 429 / 5xx 采用 bounded exponential backoff，仅限 idempotent GET；POST 只可用**同一个** operation ID 重试。

---

## 15. 测试清单

### 15.1 单元测试

```text
[ ] `expiresAt` RFC3339 UTC → Asia/Shanghai 转换准确
[ ] 已带 +08:00 的 expiresAt 不被二次偏移
[ ] 无 offset / 非法时间返回 None
[ ] reset_at 优先级高于 reset_after_seconds
[ ] 18000 → five-hour；604800 → weekly
[ ] usedPercent clamp 到 [0, 100]
[ ] detail count > credits length > usage count 的 fallback 逻辑符合预期
[ ] snake_case / camelCase payload 都可解析
[ ] unknown detail schema 不会 panic
[ ] 同一 pending operation 重启后仍返回同一 UUID
```

### 15.2 HTTP fixture 测试

使用 `wiremock` / `httpmock` / 本地 Axum mock server：

```text
[ ] usage=200 + credits=200 → 可显示 Reset 1..N expiry
[ ] usage=200 + credits=404 → 可显示 count，无 expiry
[ ] usage=200 + credits=8s timeout → count fallback，状态 unavailable
[ ] consume=204/200 → 强制重新读取 usage
[ ] consume=401 → 标记需重新登录
[ ] consume connection drop → operation=OutcomeUnknown
[ ] 再次 reset → 验证请求仍使用第一次 UUID
```

### 15.3 手动验收

```text
[ ] Reset 按钮只在 availableCount > 0 时可点
[ ] confirmation dialog 明确说明会消费一张 credit
[ ] 双击不会产生两个 consume 请求
[ ] 网络断开后不会新建另一个 UUID
[ ] reset 成功后 UI 不本地 --count，而是展示 fresh snapshot
[ ] 私有 detail endpoint 不可用时，产品其余 quota 功能完整可用
```

---

## 16. 推荐交付顺序

### Phase 1：受支持、低风险

1. 接入 Codex App Server `account/rateLimits/read`；
2. 展示 5h / weekly / plan type / available reset count；
3. 接入 `account/rateLimitResetCredit/consume`；
4. 实现 operation UUID persistence 和 per-account lock；
5. 完成 reset 后强制刷新。

### Phase 2：与参考项目 UI 一致的 expiry 体验

1. 增加 experimental `wham` detail adapter；
2. 仅读取 `expiresAt`，不让前端接触 token；
3. 显示 GMT+8 expiry list；
4. 实现 schema drift / 404 / timeout graceful degradation；
5. 增加隐私和兼容性说明。

### Phase 3：可观测性

记录脱敏 metrics：

```text
quota_usage_fetch_success_total
quota_reset_detail_available_total
quota_reset_detail_unavailable_total
quota_consume_success_total
quota_consume_unknown_outcome_total
quota_consume_no_credit_total
```

不要记录 email、account id 原文、token、request body 或 credit ID 原文。

---

## 17. 源码映射（参考项目）

| 目标 | 参考文件 | 关键函数/常量 |
|---|---|---|
| Codex quota orchestration | `src/components/quota/quotaConfigs.ts` | `fetchCodexQuota`、`fetchCodexResetCredits`、`consumeCodexRateLimitResetCredit`、`resetCodexQuota`、`renderCodexItems` |
| Upstream URLs/headers | `src/utils/quota/constants.ts` | `CODEX_USAGE_URL`、`CODEX_RATE_LIMIT_RESET_CREDITS_URL`、`CODEX_RATE_LIMIT_RESET_CREDITS_CONSUME_URL`、`CODEX_REQUEST_HEADERS` |
| Window reset 时间格式化 | `src/utils/quota/formatters.ts` | `formatCodexResetLabel` |
| Account ID / plan / subscription 解析 | `src/utils/quota/resolvers.ts` | `resolveCodexChatgptAccountId`、`resolveCodexPlanType` |
| UI confirmation + reset 后 refresh | `src/components/quota/QuotaSection.tsx` | `resetQuotaForFile` |
| Management API token replacement | CLIProxyAPI `internal/api/handlers/management/api_tools.go` | `APICall`、`resolveTokenForAuth` |

来源：

1. `router-for-me/Cli-Proxy-API-Management-Center`：
   - https://github.com/router-for-me/Cli-Proxy-API-Management-Center/blob/main/src/components/quota/quotaConfigs.ts
   - https://github.com/router-for-me/Cli-Proxy-API-Management-Center/blob/main/src/utils/quota/constants.ts
   - https://github.com/router-for-me/Cli-Proxy-API-Management-Center/blob/main/src/utils/quota/formatters.ts
   - https://github.com/router-for-me/Cli-Proxy-API-Management-Center/blob/main/src/utils/quota/resolvers.ts
2. `router-for-me/CLIProxyAPI`：
   - https://github.com/router-for-me/CLIProxyAPI/blob/main/internal/api/handlers/management/api_tools.go
3. 官方 Codex App Server：
   - https://developers.openai.com/codex/app-server

---

## 最终实施结论

要做到与参考项目截图**完全一致**，你需要 private detail endpoint 返回的 `credits[].expiresAt`；仅用官方 App Server 做不到逐张 expiry。

但对于 `Reset quota`，推荐切换到官方 App Server 的 `account/rateLimitResetCredit/consume`，以 `idempotencyKey` 为核心实现持久化幂等；当 private expiry detail 失效时，应用仍能可靠地展示 quota、available reset count 和执行 reset。

