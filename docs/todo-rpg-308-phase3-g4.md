# RPG-308: Phase3 G4 gate

- **Status**: 验证成功
- **Depends on**: RPG-304, RPG-305, RPG-306, RPG-307
- **Scope**: mock DeepSeek packaged-runtime E2E、fault/security/retry/recovery gate，并回归 Phase0～3。

## Test and acceptance

- [x] red E2E fixture/scenario verifier 先因缺少 G4 manifest 失败。
- [x] create/dry-run/attach/token/config/launcher/sidecar/fake Codex nonstream/stream/tool 全链路通过。
- [x] UI close、crash/restart、rebind/rotate/drift/detach/old token、foreign port/migration rollback。
- [x] store/config/log/session/wrapper/manifest/journal/frontend secret scan；body/tool/reasoning log capture。
- [x] limits/redirect/auth/control/spoof/path/fault/retry matrix 与 combined attempt budget 通过。
- [x] Rust full tests、frontend tests/lint/format/build、Rust fmt/clippy new-code、fixture checksum/sanitization、`git diff --check` 全绿。
- [x] 更新设计与总 tracker：RPG-301～308 `验证成功`、G4 passed，并记录精确计数/命令。

## Validation evidence

- Rust: `cargo test` — **332 passed, 2 ignored**；全 targets Clippy `-D warnings` 通过。
- Frontend: Vitest **167/167**；ESLint、Prettier check、TypeScript/Vite production build 通过。
- Contract: Phase 0 capture/verifier **15/15**，校验 **12 scenarios / 15 artifacts**。
- G4: component G4 **2/2**；process G4 **1/1** 实际执行临时 packaged-layout 中签名后的
  `lam`/sidecar/helper、读取 config 的 fake Codex 与独立 fake upstream。
- Packaging: 最终 `LAM.app` deep codesign、包内三组件 manifest/hash/codesign；
  `LAM_0.2.1_aarch64.dmg` checksum sidecar、只读挂载、镜像内 app/组件复验均通过。
- Quality: Rust fmt、限定既有基线 lint 后的全 targets Clippy、新产物 secret scan、`git diff --check` 通过。

## Done criteria

只有真实 packaged runtime 路径和 mock DeepSeek gate 均通过，才能声称 Phase3 完成；不接受 placeholder、仅类型存在或 ignored test。

## STOP conditions

- 需要真实 secret/公网 Provider，或验证会覆盖用户 UI/未提交工作。
