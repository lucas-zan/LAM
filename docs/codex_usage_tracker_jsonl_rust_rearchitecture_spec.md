# Codex Usage Tracker JSONL 处理逻辑与 Rust 重构技术规格

> **目标**：为将 `codex-usage-tracking` 从 Python 迁移/重构到 Rust（或另一门强类型语言）提供可实施、可验证、可审计的技术规格。  
> **基线版本**：`codex-usage-tracking 0.11.4` / Git tag `v0.11.4` / commit `55265365cccdf27b2f05202766b951946a547c9a`。  
> **基线日期**：2026-06-28。  
> **范围**：JSONL 日志发现、解析、状态机、增量刷新、SQLite 聚合索引、线程归因、调用归因、诊断事实、隐私边界、查询/报表契约、Rust 目标架构、迁移与验收。  
> **不把“更快”当作唯一目标**：首要目标是 semantic parity（语义一致性）与数据正确性；性能优化只能建立在可重复 benchmark 之上。

---

## TL;DR

- 这个项目不是“读取 JSONL 并统计 token”的脚本，而是一个 **stateful local event-ingestion pipeline**：
  - 从 `~/.codex/sessions/**/*.jsonl` 与可选的 `~/.codex/archived_sessions/*.jsonl` 读取；
  - 通过 `session_index.jsonl` 取得 session → thread label 映射；
  - 维护跨行 parser state；
  - 仅把 `event_msg/token_count` 变成 usage event；
  - 把前序 `session_meta`、`turn_context`、非 token event 片段归因到下一条有效 token event；
  - 将 aggregate-only fields 写入 SQLite；
  - 生成 thread summaries、dashboard/CLI/MCP 查询结果。
- 重构最大的风险不在 JSON decoder，而在以下语义：
  1. `turn_context` 跨事件继承；
  2. session/subagent/parent thread graph；
  3. cumulative counter 的单调性去重；
  4. token event 前的 call-origin segment；
  5. diagnostic fact segment 的绑定；
  6. source cursor 与 parser state 的一致性；
  7. raw content 不得进入常规持久化与导出路径。
- 当前 Python 实现已具备 append-only 增量扫描，但有至少四个应在 Rust 版显式处理的边界：
  1. **partial trailing JSONL line**：cursor 提交到 EOF，不是最后完整换行，可能吞掉正在写入的末行；
  2. **增量扫描仍有全文件 line count 开销**：每次更新 metadata 时会重新遍历整文件计算行数；
  3. **source fingerprint 不是内容 hash**：`source_file_hash` 实际是 path hash；同 size + 同 mtime 的内容替换不能检测；
  4. **删除源文件不会自动移除历史 usage rows**：常规 refresh 不对“已不在发现集合中的旧 source”进行 tombstone/reconcile。
- Rust 版应该先交付一个稳定 `core + SQLite` ingestion engine；dashboard、MCP、plugin 应在核心语义完成 parity 后再移植。
- 不建议直接复制/翻译 Python 文件结构；建议按 domain / parser / storage / service / adapters 分层，并把 parser 写成可 golden-test 的纯核心。

**置信度**：高（当前 Python 解析/持久化流程）；中（公开 Rust 项目的 feature-parity 评估，生态变化很快）。

---

## 目录

