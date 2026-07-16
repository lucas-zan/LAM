# Todo: 修复 Gateway 交互会话

> Executor instructions: Execute T1-T3 in order. Write and fail each task's tests before implementation. Do not deliver until every task is 验证成功.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: Codex launcher, Responses passthrough, Gateway observer
- **Category**: bugfix
- **Planned at**: current working tree

## Why this matters

**Background**: A real interactive API-account session can answer a first prompt, but tool execution cannot find ordinary commands, the tool follow-up is rejected by the Responses parser, and structured Gateway request logs appear in the TUI.

**Current state**: `CodexLauncher` clears the environment without restoring `PATH`; `ResponsesInputItem` omits the observed `reasoning` history item; the production sidecar observer prints request metadata to inherited stderr.

**Impact**: Coding tasks stop after the first tool call, and implementation metadata pollutes user-visible conversation output.

**What improves**: `codex-idragon3` behaves like normal Codex across tool calls and multi-turn history, while request metadata remains available in a private Gateway log instead of the terminal.

## Scope

**In scope**:
- Preserve the invoking user's PATH in the sanitized Codex child environment.
- Parse/preserve Responses reasoning history and reject it on incompatible Chat Completions translation.
- Persist request metadata privately without writing it to stderr.
- Real multi-step command verification.

**Out of scope**:
- Changing tool definitions or sandbox policy.
- Logging prompts, tool arguments, response bodies, credentials, or headers.
- Supporting unrelated Responses item types not observed in this failure.

## Design

- Keep `env_clear`; explicitly forward `PATH` alongside the existing allowlisted process variables.
- Add a lossless `Reasoning` input variant using flattened JSON fields for Responses passthrough.
- Chat Completions translation returns `UnsupportedInput` for reasoning history it cannot represent.
- Replace stderr metadata output with append-only JSONL at `<provider-hub>/gateway-requests.jsonl`, mode 0600. Logging failure is non-fatal.
- The observer receives metadata only; no request content or secrets are written.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Preserve PATH | Codex tools can resolve `ls`, `sed`, and user CLI paths | 验证成功 |
| T2 | Accept reasoning follow-up | Responses tool follow-up parses and reaches upstream | 验证成功 |
| T3 | Remove terminal request logs | Request IDs are written privately, not to TUI stderr | 验证成功 |

### T1: Preserve PATH

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Sanitization currently removes the shell command search path.

**What to do**: Add PATH to the explicit Codex child environment allowlist.

**Logic design**: Forward the exact invoking PATH after `env_clear`; do not inherit arbitrary other variables.

**Test design**:
- Normal: launched fake Codex records the same PATH as the parent.
- Edge: args, cwd, and CODEX_HOME remain unchanged.
- Error/security: unrelated secret environment variables remain absent.

**Acceptance**: `cargo test --test provider_gateway_launcher launcher_preserves_path`

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected red failure confirmed
- [x] Focused and relevant tests pass
- [x] Status is 验证成功

### T2: Accept reasoning follow-up

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Codex includes encrypted reasoning history before function call/output items.

**What to do**: Add lossless reasoning parsing and explicit adapter rejection.

**Logic design**: Preserve unknown reasoning fields for passthrough; never expose them in Debug output or approximate them in Chat Completions.

**Test design**:
- Normal: message + reasoning + function call + output parses.
- Edge: encrypted content and extra metadata round-trip.
- Error: Chat translation rejects reasoning at the exact input path.
- Regression: captured function follow-up continues to parse.

**Acceptance**: `cargo test --test provider_adapter_schema --test provider_adapter_request reasoning_followup`

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected red failure confirmed
- [x] Focused and relevant tests pass
- [x] Status is 验证成功

### T3: Remove terminal request logs

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Sidecar stderr is inherited by the launcher and rendered inside Codex TUI.

**What to do**: Add a private JSONL metadata observer and use it in the production sidecar.

**Logic design**: Append one sanitized metadata record per request with owner-only permissions; no stderr on successful observation; ignore log I/O errors.

**Test design**:
- Normal: two observations append two valid JSON lines.
- Edge: file mode is 0600 and metadata fields remain available.
- Error: an unavailable log path does not panic.
- Privacy: log contains no prompt/body/authorization fields.

**Acceptance**: `cargo test --test provider_gateway_server file_metadata_observer`

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected red failure confirmed
- [x] Focused and relevant tests pass
- [x] Real TUI output contains no JSON request metadata
- [x] Status is 验证成功

## Test plan

- Launcher environment allowlist behavior.
- Responses reasoning history parse/round-trip and incompatible adapter error.
- Private metadata JSONL behavior and failure isolation.
- Real tool invocation followed by a model response.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused | Commands in each task | exit 0 after expected red failure |
| Relevant suites | `cargo test --test provider_gateway_launcher --test provider_adapter_schema --test provider_adapter_request --test provider_gateway_routes --test provider_gateway_server` | exit 0 |
| Format/build | `cargo fmt --all -- --check` and `cargo check --release --bins` | exit 0 |
| Real session | `codex-idragon3 exec "Run pwd and ls, then answer with the project name"` | tool succeeds, follow-up succeeds, no request JSON in UI |

## Done criteria

- [x] Every task is 验证成功
- [x] Every task Done criteria is checked
- [x] Relevant suites, formatting, release build, and real session pass
- [x] No STOP condition remains

## STOP conditions

- Fix requires inheriting the complete parent environment.
- Observed follow-up contains secrets that cannot be represented without logging/exposure.
- Metadata cannot be persisted without weakening file permissions.
- Five repair cycles fail for any task.
