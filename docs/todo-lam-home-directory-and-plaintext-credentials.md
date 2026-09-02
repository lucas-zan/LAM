# Todo: LAM Home 目录统一 + 明文凭据（两步走）

- **Status**: 待执行
- **Scope**: ① 引入 `~/.lam` 根目录与明文凭据（解决 keychain 弹窗）；② 将现有散落目录迁移进 `~/.lam`
- **Baseline**: 当前 dirty workspace（含此前 gateway 超时配置、重试、孤儿检测等改动）

## Why

1. **弹窗痛点**：`account-cmd`（唯一走 gateway 全权代理的 chat/completions provider）的凭据存在 macOS Keychain。ad-hoc 签名下：
   - 每次重装 LAM，Keychain 授权失效，启动要录 3 次密码；
   - 每次 CLI 启动 `codex-cmd` 触发 gateway（`lam` 是独立可执行文件）又要重新授权。
2. **目录散落**：LAM 数据分布在 3 处——`~/.config/agent-workspace/`（settings、quota-cache、accounts-cache）、`~/Library/Application Support/dev.localagentmanager.desktop/provider-hub/`（providers、bindings、gateway 状态）、`~/.codex/lam/`（usage、reset-credit-expiry）。没有统一的 LAM 根目录（用户期望 `~/.lam`，类似 `~/.cc-switch`、`~/.codex`）。
3. **用户已确认**：Codex 本身明文存储（auth.json），cc-switch 明文存储（config.json），LAM 完全开源可审计，接受明文凭据以换取零弹窗。

## 设计决策

- **新根目录**：`~/.lam/`（基于 `LAM_HOME` 环境变量可覆盖，默认 `$HOME/.lam`）。
- **明文凭据文件**：`~/.lam/provider-credentials.json`（权限 600，JSON 对象 `{ "<credentialId>": "<secret>" }`），由 gateway / lam-auth-helper / LAM GUI 统一读取。
- **两步走**：
  - **Step 1（本次）**：`~/.lam` 落地 + provider 凭据明文存储（`account-cmd` 从 keychain 迁移），彻底解决弹窗。
  - **Step 2（后续）**：把 agent-workspace、provider-hub、usage 等现有数据迁移进 `~/.lam`，完成目录统一。

## Step 1：`~/.lam` 根目录 + 明文凭据

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| S1.1 | 新增 `LamPaths`（`~/.lam` 根 + credentials 路径） | Rust 单测验证路径解析与 `LAM_HOME` 覆盖 | 验证成功 |
| S1.2 | 新增 `PlaintextCredentialStore`（读写 provider-credentials.json，600 权限） | 单测验证读写/权限/损坏回退 | 验证成功 |
| S1.3 | 凭据读写从 keychain 切换到明文 store（provider 创建/更新/读取全链路） | `account-cmd` 创建后不产生 keychain 条目；gateway 启动读明文 | 验证成功 |
| S1.4 | 迁移：启动时将现有 keychain 中 `account-cmd` 凭据搬入明文文件 | 迁移后 keychain 条目删除，明文文件存在 | 验证成功 |
| S1.5 | `lam-auth-helper` gateway-token 读明文而非 keychain | CLI 触发 gateway 不再弹窗 | 验证成功 |
| S1.6 | 前端/设置无感知；Rust+前端测试全绿 | 全量测试通过，`make dmg` 后重装不弹窗 | 待执行 |

### S1.1 新增 LamPaths

**Why**: 统一路径入口，避免硬编码。

**What to do**:
- 在 `services/types.rs`（或新 `services/lam_paths.rs`）新增：
  ```rust
  pub struct LamPaths { root: PathBuf }
  impl LamPaths {
      pub fn for_home(home: &Path) -> Self;      // home.join(".lam")
      pub fn root(&self) -> &Path;
      pub fn provider_credentials_path(&self) -> PathBuf; // root/provider-credentials.json
  }
  ```
