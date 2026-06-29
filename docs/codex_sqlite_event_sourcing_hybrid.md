# Codex Session System → SQLite + Event Sourcing Hybrid Architecture

## 1. 设计目标（Design Goals）

将 Codex 当前基于 JSONL append-only session system 改造为：

> **SQLite 作为索引层 + Event Sourcing 作为事实层（source of truth）**

目标：

- 支持高性能查询（token / session / streak）
- 保留 event-sourcing 可回放能力
- 支持跨 session analytics
- 支持 UI 实时统计（Tauri friendly）
- 避免 JSONL scan bottleneck

---

## 2. 总体架构

```
┌──────────────────────────────┐
│        UI Layer (Tauri)      │
│  - charts                   │
│  - heatmaps                │
│  - stats dashboard         │
└──────────────┬───────────────┘
               │ SQL queries
               ▼
┌──────────────────────────────┐
│     SQLite Read Model        │
│  - session_index            │
│  - token_agg_daily          │
│  - turn_index               │
│  - streak_cache             │
└──────────────┬───────────────┘
               │ async sync
               ▼
┌──────────────────────────────┐
│ Event Store (Source of Truth)│
│  - append-only events       │
│  - immutable log           │
│  - replayable state        │
└──────────────────────────────┘
```

---

## 3. Event Store 设计（不可变层）

### 3.1 Event Schema

```sql
Event {
  id TEXT PRIMARY KEY,
  session_id TEXT,
  turn_id TEXT,

  type TEXT,

  payload JSON,

  timestamp INTEGER
}
```

---

### 3.2 Event Types

- session_start
- session_end
- turn_start
- turn_end
- token_usage
- tool_call
- tool_result

---

## 4. SQLite Read Model

## 4.1 session_index

```sql
CREATE TABLE session_index (
    session_id TEXT PRIMARY KEY,
    created_at INTEGER,
    last_active_at INTEGER,
    total_tokens INTEGER,
    total_turns INTEGER
);
```

## 4.2 token_agg_daily

```sql
CREATE TABLE token_agg_daily (
    date TEXT,
    session_id TEXT,
    tokens INTEGER,
    PRIMARY KEY(date, session_id)
);
```

## 4.3 turn_index

```sql
CREATE TABLE turn_index (
    turn_id TEXT PRIMARY KEY,
    session_id TEXT,
    start_ts INTEGER,
    end_ts INTEGER,
    duration_ms INTEGER
);
```

## 4.4 streak_cache

```sql
CREATE TABLE streak_cache (
    user_id TEXT PRIMARY KEY,
    current_streak INTEGER,
    longest_streak INTEGER,
    last_active_date TEXT
);
```

---

## 5. Write Path

Event emitted → append → projector → SQLite update

```rust
fn append_event(event: Event) {
    event_store.append(event);
    projector.notify(event);
}
```

---

## 6. Projector

```rust
fn project(event: Event) {
    match event.type {
        "token_usage" => update_token_daily(event),
        "turn_end" => update_turn_index(event),
        "session_start" => insert_session(event),
        _ => {}
    }
}
```

---

## 7. Heatmap Query

```sql
SELECT date, SUM(tokens)
FROM token_agg_daily
GROUP BY date
ORDER BY date;
```

---

## 8. Streak Query

```sql
SELECT current_streak, longest_streak
FROM streak_cache
WHERE user_id = ?;
```

---

## 9. Replay Mechanism

Event store → rebuild SQLite read model

```bash
replay_events(event_store) → rebuild_sqlite()
```

---

## 10. 总结

Event sourcing + SQLite read model hybrid:

- JSONL = source of truth
- SQLite = query acceleration layer
- projector = sync bridge
