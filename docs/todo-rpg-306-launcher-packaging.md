# RPG-306: Launcher, auth helper and packaging

- **Status**: 验证成功
- **Depends on**: RPG-301, RPG-305
- **Scope**: install manifest、component integrity、`lam codex`、token helper、Tauri bundled binaries。

## Logic design

- versioned manifest 固定 component path/version/protocol/schema/platform/SHA-256/package identity；只解析 bundle/resource 内绝对路径并在 spawn 前验证。
- `lam codex --profile` 加载/恢复 binding projection；Gateway route ensure-ready，Direct route不启动 sidecar；使用 argv 直接 spawn，设置 exact `CODEX_HOME` 并继承 cwd/stdio/signals/exit。
- auth helper 按 binding credential reference 读取且 stdout 只有 token + newline；secret 不进入 argv/env/config/manifest。
- Tauri bundle 配置包含 launcher/sidecar/helper，development 与 packaged 使用同一逻辑 manifest。

## Test and acceptance

- [x] red test：首次运行因 `gateway::launcher` 不存在而按预期失败（E0432/E0433）。
- [x] manifest 对 bundle-relative path、regular executable、owner/mode、SHA-256、platform/arch、component protocol/schema 与 codesign identity 做 fail-closed 验证。
- [x] fake Codex 证明 args（含空格）、cwd、exact `CODEX_HOME` 与 exit code 23 保真；Gateway route ensure-ready，Direct route不启动 sidecar，config/home drift 在 spawn 前失败。
- [x] `lam`、`lam-provider-gateway`、`lam-auth-helper` 三个真实 Rust binary 编译；helper 只输出 active binding token，sidecar 通过 stdin pipe 接收一次性 identity key。
- [x] 完整 Tauri `.app` bundle 成功；final nested codesign → hash manifest → outer sign 顺序通过三项 hash 与 deep codesign；最终 DMG 重建且 `hdiutil verify` 通过。
- [x] launcher/sidecar 11 focused tests、release staging verifier、Node syntax、Rust fmt/diff 通过；未把生成 binary/manifest 纳入 Git。

## STOP conditions

- component 通过 PATH/shell 查找，或 token 必须放 argv/environment。