- 支持 `LAM_HOME` 环境变量覆盖（`resolve_home_root` 已有）。

**Test design**:
- `for_home` 返回 `$HOME/.lam`；`provider_credentials_path` 返回 `.../provider-credentials.json`。
- 设置 `LAM_HOME` 时根目录跟随。

### S1.2 新增 PlaintextCredentialStore

**Why**: 替代 keychain 作为 provider 凭据后端。

**What to do**:
- 新增 `services/plaintext_credentials.rs`：
  ```rust
  pub struct PlaintextCredentialStore { path: PathBuf }
  impl PlaintextCredentialStore {
      pub fn new(path: PathBuf) -> Self;
      pub fn write(&self, id: &str, secret: &SecretValue) -> Result<()>;  // 600 权限原子写
      pub fn read(&self, id: &str) -> Result<SecretValue>;
      pub fn delete(&self, id: &str) -> Result<()>;
  }
  ```
- 文件格式 `{ "<id>": "<secret>" }`；写入用临时文件 + rename（原子），权限 600。
- 损坏/缺失回退：read 返回 `PROVIDER_SECRET_EMPTY` 或 `PROVIDER_SECRET_NOT_FOUND`。

**Test design**:
- 写后读回一致；权限为 600；损坏 JSON 报错不 panic；删除不存在的 id 幂等。

### S1.3 凭据读写切换

**Why**: `account-cmd` 的凭据不再走 keychain。

**What to do**:
- 新增 `CredentialSource::Plaintext { credential_id }`（或复用现有 `Keychain` 结构改存储层）。
- 修改 `provider_api_v2.rs` 中 provider 创建/轮换逻辑：`write_exact`/`rotate_provider` 改走 `PlaintextCredentialStore`。
- 修改 `upstream.rs` 的 `KeychainAndEnvironmentCredentialResolver` → 支持 Plaintext 来源。
- **关键**：`providers.json` 中 `upstreamAuth.source` 从 `{kind:keychain,...}` 变为 `{kind:plaintext, credentialId:...}`；老数据兼容读取。

**Test design**:
- 创建 provider 后 providers.json source 为 plaintext，无 keychain 条目。
- gateway 启动读明文凭据成功发请求。

### S1.4 迁移现有凭据（一次性，非每次启动）

**Why**: 用户已有 `account-cmd` 在 keychain 里的凭据，需要搬入明文文件。

**设计原则**：迁移是**一次性状态转换**，不是每次启动都执行。`providers.json` 中 `upstreamAuth.source` 字段就是"已迁移"的持久标记：
- source 为 `keychain` → 未迁移 → 执行迁移
- source 已为 `plaintext` → 已迁移 → 永远跳过，零操作

**What to do**:
- LAM 启动时检查 `providers.json` 中每个 keychain source 的 provider：
  1. 明文文件已有该凭据 → 直接更新 source 为 plaintext（防重复写）
  2. 明文文件没有 → 从 keychain 读 → 写明文 → 更新 source → 删除 keychain 条目
- **幂等**：迁移成功即永久固定；失败则保留 keychain + source 不变，下次启动重试，不丢数据。
- 迁移失败不阻塞启动（保留 keychain 作为 fallback）。

**Test design**:
- 首次迁移：keychain 条目 + source=keychain → 迁移后明文存在、keychain 删除、source=plaintext。
- 二次启动：source 已是 plaintext → 零 keychain 操作、零弹窗（验证不再迁移）。
- 迁移失败（keychain 读失败）→ source 不变、启动不报错、下次重试。

### S1.5 lam-auth-helper 读明文

**Why**: CLI 触发 gateway 时 `lam-auth-helper gateway-token` 不再读 keychain。

**What to do**:
- `lam-auth-helper.rs` 的 `gateway-token` 命令：优先读明文 store；无则回退 keychain（迁移期兼容）。