1. [事实基线、边界与术语](#1-事实基线边界与术语)  
2. [原项目总体架构与模块责任](#2-原项目总体架构与模块责任)  
3. [日志输入与发现规则](#3-日志输入与发现规则)  
4. [JSONL envelope 与状态机](#4-jsonl-envelope-与状态机)  
5. [token_count → UsageEvent 的完整转换契约](#5-token_count--usageevent-的完整转换契约)  
6. [线程、subagent 与调用归因](#6-线程subagent-与调用归因)  
7. [Diagnostic Facts：安全聚合诊断](#7-diagnostic-facts安全聚合诊断)  
8. [SQLite 数据模型与索引契约](#8-sqlite-数据模型与索引契约)  
9. [刷新、增量解析与事务语义](#9-刷新增量解析与事务语义)  
10. [隐私、安全与 raw evidence 边界](#10-隐私安全与-raw-evidence-边界)  
11. [当前实现的风险、缺陷与改进项](#11-当前实现的风险缺陷与改进项)  
12. [Rust 目标架构](#12-rust-目标架构)  
13. [Rust parser/ingestion 详细设计](#13-rust-parseringestion-详细设计)  
14. [数据库迁移与兼容策略](#14-数据库迁移与兼容策略)  
15. [测试、golden fixtures 与验收标准](#15-测试golden-fixtures-与验收标准)  
16. [性能与运行时设计](#16-性能与运行时设计)  
17. [可参考的 Rust 项目与采用结论](#17-可参考的-rust-项目与采用结论)  
18. [实施路线与交付物](#18-实施路线与交付物)  
19. [不变量、决策清单与最终建议](#19-不变量决策清单与最终建议)  
20. [参考来源](#20-参考来源)  

---

# 1. 事实基线、边界与术语

## 1.1 版本冻结

重构必须以一个 **immutable baseline** 开始。建议冻结：

- PyPI distribution：`codex-usage-tracking==0.11.4`
- Git tag：`v0.11.4`
- source commit：`55265365cccdf27b2f05202766b951946a547c9a`
- package release date：2026-06-27
- parser adapter version：`codex-jsonl-v2`
- SQLite schema version：`10`

为什么不能只跟 `main`：

- 当前项目迭代频繁；
- JSONL parser、diagnostics、schema migrations 都在快速变化；
- 重构工程需要一个可复现的 expected behavior；
- 未冻结版本会让“Python 与 Rust 输出不一致”无法判定是 bug 还是上游变更。

> 建议把基线 source distribution 的 SHA-256 固化在 CI/ADR：  
> `e10905a584fa4eb19e8b2b0db975546e6f16cc1aede0d1d74fc4a3f0f8c5166b`

## 1.2 目标与非目标

### 目标

- 从本地 Codex JSONL logs 生成聚合 usage facts；
- 维持现有 Python 版的 event identity、thread grouping、usage counters、origin/diagnostics 的可验证语义；
- 支持 append-only incremental ingestion；
- 正常数据路径不持久化 prompt、assistant content、tool output、arguments、raw transcript；
- 提供可复用 CLI/API/db core；
- 可渐进接入 dashboard、MCP、plugin。

### 非目标

- 不重新 tokenize prompt；
- 不推断真实 OpenAI billing；
- 不把本地日志上传到云端；
- 不把 Rust 项目做成 raw transcript search engine；
- 不在 Phase 1 复刻全部 dashboard 文案、i18n 与前端交互；
- 不通过“把所有 JSON decode 为 `serde_json::Value` 再扫一遍”获得假性能。

## 1.3 核心术语

| 术语 | 定义 |
|---|---|
| `source file` | 一份 Codex session JSONL 文件。 |
| `envelope` | JSONL 的一行 JSON 对象，通常有 top-level `type`、`timestamp`、`payload`。 |
| `token event` | 满足 `envelope.type == "event_msg"` 且 `payload.type == "token_count"` 的行。 |
| `usage event` | 从一个有效 token event 生成的一条 aggregate usage record。 |
| `session` | Codex 日志语义中的 session id。 |
| `thread` | 用 `thread_name` / parent thread / parent session 等规则归并后的逻辑会话。 |
| `turn context` | 最近一条 `turn_context` 中的 model、effort、cwd、turn id、timezone 等上下文。 |
| `parser state` | 为 append-only continuation 保存的 aggregate-only 状态。 |
| `cursor` | 已解析 source 的 byte offset 与 line count，以及恢复状态。 |
| `call-origin segment` | 两个 token count 边界之间观察到的 user/tool/compaction/activity signals。 |
| `diagnostic segment` | 两个 token count 边界之间提取的安全聚合 diagnostic facts。 |
| `raw evidence` | 原始 JSONL 的 prompt/content/tool output 等信息；只能按需、运行时、显式触发读取。 |

---

# 2. 原项目总体架构与模块责任

## 2.1 数据流

```text
~/.codex/session_index.jsonl
  └─ SessionInfo(session_id, thread_name, updated_at)

~/.codex/sessions/**/*.jsonl
~/.codex/archived_sessions/*.jsonl
  └─ file discovery
      └─ incremental plan
          └─ byte-stream JSONL parser
              ├─ session_meta state
              ├─ turn_context state
              ├─ call-origin segment
              ├─ diagnostic-fact segment
              └─ token_count → UsageEvent
                  └─ SQLite upsert
                      ├─ usage_events
                      ├─ call_diagnostic_facts
                      ├─ source_files
                      ├─ thread_summaries
                      ├─ refresh_meta
                      └─ diagnostic_snapshots
                          ├─ CLI
                          ├─ dashboard
                          ├─ MCP
                          └─ export/report APIs
```

## 2.2 关键模块映射

| Python 模块 | 当前职责 | Rust 对应建议 |
|---|---|---|
| `parser.py` | JSONL discovery、session index、stateful parse、UsageEvent 构造、cursor state serialization | `core::parser`, `core::codec`, `core::state` |
| `models.py` | immutable dataclasses：`SessionInfo`、`UsageEvent`、`DiagnosticFact`、`RefreshResult` | `domain` structs/enums |
| `call_origin.py` | 元数据信号 → `user/codex/unknown` origin 分类 | `core::origin` |
| `diagnostic_facts.py` | 安全的 facts extraction、fact merge、segment persistence | `core::diagnostics` |
| `store_sources.py` | source freshness plan、source metadata persistence | `store::sources` |
| `store.py` | refresh orchestration、SQLite CRUD、query APIs、CSV export | `service::refresh`, `store::sqlite`, `service::queries` |
| `store_schema.py` / `schema.py` | SQLite schema、migrations、column contract checksum | `store::migrations`, `store::schema` |
| `store_thread_summaries.py` | materialized thread summary rebuild | `store::thread_summary` |
| `context.py` / `redaction.py` | runtime raw evidence loading + redaction | `evidence` adapter，后置实现 |
| `reports.py` / `recommendations.py` | report/domain rules | `service::reports` |
| `server.py` / `dashboard.py` / `mcp_server.py` | HTTP/dashboard/MCP adapters | `adapters::http`, `adapters::mcp` |

## 2.3 架构判断

**正确的迁移边界是：**

```text
Raw JSONL → parser state machine → aggregate events → SQLite aggregate index
```

不是：

```text
Python CLI/dashboard → 逐命令 Rust 翻译
```

后者会把 data semantics 混在 CLI、server、template、MCP handler 中，导致不可验证。

---

# 3. 日志输入与发现规则

## 3.1 默认路径

当前默认逻辑：

```text
~/.codex/sessions/**/*.jsonl
```

可选 include archived 时追加：

```text
~/.codex/archived_sessions/*.jsonl
```

此外读取：

```text
~/.codex/session_index.jsonl
```

用于获取：

```text
id → thread_name, updated_at
```

## 3.2 发现规则的精确行为

- `sessions` 是递归 glob；
- `archived_sessions` 仅使用单层 glob；
- 只保留 `is_file()` 的路径；
- 返回按路径排序的列表；
- source 是否 archive 通过 path components / normalized string 判断：
  - parser 中：path parts 含 `archived_sessions`
  - source metadata 中：normalized path 包含 `/archived_sessions/` 或以 `archived_sessions/` 开头。

### 重构要求

不要把 archive 判断写成平台特定的字符串匹配。建议统一：

```rust
fn is_archived_path(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "archived_sessions")
}
```

同时保留 legacy compatibility tests，验证 Windows separator、relative path、absolute path 的一致行为。

## 3.3 session filename → session id

当前 parser 从 rollout filename 正则提取 session UUID：

```text
rollout-[^-]+-[0-9T:-]+-<UUID>.jsonl
```

如果 parse failure：

- 仅在 `start_byte <= 0` 时增加 `unknown_filename_format` diagnostic；
- 不会 hard fail；
- 后续可从 `session_meta.payload.id` 恢复 session id；
- 如果仍没有 id，token event 会使用 `"unknown"` 作为 effective session id。

### 风险

- 多个不可识别文件、且无 `session_meta` 的 source 可能归入同一个 `"unknown"` session。
- Rust 版不应静默把 unknown sessions 聚合到一个稳定 business key。建议内部使用：
  - `SessionId::Known(String)`
  - `SessionId::Unknown { source_fingerprint: String }`
- 兼容模式可继续写 `"unknown"`；严格模式应附加 warning/diagnostic。

---

# 4. JSONL envelope 与状态机

## 4.1 最小 envelope 假设

Parser 只假设每行是 JSON object，且一般结构类似：

```json
{
  "type": "event_msg",
  "timestamp": "2026-06-27T12:34:56Z",
  "payload": {
    "type": "token_count"
  }
}
```

实际 JSON shape 是 upstream Codex 私有日志格式，可能变动。重构必须把“log format adapter”当作 versioned boundary。

## 4.2 当前 parser adapter

- adapter version：`codex-jsonl-v2`
- Python `ParserAdapter` 的核心接口：
  - `parse_file()`
  - `parse_file_with_state()`
- stateful parse 输出：
  - `events: list[UsageEvent]`
  - `diagnostic_facts: list[DiagnosticFact]`
  - `state: ParserState`

## 4.3 ParserState：必须持久化的状态

当前 `ParserState` 包含：

```text
session_id
session_meta
current_turn
last_cumulative_total
call_origin_segment
diagnostic_facts_segment
latest_record_id
latest_event_timestamp
```

### 字段含义

| 字段 | 用途 | 为什么不能删除 |
|---|---|---|
| `session_id` | session identity | append continuation 不必重新从文件开头寻找 `session_meta`。 |
| `session_meta` | thread source、subagent、parent metadata | token event 可能远在 `session_meta` 之后。 |
| `current_turn` | turn_id、timestamp、cwd、model、effort、date、timezone | token event 本身不保证携带这些字段。 |
| `last_cumulative_total` | 单调 token counter 去重 | 防重复统计。 |
| `call_origin_segment` | user/tool/compaction/activity 信号 | 归因发生在下一条 token event。 |
| `diagnostic_facts_segment` | event interval 内的安全 diagnostics | 绑定到下一条成功 usage event。 |
| `latest_record_id` | metadata/status | source file state reporting。 |
| `latest_event_timestamp` | metadata/status | source file state reporting。 |

## 4.4 状态机输入分类

每行发生以下逻辑：

```text
decode UTF-8 + JSON
  ├─ fail → invalid_json++ ; continue
  ├─ payload 非 object → missing_payload++ ; continue
  ├─ type == session_meta
  │   ├─ 更新 session_id（仅当当前为空）
  │   └─ 更新 session_meta；continue
  ├─ type == turn_context
  │   └─ 覆盖 current_turn；continue
  ├─ type != event_msg 或 payload.type != token_count
  │   ├─ 提取 call-origin flags
  │   ├─ 提取 safe diagnostic facts
  │   ├─ 非已知 event_msg 类型 → unknown_event_shape++
  │   └─ continue
  └─ event_msg/token_count
      └─ 生成 UsageEvent（若通过完整校验）
```

## 4.5 `session_meta`

从 `session_meta.payload` 读取：

- session `id`
- `thread_source`
- `source.subagent`
- `source.subagent.other`
- `source.subagent.thread_spawn`：
  - `agent_role`
  - `agent_nickname`
  - `parent_thread_id`（存入 `parent_session_id`）

如果 `parent_thread_id` 可在 `session_index` 查到，进一步补齐：

- `parent_thread_name`
- `parent_session_updated_at`

### 注意

`session_meta` 不是 per-call field，而是“当前 source/session 的长期状态”。在 Rust 中应使用不可变 snapshot + 受控 replace，不要把 raw `serde_json::Value` 一路传到数据库层。

## 4.6 `turn_context`

`turn_context.payload` 覆盖当前 turn 状态：

| source field | persisted event field |
|---|---|
| `turn_id` | `turn_id` |
| envelope `timestamp` | `turn_timestamp` |
| `cwd` | `cwd` |
| `model` | `model` |
| `effort` | `effort` |
| `current_date` | `current_date` |
| `timezone` | `timezone` |

**关键语义：**当前实现是“最后一次 turn_context wins”。后续 token events 继承它，直到新的 `turn_context` 出现。

## 4.7 非 token event 的已知类型

Python 当前声明的 `KNOWN_NON_TOKEN_EVENT_MSG_TYPES`：

```text
agent_message
context_compacted
image_generation_end
item_completed
mcp_tool_call_begin
mcp_tool_call_end
patch_apply_end
skill_completed
skill_invoked
skill_selected
skill_started
skill_used
task_complete
task_started
thread_goal_updated
thread_rolled_back
turn_aborted
user_message
web_search_end
```

未知 `event_msg` 类型不会中断解析，但会增加 `unknown_event_shape`。

### Rust 设计

不要为未知事件建立 hard enum 并 decode failure。建议：

```rust
enum EnvelopeClass {
    SessionMeta,
    TurnContext,
    TokenCount,
    KnownNonToken(KnownEventKind),
    Unknown { entry_type: Option<String>, payload_type: Option<String> },
}
```

unknown 应可观测，而非 crash。

---

# 5. token_count → UsageEvent 的完整转换契约

## 5.1 只有这一种事件生成 usage row

必需条件：

```text
envelope.type == "event_msg"
payload.type  == "token_count"
```

其他 event 只影响 parser state、origin segment、diagnostic facts。

## 5.2 预期 payload shape

概念上：

```json
{
  "type": "event_msg",
  "timestamp": "...",
  "payload": {
    "type": "token_count",
    "info": {
      "last_token_usage": {
        "input_tokens": 0,
        "cached_input_tokens": 0,
        "output_tokens": 0,
        "reasoning_output_tokens": 0,
        "total_tokens": 0
      },
      "total_token_usage": {
        "input_tokens": 0,
        "cached_input_tokens": 0,
        "output_tokens": 0,
        "reasoning_output_tokens": 0,
        "total_tokens": 0
      },
      "model_context_window": 0
    },
    "rate_limits": {
      "plan_type": "...",
      "limit_id": "...",
      "primary": {
        "used_percent": 0.0,
        "window_minutes": 0,
        "resets_at": 0
      },
      "secondary": {
        "used_percent": 0.0,
        "window_minutes": 0,
        "resets_at": 0
      }
    }
  }
}
```

## 5.3 Required vs optional fields

### 必需

`info.last_token_usage`：

- `input_tokens`
- `cached_input_tokens`
- `output_tokens`
- `reasoning_output_tokens`
- `total_tokens`

`info.total_token_usage`：

- `input_tokens`
- `cached_input_tokens`
- `output_tokens`
- `reasoning_output_tokens`
- `total_tokens`

### 可选

- `model_context_window`
- `rate_limits`
- `rate_limits.plan_type`
- `rate_limits.limit_id`
- primary / secondary subfields
- `timestamp`（当前 Python 允许 missing，写空字符串）
- model / effort / cwd / turn id 等 context fields。

## 5.4 数字解析规则

当前 Python `_strict_int()`：

- 接受：
  - integer
  - 非空、可 `int()` 转换的 string
- 拒绝：
  - bool
  - float
  - blank string
  - 非数字 string
  - missing/null（对 required 字段）

当前 Python `_strict_float()`：

- 接受：
  - int
  - float
  - 非空 numeric string
- 拒绝：
  - bool
  - blank string
  - 非数字 string

### 重构要求

Rust 不应让 serde 的宽松 coercion 隐式决定行为。应显式实现：

```rust
fn strict_i64(value: &RawJsonValue) -> Result<i64, ParseValueError>;
fn nullable_i64(value: Option<&RawJsonValue>) -> Result<Option<i64>, ParseValueError>;
fn strict_f64(value: &RawJsonValue) -> Result<f64, ParseValueError>;
```

并保持：

- `true` / `false` 永不被当作 `1` / `0`；
- `1.0` 不应被当作 int；
- 计数最好使用 `u64` 或 `i64`，但写 SQLite 前必须处理 overflow；
- 不要把缺失 token 字段默认为 0，因为会制造伪 usage row。

## 5.5 单调 cumulative counter 去重

当前规则：

```text
if cumulative_total <= last_cumulative_total:
    duplicate_cumulative_total += 1
    skip event
```

其中 `cumulative_total` 来自：

```text
info.total_token_usage.total_tokens
```

这意味着：

- 初始 `last_cumulative_total = -1`；
- `cumulative_total` 必须严格递增；
- 等于前一个总量：跳过；
- 小于前一个总量：也跳过；
- parser 不用 cumulative totals 自行计算 delta；
- per-call token 直接来自 `last_token_usage`；
- `total_token_usage` 主要用于累计展示、单调校验、identity。

### 必须保留的结论

**不要把 `last_token_usage` 替换为 `total_token_usage - previous_total`。**

原因：

- 当前实现将 per-call 字段的唯一权威源定义为 `last_token_usage`；
- upstream 可能有 retry/duplicate/event shape 不稳定；
- 自行差分会改变输出与 Python 基线不一致；
- `total_tokens` 不是应用端应重新推导的派生值。

## 5.6 UsageEvent 字段分组

### Identity / source

- `record_id`
- `session_id`
- `event_timestamp`
- `source_file`
- `line_number`

### Session / thread metadata

- `thread_name`
- `session_updated_at`
- `thread_key`
- `thread_call_index`
- `previous_record_id`
- `next_record_id`
- `is_archived`

### Turn metadata

- `turn_id`
- `turn_timestamp`
- `cwd`
- `model`
- `effort`
- `current_date`
- `timezone`

### Call origin

- `call_initiator`
- `call_initiator_reason`
- `call_initiator_confidence`

### Agent hierarchy

- `thread_source`
- `subagent_type`
- `agent_role`
- `agent_nickname`
- `parent_session_id`
- `parent_thread_name`
- `parent_session_updated_at`

### Token counters

- `input_tokens`
- `cached_input_tokens`
- `output_tokens`
- `reasoning_output_tokens`
- `total_tokens`
- `cumulative_input_tokens`
- `cumulative_cached_input_tokens`
- `cumulative_output_tokens`
- `cumulative_reasoning_output_tokens`
- `cumulative_total_tokens`

### Rate-limit observations

- `rate_limit_plan_type`
- `rate_limit_limit_id`
- `rate_limit_primary_used_percent`
- `rate_limit_primary_window_minutes`
- `rate_limit_primary_resets_at`
- `rate_limit_secondary_used_percent`
- `rate_limit_secondary_window_minutes`
- `rate_limit_secondary_resets_at`

### Derived aggregate fields

- `uncached_input_tokens = max(input_tokens - cached_input_tokens, 0)`
- `cache_ratio = cached_input_tokens / input_tokens`，input ≤ 0 时为 0
- `reasoning_output_ratio = reasoning_output_tokens / output_tokens`，output ≤ 0 时为 0
- `context_window_percent = input_tokens / model_context_window`，window missing/0 时为 0

## 5.7 record_id 规则

当前 record id：

```text
sha256(
  session_id
  + "|"
  + (turn_id or "")
  + "|"
  + event_timestamp
  + "|"
  + cumulative_total_tokens
  + "|"
  + total_tokens
)
```

### 设计含义

- identity 是 logical usage observation，而不是 `(source_file, line_number)`；
- repeated refresh / source reparse 可以 upsert 而非重复插入；
- source file path 不在 identity 内；
- line number 不在 identity 内；
- 内容重新排列但字段相同仍映射为同一 record。

### 风险与建议

- 若 upstream 在同一 session、同 turn、同 timestamp、同 cumulative/last total 产生两个真不同 event，会折叠；
- 当前设计把这种情况视作不可区分/重复；
- Rust v1 compatibility mode 必须完全复刻；
- 未来 major schema 可引入 `event_fingerprint_v2`，但不能在未迁移的 DB 上默默更换 primary key。

---

# 6. 线程、subagent 与调用归因

## 6.1 thread key 规则

当前优先级：

```text
thread_name 有值           → "thread:<thread_name>"
否则 parent_thread_name 有值 → "thread:<parent_thread_name>"
否则 parent_session_id 有值  → "session:<parent_session_id>"
否则                     → "session:<session_id>"
```

### 含义

- 最优先使用 human-readable `thread_name`；
- subagent 会尽可能挂到 parent thread；
- 无 parent metadata 时 fallback 到 session；
- 使用 `thread_name` 作为 key 表示同名但不相关线程可能合并，这是现有语义。

### Rust 建议

内部建议保留结构化 thread identity：

```rust
enum ThreadKey {
    NamedThread(String),
    ParentThread(String),
    ParentSession(String),
    OwnSession(String),
}
```

写 SQLite 时再序列化成兼容字符串。这样能避免业务层到处解析 `"thread:"` / `"session:"` 前缀。

## 6.2 thread adjacency

每次 usage event upsert 后，Python 会全表重新计算：

- `thread_call_index`
- `previous_record_id`
- `next_record_id`

排序键：

```text
PARTITION BY coalesce(nullif(thread_key, ''), 'session:' || session_id)
ORDER BY event_timestamp, cumulative_total_tokens, line_number, record_id
```

### 重构要求

- 排序键必须 deterministic；
- timestamp 相同必须有 tie-breakers；
- 否则 dashboard “前一条/后一条” 跳转会不稳定；
- 在大库上全表重算会昂贵，可改为 affected thread incremental relink，但先保持 correctness。

## 6.3 call origin 分类

当前只读取 metadata shape，不读取文本内容。

### 信号提取

| 信号 | 条件 |
|---|---|
| `user_message` | `event_msg/user_message`；或 `response_item/message` 且 role=user |
| `compaction` | top-level `compacted`；或 `event_msg/context_compacted` |
| `tool_result` | payload type 为 `function_call_output` / `tool_search_output`；或 `event_msg/mcp_tool_call_end` |
| `codex_activity` | `event_msg/agent_message` / `mcp_tool_call_begin`；或非 user 的 `response_item` message/reasoning/function_call/tool_search_call |

### 优先级

```text
user_message        → initiator=user,  reason=user_message,       confidence=high
compaction          → initiator=codex, reason=post_compaction,    confidence=high
tool_result         → initiator=codex, reason=tool_result,        confidence=high
codex_activity      → initiator=codex, reason=agent_continuation, confidence=medium
no signal           → initiator=unknown, reason=no_signal,        confidence=low
```

### 重要语义

`call_origin_segment` 是 **两个 token counts 之间的 event segment**。当遇到 token_count 时：

1. 对 accumulated flags 分类；
2. 清空 segment；
3. 若 token record valid，则把 origin 写进 usage event。

这并不是“查看 token_count 自身的发起方”。

## 6.4 migrated/fallback origin

对老 DB 行，若 persisted origin 缺失：

- model 为 `codex-auto-review`
- 或 `thread_source == subagent`
- 或有 `subagent_type`
- 或有 `parent_session_id`

则 fallback：

```text
codex / thread_source / medium
```

否则：

```text
unknown / missing_origin / low
```

Rust migration layer应明确区分：

- `origin_observed`
- `origin_inferred`
- `origin_unknown`

不要让 downstream 把 inference 当成 primary evidence。

---

# 7. Diagnostic Facts：安全聚合诊断

## 7.1 目的

Diagnostic facts 不是存 prompt 或 command text，而是把某些可安全分类的 event 转为 aggregate facts，例如：

- context compaction；
- patch applied；
- task completed；
- turn aborted；
- MCP call；
- web search；
- image generation；
- function/tool labels；
- shell command family；
- skill labels；
- derived loop patterns。

它们最终关联到一个 `record_id`，并使用：

```text
evidence_scope = "between_token_counts"
raw_content_included = 0
```

## 7.2 基础 event → fact 映射

| event | fact type | fact name | category | confidence |
|---|---|---|---|---|
| `event_msg/context_compacted` | `compaction` | `post_compaction` | `context` | high |
| `event_msg/patch_apply_end` | `outcome` | `patch_applied` | `patch` | high |
| `event_msg/task_complete` | `outcome` | `task_complete` | `task` | high |
| `event_msg/thread_rolled_back` | `outcome` | `thread_rolled_back` | `failure` | high |
| `event_msg/turn_aborted` | `outcome` | `turn_aborted` | `turn` | high |
| `event_msg/mcp_tool_call_end` | `tool` | `mcp_tool_call_end` | `mcp` | medium |
| `event_msg/web_search_end` | `tool` | `web_search_end` | `search` | medium |
| `event_msg/image_generation_end` | `tool` | `image_generation_end` | `media` | medium |
| `response_item/function_call` | `tool` | `function_call` | `function` | low |
| `response_item/function_call_output` | `tool` | `function_call_output` | `function` | medium |
| `response_item/tool_search_call` | `tool` | `tool_search_call` | `search` | low |
| `response_item/tool_search_output` | `tool` | `tool_search_output` | `search` | medium |

## 7.3 Structured safe labels

当前实现允许保留有限的 structured label，但刻意不保存 tool args/output：

- function/tool name；
- skill label；
- shell command family，而不是原 command；
- 可能的 MCP tool label。

safe label 正则：

```text
[A-Za-z0-9_.:-]{1,80}
```

### 实施原则

- **存类别，不存原文。**
- `git status` 可变成 `git` 或 `git_status` 类别；
- 不存 branch name、path、arguments、release note、patch text；
- 不应将 command string、MCP payload、tool output 写入 DB；
- 任何“为了调试方便”加的 JSON blob 都要经过 privacy design review。

## 7.4 Fact merge

同一 diagnostic segment 中相同 `(fact_type, fact_name)` 的 facts 应合并：

- event_count 累加；
- first timestamp 取最早；
- last timestamp 取最晚；
- source line 做 min/max；
- confidence 取最高；
- record_id 在绑定 usage event 时写入。

### Rust 模型建议

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FactKey {
    fact_type: FactType,
    name: String,
}

struct DiagnosticFactDraft {
    key: FactKey,
    category: Option<String>,
    count: u32,
    confidence: Confidence,
    first_timestamp: Option<DateTime>,
    last_timestamp: Option<DateTime>,
    first_line: Option<u64>,
    last_line: Option<u64>,
}
```

## 7.5 绑定语义

当前逻辑：

- 非 token events 产生 fact drafts；
- 下一条 **成功生成的 usage event** 会吸收 drafts；
- `record_id` 被赋给这些 facts；
- segment 清空。

这意味着 diagnostic facts 是“发生在上一 token count 和当前 token count 之间”的归因，不是严格“一条 raw event 对应一条 model call”。

---

# 8. SQLite 数据模型与索引契约

## 8.1 Schema version

当前：

```text
SCHEMA_VERSION = 10
```

migration 语义：

| 版本 | 作用 |
|---|---|
| v1 | 建 `usage_events` 与 `refresh_meta` |
| v2 | migration checksum metadata |
| v3 | call origin columns |
| v4 | dashboard helper columns |
| v5 | `thread_summaries` |
| v6 | `source_files` |
| v7 | `parser_state_json` |
| v8 | observed rate-limit snapshot fields |
| v9 | `call_diagnostic_facts` |
| v10 | `diagnostic_snapshots` |

## 8.2 `usage_events`

它是核心事实表。

### Primary key

```text
record_id TEXT PRIMARY KEY
```

### 不可空 token / identity columns

- `session_id`
- `event_timestamp`
- `source_file`
- `line_number`
- per-call token counters
- cumulative token counters
- derived ratios/counters

### 重要索引

- session
- event timestamp
- model + effort
- thread name
- parent thread/session
- total tokens
- `(is_archived, event_timestamp)`
- `(is_archived, model, effort)`
- `(thread_key, event_timestamp, cumulative_total_tokens)`
- observed rate-limit timestamp partial index。

## 8.3 `source_files`

| 字段 | 当前含义 |
|---|---|
| `source_file_id` | `sha256(str(path))`；注意：不是 content hash。 |
| `source_file` | 字符串路径，unique。 |
| `source_file_hash` | 同样为 path hash。 |
| `is_archived` | 0/1。 |
| `size_bytes` | 上次 parse 后 source file size。 |
| `mtime_ns` | 上次 parse 后 mtime。 |
| `parsed_until_line` | 已解析行数。 |
| `parsed_until_byte` | 已解析 byte offset。 |
| `latest_record_id` | 最新 usage event id。 |
| `latest_event_timestamp` | 最新 event time。 |
| `parser_adapter` | format parser version。 |
| `parser_diagnostics_json` | nonzero diagnostics map。 |
| `parser_state_json` | aggregate-only continuation state。 |
| `last_indexed_at` | index timestamp。 |

## 8.4 `call_diagnostic_facts`

主键：

```text
(record_id, fact_type, fact_name)
```

外键：

```text
record_id → usage_events(record_id) ON DELETE CASCADE
```

### 关键点

- fact 与 usage event 的关联可级联删除；
- full source replace 时先删除该 source 的 facts，再删 usage rows；
- incremental upsert 时会先删除当前 record ids 的 existing facts，再插入新的 facts；
- 因此 facts 无法长期“残留”在一个被重新解析的 record 上。

## 8.5 `thread_summaries`

是 materialized aggregate，不是 source of truth。

每次重建：

1. `DELETE FROM thread_summaries`
2. 写 active scope
3. 写 all-history scope

含主要聚合：

- call count / session count；
- token sums；
- estimated cost / usage credits placeholders；
- average cache ratio；
- max context percent；
- recommendation score / recommendation text；
- origin summary；
- archived count。

### 当前启发式推荐

```text
context_window_percent >= 0.90 → high_context_use (score 100)
cache_ratio < 0.20 && input_tokens >= 50,000 → low_cache_reuse (score 80)
total_tokens >= 100,000 → large_calls (score 70)
```

Rust 版应将这些 thresholds 移出 SQL string，放入 versioned policy module 或 config，但 v1 compatibility mode 的默认值必须一致。

## 8.6 `refresh_meta`

key/value metadata，记录：

- latest refresh time；
- scanned files；
- parsed events；
- skipped events；
- inserted/updated events；
- parser adapter；
- schema version；
- usage schema checksum；
- parsed/skipped source file count；
- 各 parser diagnostic counter。

## 8.7 `diagnostic_snapshots`

on-demand report snapshot：

- `section`
- `history_scope`
- `payload_json`
- `computed_at`
- source logs scanned
- usage rows scanned
- raw content included flag（默认 0）

这块在 Rust Phase 1 可先保留 schema/compatibility read，但不必第一阶段重写所有 report algorithms。

---

# 9. 刷新、增量解析与事务语义

## 9.1 Refresh 总流程

```text
find_session_logs()
load_session_index()
connect(db)
init_db()
source_logs_requiring_parse()
close db

for each parse plan:
    parse source (outside db transaction)
    collect events/facts/state/diagnostics

upsert_usage_events(...)
record_source_file_metadata(...)
record_refresh_metadata(...)
return RefreshResult
```

## 9.2 SourceParsePlan

当前 plan：

```text
path
start_byte
start_line
initial_state
replace_existing
```

### 决策表

| 条件 | parse strategy | replace existing |
|---|---|---|
| source 在 `source_files` 中不存在 | 从头 parse | 是 |
| adapter version 不同 | 从头 parse | 是 |
| `parser_state_json` 无法恢复 | 从头 parse | 是 |
| size 与 mtime 均未变 | skip | 不适用 |
| size 变大，且 `0 < parsed_until_byte <= previous_size` | 从 byte cursor append parse | 否 |
| 其他情况：truncate / rewrite / metadata 不可靠 | 从头 parse | 是 |

## 9.3 Append-only continuation

append plan 会：

- `seek(start_byte)`；
- 从 `start_line + 1` 开始枚举行号；
- 用 persisted `ParserState` 作为 initial state；
- 只 upsert 新 events；
- 不删除旧 source rows；
- 更新 source metadata/cursor。

### 这一设计为什么必要

如果只存 byte offset、不存 parser state：

- 新 token event 会丢失 model/effort/cwd；
- subagent/parent metadata 会丢失；
- cumulative de-dup 会失效；
- call-origin segment 跨刷新边界会丢失；
- diagnostic facts 跨刷新边界会丢失。

## 9.4 Source rewrite

当 source 不满足 append-only 条件时：

1. 删除该 `source_file` 关联的 `call_diagnostic_facts`；
2. 删除该 source 的 `usage_events`；
3. 从头解析；
4. insert/upsert 新 rows；
5. 重建 links 与 thread summaries；
6. 写新的 source metadata。

### 当前实现的事务事实

- `upsert_usage_events` 在一个 SQLite connection/transaction 中完成：
  - source replacement delete；
  - usage upsert；
  - fact delete；
  - fact insert；
  - link refresh；
  - thread summary rebuild。
- `record_source_file_metadata` 是后续独立 transaction。
- `record_refresh_metadata` 又是后续独立 transaction。

### 崩溃影响

| 崩溃时点 | 影响 |
|---|---|
| parse 前 | 无 DB 改动 |
| upsert 前 | 无 DB 改动 |
| usage transaction 内 | rollback |
| usage commit 后、source metadata 前 | 下一次可能重 parse / re-upsert；通常是冗余工作，不应丢 usage |
| source metadata 后、refresh meta 前 | data 正确，status metadata 滞后 |

### Rust 建议

更强的设计是把以下内容放进一个 transaction：

```text
replace/delete + usage_events + facts + links + summaries + source cursor + refresh metadata
```

但需要注意：parser 本身必须在 transaction 之外完成，避免长时间 write lock。

可采用：

```text
parse → validate → BEGIN IMMEDIATE → optimistic source-version check → apply atomically → COMMIT
```

如果 metadata 在 parse 后发生变化（文件继续追加/改写），按 policy：

- 接受 parser snapshot，但不要超过 last complete newline；
- 或 retry planning。

## 9.5 当前实现没有 source tombstone reconciliation

常规 refresh 只处理当前 `find_session_logs()` 返回的 paths。没有看到：

```text
DELETE FROM source_files WHERE source_file NOT IN discovered_paths
DELETE FROM usage_events WHERE source_file NOT IN discovered_paths
```

因此：

- 用户删除/移动 session log 后，SQLite 历史 rows 仍保留；
- 这可能是“保留已索引历史”的设计，也可能是清理缺失；
- Rust 版必须做明确产品决策，而不是继承隐含行为。

建议 options：

```text
--source-retention=keep      # compatibility default
--source-retention=reconcile # 删除失踪 source 的 aggregate rows
--source-retention=mark      # 标记 unavailable，不删数据
```

---

# 10. 隐私、安全与 raw evidence 边界

## 10.1 当前公开隐私承诺

项目的 normal persistence/export/dashboard 路径不保存：

- prompts；
- assistant messages；
- tool output；
- pasted secrets；
- raw transcript content。

它保存 aggregate metrics，如：

- session ids；
- timestamps；
- source paths；
- thread labels；
- cwd/project metadata；
- model / effort；
- token counters；
- pricing/credit annotations；
- derived ratios；
- diagnostics categories。

## 10.2 重要前提：不落库 ≠ 不解码

当前 parser 使用 `json.loads(line)`，即完整 JSON line 进入 Python object graph。故精确表述应是：

```text
原始内容在解析期间可能短暂存在于进程内存；
但不得进入 normal persisted data model、SQLite、CSV、dashboard HTML、
诊断快照、日志或 telemetry。
```

Rust 版同样如此。`serde_json` 不会自动满足隐私；重要的是数据流控制。

## 10.3 Runtime evidence

README 说明 raw evidence：

- 仅在用户显式 action 时按需读取；
- 读取单个本地 JSONL source；
- 应用 common secret pattern redaction；
- cap returned text size；
- 可以用 `--no-context-api` 禁用；
- 支持 privacy modes（如 redacted / strict）。

### Rust 后置设计

把它隔离为独立 crate/module：

```text
evidence:
  SourceLocator
  RawLineReader
  EvidenceSelector
  Redactor
  OutputCapper
  PrivacyModePolicy
```

并明确禁止 `core` / `store` 依赖 evidence 的 raw payload type。

## 10.4 必须写入架构规则的 deny-list

以下类型不得作为 normal DB schema field：

```text
prompt
message_content
assistant_text
tool_output
tool_arguments
command_text
patch_text
diff
clipboard_content
authorization_header
api_key
cookie
bearer_token
private_key
raw_json
raw_payload
raw_envelope
```

### 防回归机制

- schema lint：拒绝 suspicious field names；
- integration test：fixture 含 fake secret，扫描 SQLite、CSV、HTML、JSON API response 不能出现；
- logging policy：debug log 不打印 decoded envelope；
- error reporting：parse errors 记录 error kind + line number，不记录 raw line。

---

# 11. 当前实现的风险、缺陷与改进项

本节刻意区分：

- **兼容性事实**：Python v0.11.4 的确如此；
- **Rust 修复建议**：可在新实现改进，但必须先决定是否需要 compatibility mode。

## 11.1 P0：partial trailing line 可能被吞掉

### 当前行为

- parser 从 `start_byte` seek；
- 对每个 `raw_line` 尝试 decode + JSON；
- 末行若正在写入且不是完整 JSON，记 `invalid_json`，然后 continue；
- metadata 更新时：

```text
parsed_until_byte = current file size
parsed_until_line = _count_lines(path)
```

即 cursor 指向 EOF，而不是最后成功解析的完整 newline boundary。

### 后果

若末行：

```text
{"type":"event_msg","payload":{"type":"token_count", ...
```

尚未写完，第一次 refresh：

- 解析失败；
- cursor 推到末尾；
- 后续追加剩余字节时，从 JSON 中间开始读取；
- 那个 token event 永久无法恢复。

### Rust 修复策略

**首选**：只提交到最后完整 newline 后的 byte offset。

```text
read bytes
for each complete line ending '\n':
  parse
if EOF has non-empty remainder without '\n':
  do not advance committed offset over remainder
```

Alternative：

- 在 parser state 保存 pending raw bytes；
- 下一次 prepend pending bytes；
- 但会让 raw content 短暂进入 cursor persistence，违背 aggregate-only state 目标。

因此不建议 persist pending raw text。只回退 offset 更安全。

### 验收 test

1. 写完整行 A；
2. 写半行 B；
3. refresh；
4. 补全 B + newline；
5. refresh；
6. B 必须被准确解析一次。

## 11.2 P1：incremental parse 仍有 O(file_size) line count

当前每个 parsed source 更新 metadata 时调用：

```text
_count_lines(path)
```

它重新打开并遍历整个文件。

### 后果

即使 parser 从 byte cursor 只读 1 KB append：

- metadata stage 仍可能扫完整 1 GB JSONL；
- 增量优化被部分抵消；
- 高刷新频率时 I/O 明显。

### Rust 修复

在 parse loop 中维护：

```rust
new_line_count = previous_line_count + complete_lines_consumed
```

对 full reparse：

- parse 时自然计数，无需第二遍。

对 append parse：

- 只加本次消费的完整 lines；
- partial tail 不计入 committed line count。

### 验收

- 大 source append 1 line；
- instrument bytes read；
- 必须接近 append size，而不是 full file size。

## 11.3 P1：source_file_hash 名称误导，实际是 path hash

当前：

```text
_source_file_hash(path) = sha256(str(path))
```

`source_file_id` 与 `source_file_hash` 都等于 path hash。

### 后果

- 不是 content fingerprint；
- 若 source 被原地替换、size 相同、mtime 恰好相同，则 refresh 会 skip；
- rename 后会被视为新 source；
- path 作为 primary identity 不等于 source object identity。

### Rust 修复建议

存三类不同概念：

```text
source_id          = stable local path identity (optional)
path_hash           = hash(normalized path)
content_guard       = hash(first/last N bytes + size) 或 inode/file-id + signature
```

不要把它们都叫 `source_file_hash`。

兼容 DB migration：

- 保留 old field；
- 新增 `content_guard_version`, `content_guard`；
- 旧值仅作为 path hash 解释。

## 11.4 P1：缺失 source 不自动清理

如第 9.5 节。需要明确 retention semantics。

## 11.5 P1：无效 token_count 会清空 origin segment，但不清空 diagnostic segment

当前 token_count handling 的顺序是：

1. `classify_call_origin(call_origin_segment)`
2. `call_origin_segment = []`
3. 再验证 `info`、usage fields、cumulative monotonicity
4. 仅成功 build event 后才清空 `diagnostic_facts_segment`

因此如果 token_count 无效或 duplicate：

- origin signals 被丢弃；
- diagnostic drafts 可能继续带到未来某个成功 token event。

### 影响

这会形成 asymmetry：

- origin 可能落到 `unknown`；
- diagnostic facts 可能跨一个无效 token boundary 绑定到更晚的 call。

### Rust 决策必须显式化

可选 policy：

| policy | origin | diagnostics | 说明 |
|---|---|---|---|
| Python-compat | token candidate 时清 origin；只有 valid usage 时清 facts | 保持现状 | 最高兼容 |
| Strict boundary | 任何 token_count 都是 boundary，均清空 | 均清空 | 避免跨边界泄漏 |
| Retryable malformed | 不清任何 segment | 保留到下一个 valid event | 适用于 partial/temporary malformed，但可能跨过真正边界 |

建议：

- Phase 1 支持 `CompatibilityPolicy::Python0114`；
- Phase 2 默认 `StrictBoundary`，同时记录 dropped segment diagnostics。

## 11.6 P1：全表 relink + summary rebuild

每次 upsert 都：

- 对全表计算 window function；
- 更新所有 link fields；
- 删除并重建所有 thread summaries。

数据大时复杂度增长明显。

### Rust 优化方向

- 记录 affected thread keys；
- 只对 affected threads 重新计算 adjacency；
- `thread_summaries` 对 affected scopes delete/insert；
- 仍保留 `rebuild-all` repair command；
- 必须有全量 parity test，防止 incremental relink 与 full relink 出现顺序差异。

## 11.7 P2：文件继续写入与 snapshot race

当前 plan → parse → persist 之间 source 可变化：

- parse 时看见一个内容快照；
- metadata 使用 later `stat()` 与 later `_count_lines()`；
- cursor 可能代表比 parse 数据更晚的文件状态。

partial line 风险也因此更严重。

### Rust 修复

- parse 期间记录:
  - `opened_size`
  - `last_complete_offset`
  - `last_complete_line`
  - `file identity`（inode/file id，若可用）
- persist cursor 用 parser observed boundary，不用末次 `stat().size`；
- commit 前 re-stat：
  - 若 file shrank / identity changed，discard/replan；
  - 若 file grew，仅提交 observed boundary，下一轮 append parse。

---

# 12. Rust 目标架构

## 12.1 Workspace 建议

```text
codex-usage/
├─ crates/
│  ├─ codex-usage-domain/
│  │  ├─ ids.rs
│  │  ├─ usage.rs
│  │  ├─ diagnostics.rs
│  │  ├─ origin.rs
│  │  └─ thread.rs
│  ├─ codex-usage-parser/
│  │  ├─ discovery.rs
│  │  ├─ jsonl_reader.rs
│  │  ├─ adapter_v2.rs
│  │  ├─ state.rs
│  │  ├─ value_decode.rs
│  │  └─ fixtures.rs
│  ├─ codex-usage-store/
│  │  ├─ sqlite.rs
│  │  ├─ migrations.rs
│  │  ├─ source_cursor.rs
│  │  ├─ usage_repo.rs
│  │  ├─ thread_summary.rs
│  │  └─ query.rs
│  ├─ codex-usage-service/
│  │  ├─ refresh.rs
│  │  ├─ reports.rs
│  │  ├─ pricing.rs
│  │  └─ exports.rs
│  ├─ codex-usage-evidence/        # optional/late
│  │  ├─ reader.rs
│  │  ├─ redaction.rs
│  │  └─ privacy.rs
│  ├─ codex-usage-cli/
│  ├─ codex-usage-http/             # optional/late
│  └─ codex-usage-mcp/              # optional/late
├─ fixtures/
│  ├─ python-0.11.4/
│  └─ adversarial/
└─ docs/
```

## 12.2 依赖建议

| 需求 | Rust crate 候选 | 说明 |
|---|---|---|
| JSON | `serde`, `serde_json` | 必需。对未知大字段使用 raw/borrowed decode 策略。 |
| SQLite | `rusqlite` | 本项目主要是 local SQLite、强事务语义、SQL control。 |
| CLI | `clap` | 子命令与 JSON output。 |
| errors | `thiserror`, `anyhow` | library typed errors + binary context。 |
| time | `time` 或 `chrono` | 明确 RFC3339 parsing policy。 |
| file watching | `notify` | 仅作为 wake-up hint，不作为 correctness source。 |
| hashing | `sha2` | record id/path hash compatibility。 |
| testing | `insta`, `proptest`, `tempfile` | golden + property + filesystem tests。 |
| observability | `tracing` | 严禁记录 raw line/payload。 |

## 12.3 依赖边界

```text
domain         ← parser, store, service
parser         ← domain
store          ← domain
service        ← parser, store, domain
cli/http/mcp   ← service
evidence       ← domain; 不允许反向依赖 store raw types
```

特别禁止：

```text
store → serde_json::Value(raw envelope)
usage row → raw JSON blob
reports → source JSONL direct read（除 evidence-specific path）
```

---

# 13. Rust parser/ingestion 详细设计

## 13.1 核心 trait

```rust
pub trait LogAdapter {
    fn version(&self) -> &'static str;

    fn parse(
        &self,
        source: &mut dyn std::io::BufRead,
        context: ParseContext<'_>,
        cursor: ParseCursor,
        state: ParserState,
    ) -> Result<ParsedSource, ParseError>;
}
```

### Parse input

```rust
pub struct ParseContext<'a> {
    pub path: &'a std::path::Path,
    pub session_index: &'a SessionIndex,
    pub archive_status: ArchiveStatus,
    pub policy: &'a ParserPolicy,
}
```

### Parse output

```rust
pub struct ParsedSource {
    pub events: Vec<UsageEvent>,
    pub diagnostic_facts: Vec<DiagnosticFact>,
    pub next_state: ParserState,
    pub committed_cursor: CommittedCursor,
    pub diagnostics: ParseDiagnostics,
    pub observed_file: ObservedFileSnapshot,
}
```

## 13.2 Cursor 设计

```rust
pub struct CommittedCursor {
    pub byte_offset: u64,
    pub line_number: u64,
    pub complete_line_only: bool,
    pub adapter_version: String,
}
```

### 不变量

- `byte_offset` 只能指向完整 line boundary；
- `line_number` 只计算已经完整处理的 lines；
- `byte_offset` 与 `line_number` 必须来自同一次 parse scan；
- 不能用 parse 后的 `metadata.len()` 覆盖 cursor；
- parser state 与 cursor 必须同 transaction commit。

## 13.3 Line reader

建议实现：

```rust
loop {
    let start_offset = offset;
    buf.clear();
    let n = reader.read_until(b'\n', &mut buf)?;

    if n == 0 { break; }

    offset += n as u64;

    if !buf.ends_with(b"\n") {
        // EOF trailing partial line
        // do not commit offset/line
        diagnostics.partial_trailing_line += 1;
        break;
    }

    line_no += 1;
    parse_complete_line(&buf, line_no, ...);
    committed_offset = offset;
    committed_line_no = line_no;
}
```

### 注意

- JSONL 定义中 `\r\n` 也要支持；
- UTF-8 invalid 必须有 diagnostic；
- 超长单行必须有 max length 防止 memory bomb；
- max line 超限策略必须显式：skip / fail source / evidence-only；
- 不能 `fs::read(path)` 全量读入内存。

## 13.4 Envelope decode

性能与隐私兼顾的建议：

1. 先 stream line；
2. JSON decode top-level；
3. 尽可能只取：
   - top-level type
   - timestamp
   - payload.type
   - structured scalar fields；
4. 对可能含大量 raw content 的 fields 避免 clone；
5. parsed event 结束后立即 drop raw decoded value。

可以采用两层：

```rust
#[derive(Deserialize)]
struct EnvelopeHeader<'a> {
    #[serde(borrow)]
    r#type: Option<Cow<'a, str>>,
    #[serde(borrow)]
    timestamp: Option<Cow<'a, str>>,
    #[serde(borrow)]
    payload: Option<&'a RawValue>,
}
```

随后按 `payload.type` 选择性 decode。

### 原则

- 不要为所有 event 建一个大而脆弱的强类型 union；
- 对 token_count、session_meta、turn_context 建强类型；
- 对其他 event 使用 selective extraction；
- unknown event 保留 category/diagnostic，不留 raw payload。

## 13.5 Domain model 草图

```rust
pub struct UsageEvent {
    pub record_id: RecordId,
    pub session_id: SessionId,
    pub thread_name: Option<String>,
    pub session_updated_at: Option<String>,
    pub event_timestamp: String,
    pub source_file: String,
    pub line_number: u64,

    pub turn: TurnMetadata,
    pub origin: CallOrigin,
    pub archive_status: ArchiveStatus,
    pub thread: ThreadMetadata,
    pub agent: AgentMetadata,
    pub counters: TokenCounters,
    pub cumulative: TokenCounters,
    pub observed_rate_limit: Option<RateLimitObservation>,
}
```

### 类型化 enums

```rust
pub enum CallInitiator { User, Codex, Unknown }
pub enum OriginReason { UserMessage, PostCompaction, ToolResult, AgentContinuation, NoSignal, ThreadSource, MissingOrigin }
pub enum Confidence { Unknown, Low, Medium, High }
pub enum ArchiveStatus { Active, Archived }
pub enum FactType { Compaction, Outcome, Tool, Function, McpTool, Skill, CommandFamily, DerivedLoop, Other(String) }
```

保存到 SQLite 时映射为 compatibility strings。

## 13.6 Parser policy

必须显式 versioned：

```rust
pub enum ParserPolicy {
    Python0114Compatibility,
    StrictV1,
}
```

至少控制：

- invalid token boundary 的 segment clear 行为；
- unknown session fallback；
- partial trailing line；
- timestamp missing；
- duplicate cumulative behavior；
- unsupported number coercion；
- source rewrite detection。

## 13.7 Refresh service

```rust
pub trait UsageStore {
    fn initialize(&mut self) -> Result<(), StoreError>;
    fn source_plan(&self, discovered: &[SourceFile]) -> Result<Vec<SourceParsePlan>, StoreError>;
    fn apply_refresh(&mut self, refresh: ParsedRefresh) -> Result<RefreshResult, StoreError>;
}
```

`apply_refresh()` 必须在单个 transaction 内：

```text
BEGIN
  verify source versions if applicable
  delete replaced source rows/facts
  upsert usage rows
  replace/upsert facts
  update event links
  rebuild affected summaries
  upsert source cursor + state
  update refresh metadata
COMMIT
```

---

# 14. 数据库迁移与兼容策略

## 14.1 方案选择

### 方案 A：Rust 直接读写 Python DB

优点：

- 双实现并行验证；
- dashboard/CLI 可逐步切换；
- user 不需要立即 rebuild。

缺点：

- schema compatibility 成本高；
- Python 未来 schema 变化需要监听；
- Rust 团队受 legacy columns/semantics 束缚。

### 方案 B：Rust 使用新 DB

优点：

- 设计清晰；
- cursor/source identity 可修复；
- migration 控制更自由。

缺点：

- 迁移期需要 dual-index；
- dashboard/API 需要同时迁移；
- 用户磁盘多一个 DB。

### 推荐：双层策略

```text
Phase 1：Rust compatible writer / reader，支持 Python schema v10
Phase 2：新增 Rust metadata fields（不破坏 Python）
Phase 3：可选 Rust-native DB v2 + importer
```

## 14.2 兼容写入规则

Rust v1 必须保持：

- `record_id` 算法一致；
- `usage_events` column names 与 types 一致；
- `thread_key` string format 一致；
- numeric null/default semantics 一致；
- diagnostic fact key 一致；
- event ordering一致；
- archived active/all-history summary scope 名称一致；
- source metadata adapter version 是可识别的。

## 14.3 推荐新增字段

在不破坏旧 reader 的前提下，未来可增：

```text
cursor_boundary_kind
cursor_observed_size_bytes
cursor_observed_mtime_ns
content_guard_version
content_guard
source_file_identity
parse_policy_version
partial_tail_seen_at
last_refresh_error_kind
```

## 14.4 Migration discipline

- migration 只能 append/repair；
- 重大 identity 改动需要新 table / migration，不就地改 primary key；
- 每个 migration 需要：
  - forward migration test；
  - existing DB upgrade test；
  - downgrade/read-only fallback policy；
  - data-count invariant test；
  - privacy scan test。

---

# 15. 测试、golden fixtures 与验收标准

## 15.1 Golden fixture 结构

```text
fixtures/
├─ python-0.11.4/
│  ├─ basic/
│  │  ├─ source.jsonl
│  │  ├─ session_index.jsonl
│  │  ├─ expected_events.json
│  │  ├─ expected_facts.json
│  │  ├─ expected_state.json
│  │  └─ expected_sqlite_dump.json
│  ├─ subagent/
│  ├─ archived/
│  ├─ malformed/
│  ├─ incremental/
│  └─ rate_limits/
└─ adversarial/
   ├─ partial_tail/
   ├─ huge_line/
   ├─ invalid_utf8/
   ├─ size_mtime_collision/
   ├─ rename/
   ├─ truncate/
   └─ delete_source/
```

## 15.2 必测用例

### Basic parser

- `session_meta → turn_context → token_count`;
- token event without turn context；
- session id 从 filename 得到；
- session id 从 session meta 得到；
- filename 不可识别；
- missing timestamp；
- unknown event type；
- missing payload；
- invalid JSON；
- invalid UTF-8。

### Token accounting

- 计数为 int；
- numeric string；
- invalid float；
- bool；
- blank string；
- missing last usage；
- missing total usage；
- missing cumulative total；
- duplicate cumulative total；
- decreasing cumulative total；
- cached input > input（derived uncached clamp to 0）；
- zero input / zero output ratios。

### State continuation

- parse whole file 一次；
- parse prefix + persist state + parse suffix；
- output events/facts/state 必须等价；
- continuation 时 `turn_context` 可继承；
- continuation 时 `session_meta` 可继承；
- continuation 时 `last_cumulative_total` 可去重；
- continuation 时 call-origin segment 可继承；
- continuation 时 diagnostic segment 可继承。

### Thread/subagent

- named thread；
- parent thread；
- parent session fallback；
- own session fallback；
- subagent other；
- thread_spawn role/nickname；
- missing parent session index；
- same thread name collision（验证 current behavior）。

### Call origin

- user message → user/high；
- compaction → codex/high；
- tool result → codex/high；
- activity → codex/medium；
- no signal → unknown/low；
- priority collision：user + compaction 应 user；
- invalid token count boundary 的 current Python behavior；
- strict policy 的 intended behavior。

### Diagnostics

- each mapped event；
- safe tool label；
- unsafe label rejected；
- command family only，不保存 command；
- duplicate facts merge；
- fact timestamps/lines min/max；
- fact binding to successful token event；
- source replacement 删除 old facts；
- re-upsert record replacement facts。

### Incremental source behavior

- unchanged size + mtime skip；
- append parse from exact cursor；
- truncate triggers full replace；
- adapter version mismatch triggers full replace；
- invalid parser state triggers full replace；
- partial trailing line；
- file grows during parse；
- same size + same mtime content mutation；
- source delete / rename retention policy。

### DB

- migration from empty；
- migration from v1-v10 fixture；
- upsert idempotence；
- replacement deletes source-only rows；
- foreign key cascade；
- thread links deterministic；
- active and all-history summary values；
- transaction crash simulation；
- busy/locked SQLite behavior。

### Privacy

- fake API key/prompt/tool output embedded in fixture；
- ensure not in:
  - DB dump；
  - export CSV；
  - normal JSON response；
  - static dashboard HTML；
  - error logs；
  - diagnostics snapshot；
- evidence path only returns redacted/capped text。

## 15.3 Differential testing

建议 Python baseline 作为 oracle：

```text
fixture JSONL
  → Python v0.11.4 expected JSON/SQLite
  → Rust output JSON/SQLite
  → canonicalizer
  → structural diff
```

### Canonicalize 规则

- stable sort events by `record_id`；
- normalize absolute paths；
- normalize generated timestamps；
- normalize SQLite row order；
- 不忽略 token count、thread/origin/fact fields；
- 任何业务字段差异必须 fail。

## 15.4 验收标准

Phase 1 “core ready” 的最低条件：

1. 100% golden fixture parity；
2. incremental split/continuation 与 full parse 等价；
3. partial trailing line test 通过；
4. no raw secret leaks in normal persistence；
5. DB migrations deterministic；
6. refresh repeated N 次后 usage/facts row counts稳定；
7. new implementation does not read full file for small append（performance assertion）；
8. `cargo test --workspace`、clippy、fmt、cross-platform test 通过。

---

# 16. 性能与运行时设计

## 16.1 正确的 benchmark 问题

不要问：

> Rust 比 Python 快多少？

应问：

- 每次 append 1 行时读取了多少字节？
- 10 GB source 下 peak RSS 是否受限？
- 全量 parse 每百万行耗时？
- SQLite transaction 耗时与 lock duration？
- thread summary rebuild 在 1M events 下耗时？
- dashboard/API query 的 p95？
- 文件 watcher wake-ups 是否导致重复 refresh？

## 16.2 内存模型

正确目标：

```text
O(max_line_size + parser_state + batch_size)
```

而不是：

```text
O(total_source_file_size)
```

Rust 生态里已有报告显示，某些 usage tools 曾用 `fs::read()` 把整个 Codex session JSONL 载入内存，导致 multi-GB session 失败。这个项目应以 streaming reader 为硬性要求。

## 16.3 Batch strategy

建议：

- parser 输出可分批；
- SQLite upsert batch size configurable；
- 每批保持 source cursor 不提交，直到 source parse + write policy完成；
- 若要支持非常大文件：
  - 避免 `Vec<UsageEvent>` 无限制积累；
  - 可使用 source-local batches；
  - 但 diagnostics segment / parser state 需要保留。

### 简化 Phase 1

可先保持“每 file collect then transaction”模式，但写明 max file memory 风险；随后迭代 batch writer。

## 16.4 WAL / SQLite

Python current behavior：

- connect timeout 5 seconds；
- `busy_timeout = 5000`；
- 尝试 `journal_mode = WAL`，失败抑制 database error；
- normal context manager commit / exception rollback。

Rust 建议：

- WAL 保持；
- busy timeout configurable；
- explicit foreign keys：`PRAGMA foreign_keys=ON`；
- checkpoint policy 不应在每 refresh 强制；
- 执行 query 与 write 分 connection；
- source parse 不持有 DB write transaction。

## 16.5 Watcher

`notify` 可以做 UX 优化，但不能成为 correctness mechanism：

- FSEvents/inotify/ReadDirectoryChangesW 可能丢事件、合并事件；
- debounce 必需；
- 仍需 periodic reconciliation 或 explicit manual refresh；
- active file 末尾 partial line 必须由 parser cursor 保护，而不是 watcher timing 保护。

---

# 17. 可参考的 Rust 项目与采用结论

以下结论仅说明“可借鉴点”，不等于可直接复用。

| 项目 | 可借鉴 | 不足/不能替代 |
|---|---|---|
| `codexusage` crate | Codex/Claude usage CLI 的 Rust 方向 | 未证实覆盖本项目的 SQLite source cursor、thread summary、privacy contract、MCP/dashboard feature set。 |
| `coding_agent_usage_tracker` / `caut` | 多 provider CLI、Rust distribution、quota/rate limits | 定位为跨 provider usage monitor，不是本项目的 aggregate warehouse。 |
| `codex-linux-usage-tray-indicator` | size+mtime 文件 cache、token_count 聚合、local-only pattern | 无 SQLite aggregate index、persistent parser state、thread graph、facts pipeline。 |
| `tokenusage` | Rust CLI/TUI/GUI、Codex/Claude unified UX | 产品边界不同，需审查其 source/log privacy 和 parser semantics。 |
| `Codex Trace` | JSONL session viewer、multiple formats、live tail、collaboration chains | 它面向 raw session viewing/search；与本项目的 aggregate-only privacy model 不同。 |
| `ccusage` Rust implementation | 大日志 streaming 的反例与改进方向 | 曾出现全文件读取内存问题；可作为“不要这样做”的性能回归用例。 |

## 17.1 推荐采用策略

- **借鉴**：
  - CLI ergonomics；
  - streaming reader；
  - multi-provider abstraction（仅当未来产品需要）；
  - Tauri/GTK UI（仅 UI phase）。
- **不要直接依赖其 parser 作为核心**：
  - JSONL schema 变更、session attribution、privacy policy 是项目特定的；
  - dependency 引入会把 parity control 交给外部项目；
  - 粒度与 data contracts 通常不匹配。

---

# 18. 实施路线与交付物

## Phase 0：冻结与取证

交付：

- baseline source tarball + commit pin；
- Python fixture exporter；
- expected events/facts/source state/SQLite dumps；
- ADR：scope、compatibility policy、privacy policy；
- benchmark corpus definition。

完成标准：

- 任意开发者可重复从 baseline 生成 fixtures；
- CI 可以校验 fixture checksum。

## Phase 1：Rust domain + parser parity

交付：

- `codex-usage-domain`；
- `codex-usage-parser`；
- v0.11.4 compatibility policy；
- streaming JSONL reader；
- parser state JSON codec；
- golden tests。

完成标准：

- full parse parity；
- append continuation parity；
- malformed input diagnostics parity；
- no database yet也能输出 canonical events/facts/state。

## Phase 2：SQLite compatible store

交付：

- schema v10 compatible migrations；
- source plan；
- usage/fact upsert；
- links/thread summaries；
- refresh metadata；
- `refresh`, `inspect-log`, `summary`, `query` CLI。

完成标准：

- Python DB 可读；
- Rust write 后 Python dashboard 可读，或至少 query-level parity；
- idempotent refresh；
- crash/retry invariant。

## Phase 3：安全/正确性修复开关

交付：

- complete-line cursor；
- content guard；
- source retention policy；
- affected-thread incremental rebuild；
- strict parser policy。

完成标准：

- 修复项都有 migration / feature flag / compatibility story；
- 不会在 silent mode 改变历史 totals。

## Phase 4：reports、pricing、exports

交付：

- reports domain；
- CSV / JSON contract；
- optional pricing/rate-card；
- support bundle。

完成标准：

- machine-readable output stable；
- sensitive data leakage tests。

## Phase 5：HTTP/dashboard/MCP/evidence

交付：

- HTTP API；
- dashboard；
- MCP tools；
- evidence reader + redaction；
- privacy modes。

完成标准：

- normal operation does not require raw evidence；
- evidence opt-in；
- no raw transcript in generated static assets。

---

# 19. 不变量、决策清单与最终建议

## 19.1 必须写进代码和测试的不变量

1. **每条 persisted usage row 必须来自有效 `event_msg/token_count`。**
2. **per-call token fields 来自 `last_token_usage`；不得自行差分 cumulative counter。**
3. **cumulative total 必须严格递增，否则不产生 usage row。**
4. **parser continuation 必须恢复 session meta、turn context、origin segment、diagnostic segment、last cumulative。**
5. **cursor 只能提交到完整 line boundary。**
6. **normal persistence 不得保存 raw prompt/message/tool output/args。**
7. **record id 算法在 compatibility mode 下必须完全一致。**
8. **thread ordering必须 deterministic。**
9. **same input refresh 多次必须幂等。**
10. **source replace 必须删除旧 source 的 usage rows 与 associated facts。**
11. **source cursor/state 与 rows 应在同一个 commit unit 内一致。**
12. **任何错误 telemetry 不得包含 raw line。**

## 19.2 需要产品/技术负责人明确决定的事项

| 决策 | 选项 | 建议 |
|---|---|---|
| 保持 Python DB compatibility | Yes / No | Yes，至少 Phase 1-2。 |
| 处理 partial tail | Python legacy / fixed | fixed，提供 compat flag。 |
| missing source retention | keep / mark / delete | 默认 keep 或 mark；删除必须显式。 |
| unknown session id | literal `"unknown"` / per-source anonymous | internal per-source anonymous，legacy export可映射。 |
| invalid token boundary | Python asymmetric / strict boundary | 先 compatibility，再 strict default。 |
| source fingerprint | path only / content guard | content guard。 |
| thread key | legacy string / typed internal | typed internal + legacy serializer。 |
| UI migration | immediate / late | late。 |
| multi-provider scope | Codex only / extensible | domain 预留 provider，Phase 1 只 Codex。 |

## 19.3 最终建议

结论不是“把 Python 改成 Rust”。

准确的结论是：

> 将项目重构为一个**以 aggregate-only、可恢复 parser state、完整行 cursor、强 schema contract 为中心**的 local event ingestion engine；Rust 是实现该 engine 的合适语言，但 UI、MCP、plugin 都应是后置 adapter。

优先级必须是：

```text
1. 语义与隐私正确性
2. 增量解析正确性
3. SQLite 一致性与可恢复性
4. Golden parity
5. 大日志 bounded-memory
6. Query/report parity
7. Dashboard/MCP/UI
8. 微优化
```

最危险的错误不是跑得慢，而是：

- 少记一条尾部 token event；
- 重复累计；
- 把 subagent 归错 parent thread；
- 把 diagnostics 挂到错误 call；
- 为了便捷把 prompt/tool output 偷偷落库；
- 因 cursor/state 不一致而产生 silent drift。

---

# 20. 参考来源

## 原项目基线

1. PyPI release `codex-usage-tracking 0.11.4`  
   https://pypi.org/project/codex-usage-tracking/

2. GitHub release/tag source  
   https://github.com/douglasmonsky/codex-usage-tracker/tree/v0.11.4

3. Baseline source commit  
   https://github.com/douglasmonsky/codex-usage-tracker/commit/55265365cccdf27b2f05202766b951946a547c9a

4. Parser implementation  
   https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/parser.py

5. Source-plan / cursor metadata implementation  
   https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/store_sources.py

6. SQLite orchestration  
   https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/store.py

7. SQLite migrations  
   https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/store_schema.py

8. Persisted usage schema  
   https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/schema.py

9. Call-origin classification  
   https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/call_origin.py

10. Diagnostic facts  
    https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/diagnostic_facts.py

11. Thread summaries  
    https://raw.githubusercontent.com/douglasmonsky/codex-usage-tracker/v0.11.4/src/codex_usage_tracker/store_thread_summaries.py

## Rust ecosystem references

12. coding_agent_usage_tracker / caut  
    https://github.com/Dicklesworthstone/coding_agent_usage_tracker

13. Codex Linux Usage Tray  
    https://github.com/wojciechsacewicz/codex-linux-usage-tray-indicator

14. tokenusage  
    https://github.com/hanbu97/tokenusage

15. Codex Trace  
    https://github.com/PixelPaw-Labs/codex-trace

16. ccusage issue on whole-file read memory risk  
    https://github.com/ccusage/ccusage/issues/1124

17. codexusage crate  
    https://crates.io/crates/codexusage

---

## 附录 A：Python v0.11.4 parser diagnostics keys

```text
invalid_json
missing_payload
unknown_filename_format
unknown_event_shape
missing_info
missing_last_token_usage
missing_total_token_usage
missing_cumulative_total
duplicate_cumulative_total
invalid_integer
partial_field_count
invalid_model_context_window
skipped_events
```

## 附录 B：建议的 Rust refresh 伪代码

```rust
pub fn refresh(store: &mut SqliteStore, codex_home: &Path) -> Result<RefreshResult> {
    let sources = discover_sources(codex_home)?;
    let session_index = load_session_index(codex_home)?;

    let plans = store.plan_sources(&sources)?;

    let mut parsed = Vec::new();
    for plan in plans {
        let result = parser.parse_path(
            &plan.path,
            &session_index,
            plan.cursor,
            plan.state,
        )?;
        parsed.push((plan, result));
    }

    store.apply_refresh_atomically(parsed)
}
```

## 附录 C：partial-tail 安全伪代码

```rust
while reader.read_until(b'\n', &mut line)? != 0 {
    if !line.ends_with(b"\n") {
        diagnostics.partial_trailing_line += 1;
        break; // do not advance persisted cursor
    }

    let line_start = committed_offset;
    let line_end = committed_offset + line.len() as u64;

    parse_line(&line, next_line_no)?;
    committed_offset = line_end;
    committed_line_no = next_line_no;
}
```

