# Todo List

修复两个缺陷：profile 账号的 wrapper 落在不在 PATH 的目录导致终端无法识别命令；
Settings 中的 Handoff terminal 列表只在 App 启动时探测一次，新装的终端（cmux）不出现。

规模：M（Rust 后端 + React 前端，5 个任务）

## 背景

- `wrapper_path` 硬编码 `~/bin`，而 macOS 默认 PATH 不含 `~/bin`，`codex-<name>` 报 command not found。
- 契约文档定义了 `WRAPPER_DIR_NOT_IN_PATH`，roadmap 列了「wrapper 目录 PATH 检查」，均未实现。
- GUI 进程的 `PATH` 是 macOS 精简版，不能用来判断终端里的 PATH，必须经登录 shell 探测。
- `listTerminalTargets()` 在 `loadSettings()` 中调用，而 `loadSettings()` 位于空依赖 `useEffect`，全程只跑一次。

## 任务

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| T1 wrapper 目录解析 | `resolve_wrapper_dir` 按登录 shell PATH 在 `~/.local/bin`、`~/bin` 中选首个命中项，都不命中时回落 `~/bin`；探测结果缓存，支持 `LAM_LOGIN_SHELL_PATH` 覆盖 | 验证成功 |
| T2 wrapper 路径兼容既有位置 | `wrapper_path` 优先返回已存在的 wrapper 文件路径，避免改目录后 rename/delete/repair 找不到旧文件 | 验证成功 |
| T3 迁移旧 wrapper | `repair_managed_wrappers` 把非首选目录中的受管 wrapper 迁移到首选目录并删除旧文件，且保持幂等 | 验证成功 |
| T4 目录不在 PATH 时告警 | 首选目录不在登录 shell PATH 时，创建账号与导入 session profile 的结果 `warnings` 含可执行的 PATH 提示 | 验证成功 |
| T5 Settings 重新探测终端 | 进入 Settings 页时重新调用 `listTerminalTargets`，新安装的终端无需重启 App 即可出现 | 验证成功 |

## 测试

- `account.rs` `mod wrapper_location_tests`：目录选择、路径兼容、告警文案（纯函数，无环境依赖）
- `phase1_core.rs`：新建落盘位置、创建告警、session 导入告警、迁移与幂等
- `stores/app.test.ts`：`refreshTerminalTargets` 更新列表与失败时保留旧值

## 验证命令

| 目的 | 命令 |
|------|------|
| Rust 单元测试 | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml wrapper_dir` |
| Rust 集成测试 | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test phase1_core wrapper` |
| Rust 全量回归 | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` |
| Rust 格式 | `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check` |
| 前端测试 | `cd apps/desktop && npm test` |
| 前端构建 | `cd apps/desktop && npm run build` |

## 验证结果

全部通过：Rust 单元 9/9、`phase1_core` wrapper 用例 8/8、Rust 全量套件无回归、
`cargo fmt --check` 干净、前端 230/230 + lint + build + ui-smoke 均通过。

Clippy 仍报 4 处告警，全部位于本次未改动的 `gateway/supervisor.rs` 与 PAT 切换分支，属既有问题。

## 环境副产物

- `wiremock` 从 0.6.5 降级到 0.6.4（0.6.5 使用 let-chains，需要 rustc 1.88，本机为 1.87）。
- `apps/desktop` 补装了 `package.json` 中已声明但未安装的依赖，否则两个 handoff 测试文件无法加载。

## 已知限制

- 若 `~/.local/bin` 与 `~/bin` 都不在登录 shell PATH 中，wrapper 仍落在 `~/bin`，
  此时依赖 T4 的告警提示用户手动加 PATH；设计上不自动改写 shell rc 文件。
- 登录 shell PATH 探测每个进程只做一次（`OnceLock` 缓存），用户改完 rc 需要重启 LAM 才会重新解析目录。