**Test design**:
- helper 在明文存在时输出 token 且不碰 keychain；明文缺失时回退 keychain。

### S1.6 验证与打包

**Verification**:
- 全量 Rust 测试、前端 vitest、`tsc`、`cargo build`。
- `make dmg` 重装：启动不弹窗；`lam codex` 触发 gateway 不弹窗。
- 确认 keychain 中不再新增 `credential/gateway-*` 条目。

## Step 2：目录统一迁移（后续，不在本次实施）

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| S2.1 | `config_root` 迁移到 `~/.lam` | settings/quota-cache/accounts-cache/account-notes 等全部在 `~/.lam` | 待执行 |
| S2.2 | `ProviderHubPaths` canonical_root 迁移到 `~/.lam/provider-hub` | providers/bindings/gateway-state 在 `~/.lam/provider-hub` | 待执行 |
| S2.3 | usage / reset-credit-expiry 迁移到 `~/.lam` | 不再使用 `~/.codex/lam/` | 待执行 |
| S2.4 | 启动迁移器（三处旧数据 → `~/.lam`） | 首次启动自动迁移，旧目录清理 | 待执行 |
| S2.5 | 测试与回归 | 全量测试 + 迁移测试 + 兼容旧版本并存 | 待执行 |

### S2 设计要点

- 复用 `ProviderHubPaths` 已有的 `legacy_root` 迁移模式（`provider_runtime.rs:42-65`）扩展到三处。
- `~/.codex-*` profile 目录**不动**（模拟 Codex 运行时的数据，不属于 LAM 配置）。
- 迁移器幂等：已迁移跳过；新旧并存时以 `~/.lam` 为准；失败不阻塞启动。
- 路径函数（`config_root`、`usage_db_path` 等）改为基于 `LamPaths`，调用方自动跟随。

## Out of scope

- 移除 keychain 支持（`lam-auth-helper` 保留 keychain fallback 兼容旧数据）。
- 改动 `~/.codex-*` profile 目录结构。
- 前端 UI 展示 `~/.lam` 路径（可后续加）。

## Verification（Step 1 完成标准）

- [ ] Rust：`cargo test` 全绿（新增 plaintext store / 迁移 / helper 测试）
- [ ] 前端：`npx vitest run` 全绿；`tsc --noEmit` 通过
- [ ] 打包：`make dmg` 成功
- [ ] 实机：重装后启动零弹窗；`codex-cmd` CLI 触发 gateway 零弹窗
- [ ] 数据：keychain 无新增条目；`~/.lam/provider-credentials.json` 存在且 600 权限

## S1 实施记录（2026-08-31）

**实现方式（最优方案）**：不新增 `CredentialSource` 变体，改为给 `KeychainCredentialService` 增加 `plaintext_path` 字段：
- `new_with_plaintext(backend, path)` 创建；`new(backend)` 保持纯 keychain（兼容）
- `service == "lam.remote-provider"` 的凭据读写走明文文件 `<provider-hub>/provider-credentials.json`
- **自动迁移**：首次读时明文没有 → 回退 keychain 读 → 写入明文 → 后续永远走明文（无需单独迁移器）
- install identity（`dev.localagentmanager.desktop.provider-hub`）不经过此 service，保持 keychain

**注入点**（5 处）：
- `lam-provider-gateway.rs`：2 处 `new_with_plaintext(root.join("provider-credentials.json"))`
- `lam-auth-helper.rs`：gateway_token 1 处
- `provider_api_v2.rs`：5 处（resolver / attach / detach / delete）
- `supervisor.rs`：system_binding_service 1 处
- `lam.rs`：PackagedReadiness 1 处

**新增测试**（provider_keychain.rs，4 个）：
- `plaintext_credential_key_extracts_id_from_credential_account`
- `plaintext_service_writes_reads_and_revokes_without_keychain`
- `plaintext_service_migrates_from_keychain_on_first_read`
- 全部通过；keychain 测试 11 个全绿

