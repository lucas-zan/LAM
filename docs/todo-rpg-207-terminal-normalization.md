# RPG-207: Terminal normalization

- **Status**: 验证成功
- **Depends on**: RPG-204, RPG-205

## Logic design

- 统一 usage 的 input/output/total 与可选 detail；缺失数据不捏造。
- 稳定分类 validation/auth/capability/upstream/timeout/cancelled/internal，仅保留 HTTP status 并脱敏 body。
- retryable 是分类元数据，不在 adapter 中重放 HTTP request。
- 区分 before/after first byte 的 timeout/cancel/disconnect；stream failure 仅一个 failed event。

## Test and acceptance

- [x] 先写 usage/error/retry 红测；首次运行因 `adapters::terminal` 不存在而按预期失败（E0432）。
- [x] full/missing/inconsistent usage；400/401/403/408/409/429/5xx，raw/malformed body 均不解析或暴露。
- [x] Retry-After seconds/date/bound、timeout/cancel/disconnect/overflow 全覆盖。
- [x] Debug/JSON/log-safe serialization 不含 upstream body/token/prompt；focused test 5 passed。

## Done criteria

红测、泄露测试和 focused regression 通过后为 `验证成功`。