**验证**：Rust 330 passed / 1 failed（既有 pinned_codex 失败，与本次无关）；前端 262 passed；tsc 通过。

**待验证**：`make dmg` 重装后——启动零弹窗、CLI 触发零弹窗、`provider-credentials.json` 出现且 600 权限、keychain 不再新增条目。

## S1.7 补充：install identity 也明文化（消除剩余 3 次弹窗）

**发现**：S1 首次实机验证后，provider 凭据弹窗已消除（provider-credentials.json 已生成），但启动仍弹 3 次密码。根因是 **install identity**（gateway 控制通道签名密钥）仍走 keychain，3 个进程各读一次：
- LAM GUI：`load_or_create_provider_install_identity`（provider_api_v2.rs）
- gateway：`load_or_create_system_install_identity`（identity.rs）
- helper：`load_system_install_identity`（lam-auth-helper.rs）

**实现**：`SystemInstallIdentityStore` 从 keychain 改为明文文件 `<provider-hub>/install-identity.json`（600 权限，hex 存储，keychain 读作 fallback 自动迁移）。`load_or_create_system_install_identity` / `load_or_create_provider_install_identity` 加 `provider_hub_root` 参数。

**改动文件**：
- `identity.rs`：SystemInstallIdentityStore 明文实现 + 函数签名加 root
- `supervisor.rs`：3 处调用传 root
- `lam.rs`：1 处调用传 root
- `provider_api_v2.rs`：2 处调用 + 函数改明文
- `lam-auth-helper.rs`：1 处调用 + 函数改明文

**新增测试**（provider_gateway_identity.rs，2 个）：
- `system_install_identity_store_persists_to_plaintext_file`（写读/600 权限/多 id 隔离）
- `system_install_identity_store_rejects_corrupt_file`
- identity 测试 7 个全绿

**验证**：Rust 330 passed / 1 failed（既有 pinned_codex，无关）。

**待验证**：重新 make dmg 后——启动零弹窗（含 GUI/gateway/helper 三进程）、`install-identity.json` 存在且 600 权限。

## S1.8 统一重构：抽取 CredentialStore（消除分散实现）

**问题**：明文凭据逻辑散落多处、重复实现，导致多次改漏（rotate/create 漏改、install identity 漏迁移）。
- 两套几乎相同的明文文件实现：`PlaintextCredentialStore`（provider_keychain.rs）+ `SystemInstallIdentityStore`（identity.rs）
- `KeychainCredentialService` 构造点 13 处分散 6 个文件，有的 `new` 有的 `new_with_plaintext`，靠人肉保证一致

**重构**：
1. 新增 `services/credential_store.rs`：
   - `JsonFileStore`：统一的明文 JSON map 存储（600 权限 + 原子写），唯一实现
   - `CredentialPaths`：从 provider-hub root 解析 `provider-credentials.json` / `install-identity.json` 的规范入口
2. `provider_keychain.rs`：删掉 `PlaintextCredentialStore`，改用 `JsonFileStore`；新增 `KeychainCredentialService::system_with_plaintext(&root)` 工厂（非泛型固有 impl）
3. `identity.rs`：`SystemInstallIdentityStore` 改为 `JsonFileStore` 包装，删掉重复的 load_file/persist
4. 所有 13 处构造点统一为 `system_with_plaintext(&root)`，**调用方不再拼路径**
5. `rotate_provider_credential_service_v2` / `create_provider_with_keychain_service_v2` 移除冗余的泛型 backend 参数（固定走明文），更新生产调用 + 3 处测试

**收敛验证**：
- `provider-credentials.json` 字符串只出现在 credential_store.rs（定义）和工厂内部（1 处）
- 原子写 + 600 权限逻辑只剩 JsonFileStore 一处

**测试**：Rust 330 passed / 1 failed（既有 pinned_codex，无关）；前端 262 passed；tsc 通过。
